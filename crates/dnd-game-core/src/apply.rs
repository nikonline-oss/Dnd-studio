//! `apply`: intent → events. Pure: never mutates state, never performs I/O.

use dnd_protocol::{
    ActionResolution, Event, Grid, Intent, IntentEnvelope, MAX_CELL_SIZE, MIN_CELL_SIZE, MapDto,
    RejectReason, TokenDto, Version, VersionPolicy,
};

use crate::evolve::evolve;
use crate::permissions::{Actor, authorize};
use crate::rng::GameRng;
use crate::state::GameState;

/// Error returned by [`apply`]; the reason is sent to the client as `Rejected`.
pub type RuleError = RejectReason;

/// Maximum length of user-visible names, in characters.
pub const MAX_NAME_LEN: usize = 200;
/// Maximum length of the campaign description, in characters.
pub const MAX_DESCRIPTION_LEN: usize = 20_000;

/// How many resolution rounds one intent may take (ADR-0052). A round runs the follow-up stages
/// until they stop producing events; a rule that keeps generating new ones would otherwise spin
/// the session. Hitting the limit cancels what is still waiting and is recorded in the trace.
pub const MAX_RESOLUTION_ROUNDS: u32 = 8;

/// How many events one stage produced in one [`apply_traced`], cumulatively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageTrace {
    pub stage: &'static str,
    pub events: usize,
}

/// Deterministic record of what one intent caused (ADR-0052): the stages that fired, how many
/// resolution rounds ran, and whether the step limit was reached. Diagnostics only — it never
/// carries game data and is not part of the protocol.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionTrace {
    pub rounds: u32,
    pub stages: Vec<StageTrace>,
    /// The queue hit [`MAX_RESOLUTION_ROUNDS`]; waiting states were cancelled.
    pub limit_reached: bool,
}

impl ExecutionTrace {
    fn add(&mut self, stage: &'static str, events: usize) {
        if events == 0 {
            return;
        }
        if let Some(entry) = self.stages.iter_mut().find(|entry| entry.stage == stage) {
            entry.events += events;
        } else {
            self.stages.push(StageTrace { stage, events });
        }
    }

    /// Events produced by `stage` (0 if it never fired).
    #[must_use]
    pub fn stage(&self, stage: &str) -> usize {
        self.stages
            .iter()
            .find(|entry| entry.stage == stage)
            .map_or(0, |entry| entry.events)
    }
}

/// [`apply`] with the deterministic trace of its stages (ADR-0052).
pub fn apply_traced(
    state: &GameState,
    envelope: &IntentEnvelope,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Result<(Vec<Event>, ExecutionTrace), RuleError> {
    let mut trace = ExecutionTrace::default();
    // Real-time effects due by the envelope's timestamp expire first (the core has no clock).
    let mut events = crate::effects::expire_by_clock(state, envelope.ts_ms);
    trace.add("clock_effects", events.len());
    let mut work = state.clone();
    for event in &events {
        evolve(&mut work, event);
    }
    let decided = decide(&work, envelope, actor, rng)?;
    trace.add("intent", decided.len());
    events.extend(decided);
    // A turn-end effect can only expire when effects exist or this batch adds one (ADR-0053):
    // the clone is skipped on the hot path otherwise.
    let expired = if work.effects.is_empty()
        && !events
            .iter()
            .any(|event| matches!(event, Event::TokenEffectApplied { .. }))
    {
        Vec::new()
    } else {
        let mut effect_state = work.clone();
        for event in &events {
            evolve(&mut effect_state, event);
        }
        crate::effects::expire_turn_end(&effect_state)
    };
    trace.add("turn_effects", expired.len());
    events.extend(expired);
    let maintained = crate::maintained::cleanup(&work, &events);
    trace.add("maintained", maintained.len());
    events.extend(maintained);
    let bindings = crate::triggers::sync_board_bindings(&work, &envelope.intent, &events, rng)?;
    trace.add("board_bindings", bindings.len());
    events.extend(bindings);
    let reactions = crate::triggers::react(&work, &envelope.intent, &events, envelope.ts_ms, rng);
    trace.add("trigger_reactions", reactions.len());
    events.extend(reactions);
    let followups = crate::mechanics::after(&work, &events, rng);
    trace.add("action_followups", followups.len());
    events.extend(followups);

    // The follow-up stages feed each other: a continuation may be answered by a save roll, whose
    // result resumes a trigger that starts another action. Run them as rounds until they settle
    // instead of repeating individual calls by hand (ADR-0052).
    let mut settled = false;
    while trace.rounds < MAX_RESOLUTION_ROUNDS {
        let before = events.len();
        let maintained = crate::maintained::cleanup(&work, &events);
        trace.add("maintained", maintained.len());
        events.extend(maintained);
        let pending = crate::interruptions::cleanup(&work, &events, envelope.ts_ms, rng);
        trace.add("interruptions", pending.len());
        events.extend(pending);
        let scripts = crate::triggers::resume(&work, &events, envelope.ts_ms, rng);
        trace.add("trigger_resume", scripts.len());
        events.extend(scripts);
        let saves = crate::saves::cleanup(&work, &events, envelope.ts_ms, rng);
        trace.add("saves", saves.len());
        events.extend(saves);
        trace.rounds += 1;
        if events.len() == before {
            settled = true;
            break;
        }
    }
    if !settled {
        trace.limit_reached = true;
        cancel_waiting(&work, &mut events, envelope.ts_ms, rng, &mut trace);
    }

    let items = crate::items::cleanup(&work, &events)?;
    trace.add("items", items.len());
    events.extend(items);
    let summaries = crate::roll_requests::summaries(state, &events, rng);
    trace.add("roll_summaries", summaries.len());
    events.extend(summaries);
    let views = crate::rules_view::after(state, &events);
    trace.add("rules_view", views.len());
    events.extend(views);
    Ok((events, trace))
}

/// Decides whether `envelope` may be applied to `state` by `actor` and returns the resulting events.
///
/// Checks run in this order: permissions, existence, version (ADR-0003), value validation.
/// Apply the returned events with [`crate::evolve`] to obtain the next state.
///
/// Triggers that the intent sets off add their events to the same batch (ADR-0024). The stages
/// and their order are fixed by ADR-0052; use [`apply_traced`] when the diagnostics are needed.
pub fn apply(
    state: &GameState,
    envelope: &IntentEnvelope,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RuleError> {
    apply_traced(state, envelope, actor, rng).map(|(events, _)| events)
}

/// Cancels what is still waiting after the resolution limit (ADR-0052): actions and saves get a
/// cancellation notice, movements and trigger continuations are dropped from the runtime.
fn cancel_waiting(
    before: &GameState,
    events: &mut Vec<Event>,
    at: i64,
    rng: &mut dyn GameRng,
    trace: &mut ExecutionTrace,
) {
    let mut state = before.clone();
    for event in events.iter() {
        evolve(&mut state, event);
    }
    for stack in state.plugin_runtime.action_stacks.clone() {
        if let Ok(cancelled) =
            crate::interruptions::cancel(&state, stack.id, ActionResolution::Invalid, at, rng)
        {
            for event in &cancelled {
                evolve(&mut state, event);
            }
            trace.add("limit_cancelled_actions", 1);
            events.extend(cancelled);
        }
    }
    for group in state.plugin_runtime.saves.clone() {
        if let Ok(cancelled) = crate::saves::cancel(&state, group.id, rng) {
            for event in &cancelled {
                evolve(&mut state, event);
            }
            trace.add("limit_cancelled_saves", 1);
            events.extend(cancelled);
        }
    }
    let dropped =
        !state.plugin_runtime.movements.is_empty() || !state.plugin_runtime.trigger_runs.is_empty();
    if dropped {
        state.plugin_runtime.movements.clear();
        state.plugin_runtime.trigger_runs.clear();
        events.push(Event::PluginRuntimeUpdated {
            runtime: Box::new(state.plugin_runtime.clone()),
        });
        trace.add("limit_dropped_continuations", 1);
    }
}

/// The events of the intent itself.
fn decide(
    state: &GameState,
    envelope: &IntentEnvelope,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RuleError> {
    let intent = &envelope.intent;
    envelope.validate()?;
    authorize(actor, intent, state)?;
    if let Intent::MoveToken { id, .. } | Intent::MoveTokenWithMode { id, .. } = intent
        && (crate::interruptions::busy(state, *id)
            || state
                .tokens
                .get(id)
                .is_some_and(|t| crate::rules_view::map_busy(state, t.map_id)))
    {
        return Err(RejectReason::invalid("pending_action"));
    }
    if let Intent::NextTurn { combat_id: id } | Intent::EndCombat { id } = intent
        && state.combats.get(id).is_some_and(|combat| {
            crate::rules_view::map_busy(state, combat.map_id)
                || state.plugin_runtime.action_stacks.iter().any(|s| {
                    s.map == combat.map_id
                        || s.frames.iter().any(|f| {
                            state.combatants.values().any(|p| {
                                p.combat_id == combat.id && p.character_id == Some(f.owner)
                            }) || state.tokens.values().any(|t| {
                                t.map_id == combat.map_id && t.character_id == Some(f.owner)
                            })
                        })
                })
                || state.plugin_runtime.movements.iter().any(|m| {
                    state
                        .tokens
                        .get(&m.token)
                        .is_some_and(|t| t.map_id == combat.map_id)
                })
        })
    {
        return Err(RejectReason::invalid("pending_movement"));
    }

    if let Some(result) = crate::mechanics::apply(state, envelope, actor, rng) {
        return result;
    }
    if let Some(id) = intent.created_id()
        && id_in_use(state, id)
    {
        return Err(RejectReason::AlreadyExists);
    }

    if intent.version_policy() == VersionPolicy::Checked {
        let current = current_version(state, intent)?;
        match envelope.expected_version {
            None => return Err(RejectReason::MissingExpectedVersion),
            Some(expected) if expected != current => {
                return Err(RejectReason::StaleVersion { current });
            }
            Some(_) => {}
        }
    }

    match intent {
        Intent::ShowImage { asset_id } => {
            let asset = state.assets.get(asset_id).ok_or(RejectReason::NotFound)?;
            if asset.usage == dnd_protocol::AssetUsage::Audio {
                return Err(RejectReason::invalid("asset_id"));
            }
            let version = state
                .presented_image
                .as_ref()
                .map(|p| bump(p.version))
                .transpose()?
                .unwrap_or(Version::INITIAL);
            Ok(vec![Event::ImagePresentationChanged {
                presentation: Some(dnd_protocol::model::PresentedImageDto {
                    id: envelope.intent_id,
                    asset_id: *asset_id,
                    version,
                }),
            }])
        }
        Intent::HideImage { .. } => {
            Ok(vec![Event::ImagePresentationChanged { presentation: None }])
        }
        Intent::CreateItem { .. }
        | Intent::InstantiateItem { .. }
        | Intent::UpdateItem { .. }
        | Intent::TransferItem { .. }
        | Intent::DuplicateItem { .. }
        | Intent::DeleteItem { .. } => crate::items::apply(state, intent)
            .unwrap_or_else(|| Err(RejectReason::invalid("intent"))),
        Intent::ResolveActionSave { .. } | Intent::CancelActionSaves { .. } => {
            Err(RejectReason::invalid("intent"))
        }
        Intent::AdvanceCalendarTime { amount, unit } => {
            crate::game_time::advance_inner(state, crate::calendar::delta(state, *amount, *unit)?)
        }
        Intent::AdvanceGameTime { seconds } => crate::game_time::advance(state, *seconds),
        Intent::UpdateCampaignMeta {
            name,
            description,
            token_inspector,
            item_states,
            visibility,
        } => {
            if name.is_none()
                && description.is_none()
                && token_inspector.is_none()
                && item_states.is_none()
                && visibility.is_none()
            {
                return Err(RejectReason::invalid("intent"));
            }
            let mut campaign = state.campaign.clone();
            if let Some(name) = name {
                campaign.name = valid_name(name, "name")?;
            }
            if let Some(description) = description {
                if description.chars().count() > MAX_DESCRIPTION_LEN {
                    return Err(RejectReason::invalid("description"));
                }
                campaign.description.clone_from(description);
            }
            if let Some(policy) = token_inspector {
                campaign.token_inspector = valid_token_inspector(policy)?;
            }
            if let Some(states) = item_states {
                campaign.item_states = valid_item_states(states)?;
            }
            if let Some(policy) = visibility {
                campaign.visibility = valid_visibility(policy)?;
            }
            campaign.version = bump(campaign.version)?;
            Ok(vec![Event::CampaignMetaUpdated { campaign }])
        }

        Intent::CreateMap {
            id,
            name,
            description,
            background_asset_id,
            width,
            height,
            grid,
            visible_to_players,
        } => {
            if let Some(asset_id) = background_asset_id {
                let asset = state.assets.get(asset_id).ok_or(RejectReason::NotFound)?;
                if asset.usage != dnd_protocol::AssetUsage::Map {
                    return Err(RejectReason::invalid("background_asset_id"));
                }
            }
            let map = MapDto {
                id: *id,
                name: valid_name(name, "name")?,
                description: valid_map_description(description)?,
                background_asset_id: *background_asset_id,
                width: valid_map_size(*width, "width")?,
                height: valid_map_size(*height, "height")?,
                grid: valid_grid(*grid)?,
                visible_to_players: *visible_to_players,
                version: Version::INITIAL,
            };
            Ok(vec![Event::MapCreated { map }])
        }

        Intent::UpdateMap {
            id,
            name,
            description,
            grid,
            visible_to_players,
        } => {
            if name.is_none()
                && description.is_none()
                && grid.is_none()
                && visible_to_players.is_none()
            {
                return Err(RejectReason::invalid("intent"));
            }
            let mut map = state.maps.get(id).cloned().ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                map.name = valid_name(name, "name")?;
            }
            if let Some(description) = description {
                map.description = valid_map_description(description)?;
            }
            if let Some(grid) = grid {
                map.grid = valid_grid(*grid)?;
            }
            if let Some(visible) = visible_to_players {
                map.visible_to_players = *visible;
            }
            map.version = bump(map.version)?;
            Ok(vec![Event::MapUpdated { map }])
        }

        Intent::DeleteMap { id } => Ok(crate::boards::delete_map(state, *id)),

        Intent::CreateToken {
            id,
            map_id,
            character_id,
            asset_id,
            name,
            x,
            y,
            rotation,
            scale,
            layer,
            visible,
            disposition,
        } => {
            let map = state.maps.get(map_id).ok_or(RejectReason::NotFound)?;
            let (x, y) = on_map(map, *x, *y)?;
            if let Some(asset_id) = asset_id
                && !state.assets.contains_key(asset_id)
            {
                return Err(RejectReason::NotFound);
            }
            if let Some(character_id) = character_id
                && !state.characters.contains_key(character_id)
            {
                return Err(RejectReason::NotFound);
            }
            let token = TokenDto {
                id: *id,
                map_id: *map_id,
                character_id: *character_id,
                asset_id: *asset_id,
                name: valid_token_name(name)?,
                x,
                y,
                rotation: valid_rotation(*rotation)?,
                scale: valid_scale(*scale)?,
                layer: *layer,
                disposition: *disposition,
                visible: *visible,
                inspector: None,
                version: Version::INITIAL,
            };
            Ok(vec![Event::TokenCreated { token }])
        }

        Intent::UpdateToken {
            id,
            name,
            rotation,
            scale,
            layer,
            visible,
            disposition,
            inspector,
        } => {
            if name.is_none()
                && rotation.is_none()
                && scale.is_none()
                && layer.is_none()
                && visible.is_none()
                && disposition.is_none()
                && inspector.is_none()
            {
                return Err(RejectReason::invalid("intent"));
            }
            let mut token = state
                .tokens
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                token.name = valid_token_name(name)?;
            }
            if let Some(rotation) = rotation {
                token.rotation = valid_rotation(*rotation)?;
            }
            if let Some(scale) = scale {
                token.scale = valid_scale(*scale)?;
            }
            if let Some(layer) = layer {
                token.layer = *layer;
            }
            if let Some(visible) = visible {
                token.visible = *visible;
            }
            if let Some(disposition) = disposition {
                token.disposition = *disposition;
            }
            if let Some(policy) = inspector {
                let policy = valid_token_inspector(policy)?;
                // An empty policy is how a token goes back to following the campaign.
                token.inspector = (!policy.is_empty()).then_some(policy);
            }
            token.version = bump(token.version)?;
            Ok(vec![Event::TokenUpdated { token }])
        }

        Intent::MoveToken { id, x, y } => {
            let token = state.tokens.get(id).ok_or(RejectReason::NotFound)?;
            let map = state
                .maps
                .get(&token.map_id)
                .ok_or(RejectReason::NotFound)?;
            let (x, y) = on_map(map, *x, *y)?;
            Ok(vec![Event::TokenMoved { id: *id, x, y }])
        }

        Intent::DeleteToken { id } => {
            // Its effects go first (children before the parent, like folders).
            let mut events: Vec<_> = state
                .effects
                .values()
                .filter(|e| e.token == *id)
                .map(|e| Event::TokenEffectRemoved {
                    id: e.id,
                    expired: false,
                })
                .collect();
            events.push(Event::TokenDeleted { id: *id });
            Ok(events)
        }

        Intent::CreateCharacter { .. }
        | Intent::DeleteCharacter { .. }
        | Intent::UpdateCharacter { .. }
        | Intent::SubmitCharacter { .. }
        | Intent::ApproveCharacter { .. }
        | Intent::ReturnCharacter { .. }
        | Intent::ArchiveCharacter { .. }
        | Intent::AssignCharacterOwner { .. } => {
            crate::characters::apply(state, intent, actor, rng)
                .unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::RegisterAsset { .. } => {
            crate::assets::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::ApplyFog { .. } | Intent::ResetFog { .. } => {
            crate::fog::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::StartCombat { .. }
        | Intent::AddCombatant { .. }
        | Intent::RemoveCombatant { .. }
        | Intent::RollInitiative { .. }
        | Intent::SetInitiative { .. }
        | Intent::NextTurn { .. }
        | Intent::EndCombat { .. } => crate::combat::apply(state, intent, actor, rng)
            .unwrap_or(Err(RejectReason::invalid("intent"))),

        Intent::SetConditions { .. } | Intent::GiveItem { .. } => {
            crate::characters::apply(state, intent, actor, rng)
                .unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::SendMessage { .. } | Intent::RollDice { .. } => {
            crate::chat::apply(state, intent, actor, rng)
                .unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateJournalEntry { .. }
        | Intent::UpdateJournalEntry { .. }
        | Intent::MoveJournalEntry { .. }
        | Intent::DeleteJournalEntry { .. }
        | Intent::CreateJournalFolder { .. }
        | Intent::RenameJournalFolder { .. }
        | Intent::MoveJournalFolder { .. }
        | Intent::DeleteJournalFolder { .. } => crate::journal::apply(state, intent, actor)
            .unwrap_or(Err(RejectReason::invalid("intent"))),

        Intent::InstallPlugin { .. }
        | Intent::UninstallPlugin { .. }
        | Intent::MoveTokenWithMode { .. }
        | Intent::ResolvePluginReaction { .. }
        | Intent::CancelPluginMovement { .. }
        | Intent::ResolveActionReaction { .. }
        | Intent::ContinuePluginAction { .. }
        | Intent::CancelActionStack { .. }
        | Intent::EndMaintainedEffect { .. }
        | Intent::ExecutePluginAction { .. }
        | Intent::ConfigurePluginFeature { .. }
        | Intent::SetSystemPlugin { .. } => {
            crate::plugins::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateBoard { .. }
        | Intent::UpdateBoard { .. }
        | Intent::DeleteBoard { .. }
        | Intent::CreateBoardNode { .. }
        | Intent::UpdateBoardNode { .. }
        | Intent::MoveBoardNode { .. }
        | Intent::DeleteBoardNode { .. }
        | Intent::CreateBoardEdge { .. }
        | Intent::UpdateBoardEdge { .. }
        | Intent::DeleteBoardEdge { .. } => {
            crate::boards::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateTrigger { .. }
        | Intent::UpdateTrigger { .. }
        | Intent::DeleteTrigger { .. }
        | Intent::FireTrigger { .. } => crate::triggers::apply(state, intent, envelope.ts_ms, rng)
            .unwrap_or(Err(RejectReason::invalid("intent"))),

        Intent::RemoveTokenEffect { .. } => {
            crate::effects::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::RequestRolls { .. }
        | Intent::FulfillRollRequest { .. }
        | Intent::CancelRollRequest { .. } => {
            crate::roll_requests::apply(state, intent, actor, rng)
                .unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateZone { .. }
        | Intent::UpdateZone { .. }
        | Intent::SetZoneOcclusion { .. }
        | Intent::SetZoneColor { .. }
        | Intent::DeleteZone { .. }
        | Intent::MergeZones { .. } => {
            crate::zones::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateLibraryFolder { .. }
        | Intent::RenameLibraryFolder { .. }
        | Intent::MoveLibraryFolder { .. }
        | Intent::DeleteLibraryFolder { .. }
        | Intent::MoveAsset { .. }
        | Intent::RenameAsset { .. }
        | Intent::PlaySound { .. }
        | Intent::SetSoundPlayback { .. }
        | Intent::StopSound { .. } => {
            crate::library::apply(state, intent).unwrap_or(Err(RejectReason::invalid("intent")))
        }
        Intent::CreatePlaylist { .. }
        | Intent::UpdatePlaylist { .. }
        | Intent::MovePlaylist { .. }
        | Intent::DeletePlaylist { .. } => crate::playlists::apply(state, intent),
    }
}

/// Current version of the entity a checked intent targets.
/// Version of the intent's target, for explicit rebasing and offline projections.
pub fn current_version(state: &GameState, intent: &Intent) -> Result<Version, RejectReason> {
    let version = match intent {
        Intent::ShowImage { .. } => None,
        Intent::HideImage { id } => state
            .presented_image
            .as_ref()
            .filter(|p| p.id == *id)
            .map(|p| p.version),
        Intent::RequestRolls { .. } | Intent::CreatePlaylist { .. } => None,
        Intent::UpdatePlaylist { id, .. }
        | Intent::MovePlaylist { id, .. }
        | Intent::DeletePlaylist { id } => state.playlists.get(id).map(|p| p.version),
        Intent::AdvanceCalendarTime { .. } | Intent::AdvanceGameTime { .. } => {
            Some(state.campaign.game_clock.version)
        }
        Intent::UpdateItem { id, .. }
        | Intent::TransferItem { id, .. }
        | Intent::DeleteItem { id } => state.items.get(id).map(|i| i.version),
        Intent::DuplicateItem { .. } => None,
        Intent::UpdateCampaignMeta { .. } => Some(state.campaign.version),
        Intent::UpdateMap { id, .. } | Intent::DeleteMap { id } => {
            state.maps.get(id).map(|m| m.version)
        }
        Intent::UpdateToken { id, .. }
        | Intent::DeleteToken { id }
        | Intent::MoveToken { id, .. } => state.tokens.get(id).map(|t| t.version),
        Intent::DeleteCharacter { id }
        | Intent::UpdateCharacter { id, .. }
        | Intent::SubmitCharacter { id }
        | Intent::ApproveCharacter { id }
        | Intent::ReturnCharacter { id }
        | Intent::ArchiveCharacter { id }
        | Intent::AssignCharacterOwner { id, .. }
        | Intent::SetConditions { id, .. }
        | Intent::GiveItem { id, .. } => state.characters.get(id).map(|c| c.version),
        Intent::UpdateJournalEntry { id, .. }
        | Intent::MoveJournalEntry { id, .. }
        | Intent::DeleteJournalEntry { id } => state.journal.get(id).map(|e| e.version),
        Intent::ResolveActionSave { .. }
        | Intent::CancelActionSaves { .. }
        | Intent::CreateItem { .. }
        | Intent::InstantiateItem { .. }
        | Intent::CreateMap { .. }
        | Intent::CreateToken { .. }
        | Intent::CreateCharacter { .. }
        | Intent::CreateJournalEntry { .. }
        | Intent::SendMessage { .. }
        | Intent::RollDice { .. }
        | Intent::RegisterAsset { .. }
        | Intent::ApplyFog { .. }
        | Intent::ResetFog { .. }
        | Intent::StartCombat { .. }
        | Intent::AddCombatant { .. }
        | Intent::CreateJournalFolder { .. }
        | Intent::InstallPlugin { .. }
        | Intent::UninstallPlugin { .. }
        | Intent::MoveTokenWithMode { .. }
        | Intent::ResolvePluginReaction { .. }
        | Intent::CancelPluginMovement { .. }
        | Intent::ResolveActionReaction { .. }
        | Intent::ContinuePluginAction { .. }
        | Intent::CancelActionStack { .. }
        | Intent::EndMaintainedEffect { .. }
        | Intent::ExecutePluginAction { .. }
        | Intent::ConfigurePluginFeature { .. }
        | Intent::SetSystemPlugin { .. }
        | Intent::CreateBoard { .. }
        | Intent::CreateBoardNode { .. }
        | Intent::CreateBoardEdge { .. }
        | Intent::CreateZone { .. } => None,
        Intent::UpdateZone { id, .. }
        | Intent::SetZoneOcclusion { id, .. }
        | Intent::SetZoneColor { id, .. }
        | Intent::DeleteZone { id }
        | Intent::MergeZones { into: id, .. } => state.zones.get(id).map(|z| z.version),
        Intent::CreateTrigger { .. } | Intent::FireTrigger { .. } => None,
        Intent::CreateLibraryFolder { .. }
        | Intent::PlaySound { .. }
        | Intent::SetSoundPlayback { .. }
        | Intent::StopSound { .. } => None,
        Intent::RenameLibraryFolder { id, .. }
        | Intent::MoveLibraryFolder { id, .. }
        | Intent::DeleteLibraryFolder { id } => state.library_folders.get(id).map(|f| f.version),
        Intent::MoveAsset { id, .. } | Intent::RenameAsset { id, .. } => {
            state.assets.get(id).map(|a| a.version)
        }
        Intent::UpdateTrigger { id, .. } | Intent::DeleteTrigger { id } => {
            state.triggers.get(id).map(|t| t.version)
        }
        Intent::RemoveTokenEffect { id } => state.effects.get(id).map(|e| e.version),
        Intent::FulfillRollRequest { id } | Intent::CancelRollRequest { id } => {
            state.roll_requests.get(id).map(|r| r.version)
        }
        Intent::MoveBoardNode { .. } => None,
        Intent::UpdateBoard { id, .. } | Intent::DeleteBoard { id } => {
            state.boards.get(id).map(|b| b.version)
        }
        Intent::UpdateBoardNode { id, .. } | Intent::DeleteBoardNode { id } => {
            state.board_nodes.get(id).map(|n| n.version)
        }
        Intent::UpdateBoardEdge { id, .. } | Intent::DeleteBoardEdge { id } => {
            state.board_edges.get(id).map(|e| e.version)
        }
        Intent::RenameJournalFolder { id, .. }
        | Intent::MoveJournalFolder { id, .. }
        | Intent::DeleteJournalFolder { id } => state.journal_folders.get(id).map(|f| f.version),
        Intent::RemoveCombatant { id }
        | Intent::RollInitiative { id }
        | Intent::SetInitiative { id, .. } => state.combatants.get(id).map(|c| c.version),
        Intent::NextTurn { combat_id: id } | Intent::EndCombat { id } => {
            state.combats.get(id).map(|c| c.version)
        }
    };
    version.ok_or(RejectReason::NotFound)
}

/// The GM's visibility switches: plain sheet paths, bounded (ADR-0063). Duplicates are refused,
/// because two rules for one path would make the deepest-path rule ambiguous.
fn valid_visibility(
    policy: &dnd_protocol::VisibilityPolicy,
) -> Result<dnd_protocol::VisibilityPolicy, RejectReason> {
    if policy.sheet.len() > 512
        || policy.sheet.iter().any(|rule| {
            let plain = !rule.path.is_empty()
                && rule.path.chars().count() <= 200
                && rule.path.split('.').all(|part| {
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                });
            !plain
        })
    {
        return Err(RejectReason::invalid("visibility"));
    }
    let mut paths = std::collections::BTreeSet::new();
    for rule in &policy.sheet {
        if !paths.insert(rule.path.clone()) {
            return Err(RejectReason::invalid("visibility"));
        }
    }
    Ok(policy.clone())
}

/// Campaign item states: bounded, plain ids, no duplicates (ADR-0062). An empty list is how the
/// campaign goes back to the built-in states.
fn valid_item_states(
    states: &[dnd_protocol::ItemStateDef],
) -> Result<Vec<dnd_protocol::ItemStateDef>, RejectReason> {
    if states.len() > dnd_protocol::MAX_ITEM_STATES {
        return Err(RejectReason::invalid("item_states"));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut clean = Vec::with_capacity(states.len());
    for state in states {
        let id = state.id.trim();
        if id.is_empty()
            || id.chars().count() > dnd_protocol::MAX_ITEM_STATE_LEN
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !ids.insert(id.to_owned())
        {
            return Err(RejectReason::invalid("item_states"));
        }
        clean.push(dnd_protocol::ItemStateDef {
            id: id.to_owned(),
            label: state.label.clone(),
            active: state.active,
        });
    }
    Ok(clean)
}

/// A token inspector policy with duplicates removed and both lists bounded (ADR-0061).
///
/// `reveal` and `conceal` may name the same field: `conceal` wins, which is how a GM closes a
/// field the base policy or a broad `reveal` opens.
fn valid_token_inspector(
    policy: &dnd_protocol::TokenInspectorPolicy,
) -> Result<dnd_protocol::TokenInspectorPolicy, RejectReason> {
    if policy.reveal.len() > dnd_protocol::TokenField::ALL.len()
        || policy.conceal.len() > dnd_protocol::TokenField::ALL.len()
    {
        return Err(RejectReason::invalid("token_inspector"));
    }
    let clean = |list: &[dnd_protocol::TokenField]| {
        let mut fields: Vec<_> = list.to_vec();
        fields.sort_unstable();
        fields.dedup();
        fields
    };
    Ok(dnd_protocol::TokenInspectorPolicy {
        reveal: clean(&policy.reveal),
        conceal: clean(&policy.conceal),
    })
}

/// The inspector text of a map: trimmed, may be empty, bounded like the campaign description.
fn valid_map_description(description: &str) -> Result<String, RejectReason> {
    let trimmed = description.trim();
    if trimmed.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(RejectReason::invalid("description"));
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn valid_name(name: &str, field: &str) -> Result<String, RejectReason> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_LEN {
        return Err(RejectReason::invalid(field));
    }
    Ok(trimmed.to_owned())
}

/// Token names may be empty (an unnamed prop).
fn valid_token_name(name: &str) -> Result<String, RejectReason> {
    let trimmed = name.trim();
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(RejectReason::invalid("name"));
    }
    Ok(trimmed.to_owned())
}

fn finite(value: f64, field: &str) -> Result<f64, RejectReason> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(RejectReason::invalid(field))
    }
}

fn valid_grid(grid: Grid) -> Result<Grid, RejectReason> {
    let cell_size = finite(grid.cell_size, "grid.cell_size")?;
    if !(MIN_CELL_SIZE..=MAX_CELL_SIZE).contains(&cell_size) {
        return Err(RejectReason::invalid("grid.cell_size"));
    }
    Ok(Grid {
        kind: grid.kind,
        cell_size,
        offset_x: finite(grid.offset_x, "grid.offset_x")?,
        offset_y: finite(grid.offset_y, "grid.offset_y")?,
    })
}

/// Normalized into `[0, 360)`.
fn valid_rotation(rotation: f64) -> Result<f64, RejectReason> {
    Ok(finite(rotation, "rotation")?.rem_euclid(360.0))
}

fn valid_scale(scale: f64) -> Result<f64, RejectReason> {
    let scale = finite(scale, "scale")?;
    if scale <= 0.0 {
        return Err(RejectReason::invalid("scale"));
    }
    Ok(scale)
}

/// Whether any entity — of any kind — already uses `id`. Ids are unique across kinds, so an id
/// never means two things to clients or the database (ADR-0015).
pub(crate) fn id_in_use(state: &GameState, id: dnd_protocol::Uuid) -> bool {
    state
        .roll_requests
        .contains_key(&dnd_protocol::RollRequestId(id))
        || state.items.contains_key(&dnd_protocol::ItemId(id))
        || state.playlists.contains_key(&dnd_protocol::PlaylistId(id))
        || state.maps.contains_key(&dnd_protocol::MapId(id))
        || state.tokens.contains_key(&dnd_protocol::TokenId(id))
        || state
            .characters
            .contains_key(&dnd_protocol::CharacterId(id))
        || state
            .journal
            .contains_key(&dnd_protocol::JournalEntryId(id))
        || state.assets.contains_key(&dnd_protocol::AssetId(id))
        || state
            .fog
            .values()
            .any(|f| f.ops.iter().any(|op| op.id.0 == id))
        || state
            .journal_folders
            .contains_key(&dnd_protocol::JournalFolderId(id))
        || state.combats.contains_key(&dnd_protocol::CombatId(id))
        || state
            .combatants
            .contains_key(&dnd_protocol::CombatantId(id))
        || state.boards.contains_key(&dnd_protocol::BoardId(id))
        || state
            .board_nodes
            .contains_key(&dnd_protocol::BoardNodeId(id))
        || state
            .board_edges
            .contains_key(&dnd_protocol::BoardEdgeId(id))
        || state.zones.contains_key(&dnd_protocol::ZoneId(id))
        || state.triggers.contains_key(&dnd_protocol::TriggerId(id))
        || state.effects.contains_key(&dnd_protocol::EffectId(id))
}

/// Next version, rejected once it would leave the JS-safe range (ADR-0015).
pub(crate) fn bump(version: Version) -> Result<Version, RejectReason> {
    version
        .checked_next()
        .ok_or_else(|| RejectReason::invalid("version"))
}

/// Logical map size, `1..=MAX_MAP_SIZE` (ADR-0016).
fn valid_map_size(size: i64, field: &str) -> Result<i64, RejectReason> {
    if (1..=dnd_protocol::MAX_MAP_SIZE).contains(&size) {
        Ok(size)
    } else {
        Err(RejectReason::invalid(field))
    }
}

/// A position in Units16 must lie on the map: `0..=width*16`, `0..=height*16` (ADR-0016).
pub(crate) fn on_map(map: &MapDto, x: i64, y: i64) -> Result<(i64, i64), RejectReason> {
    let per = dnd_protocol::UNITS16_PER_UNIT;
    if !(0..=map.width.saturating_mul(per)).contains(&x) {
        return Err(RejectReason::invalid("x"));
    }
    if !(0..=map.height.saturating_mul(per)).contains(&y) {
        return Err(RejectReason::invalid("y"));
    }
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_protocol::{IntentId, MovementMode, PendingMovement, PluginRuntime, TokenId};

    /// The limit path drops continuations that are waiting for a person, not a rule (ADR-0052).
    /// Cancelling actions and saves delegates to `interruptions::cancel`/`saves::cancel`, which
    /// their own tests cover.
    #[test]
    fn reaching_the_limit_clears_continuations_with_one_runtime_event() {
        let mut state = GameState::new("Campaign");
        state.plugin_runtime = PluginRuntime {
            movements: vec![PendingMovement {
                id: IntentId::new_v7(),
                token: TokenId::new_v7(),
                destination_x: 0,
                destination_y: 0,
                mode: MovementMode::Voluntary,
                offers: Vec::new(),
                handled: Vec::new(),
            }],
            ..PluginRuntime::default()
        };
        let mut events = Vec::new();
        let mut trace = ExecutionTrace {
            rounds: MAX_RESOLUTION_ROUNDS,
            limit_reached: true,
            ..ExecutionTrace::default()
        };
        let mut rng = crate::rng::SplitMix64::new(1);
        cancel_waiting(&state, &mut events, 0, &mut rng, &mut trace);
        assert_eq!(events.len(), 1);
        let Event::PluginRuntimeUpdated { runtime } = &events[0] else {
            panic!("expected a runtime update");
        };
        assert!(runtime.movements.is_empty());
        assert_eq!(trace.stage("limit_dropped_continuations"), 1);
        assert!(
            state.plugin_runtime.movements.len() == 1,
            "the input is untouched"
        );
    }
}
