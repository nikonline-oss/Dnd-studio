//! Triggers (ADR-0024): "when → if → then" records the core runs after an intent. Data, not code:
//! conditions are `dnd-expr`, dice are `dnd-dice`, actions come from a fixed list. Their events
//! join the intent's batch, so the local game and the session server run them alike. Events
//! caused by triggers do not set off other triggers.

use std::collections::BTreeMap;

use dnd_expr::{Context, Expr, LookupError, Value as ExprValue};
use dnd_protocol::{
    AttackCheck, BoardVisibility, CharacterDto, CharacterId, ChatKind, ChatMessageDto,
    ChatVisibility, CombatDto, CombatId, CombatantDto, CombatantId, Event, Intent,
    JournalVisibility, MAX_COMBATANTS, MAX_SPAWN_TOKENS, MAX_TRIGGER_STEPS, MAX_TRIGGER_TEXT,
    MAX_TRIGGERS, MAX_TRIGGERS_PER_INTENT, MapId, RejectReason, ResourceTarget, RevealTarget,
    RollRequestDto, RollRequestId, RollRequestStatus, SpawnPlacement, SystemMessage, TokenDto,
    TokenId, TokenLayer, TriggerAction, TriggerDef, TriggerDto, TriggerEvent, TriggerFiredDto,
    TriggerId, Version, ZoneShape, uuid_v7_from_random,
};
use serde_json::Value;

use crate::apply::{MAX_NAME_LEN, bump, valid_name};
use crate::chat::{DiceRngAdapter, message_id, posted, roll_dto, system};
use crate::combat::initiative_expression;
use crate::evolve::evolve;
use crate::plugins::{CharacterFields, give_item, inventory_names, normalize_sheet, sheet_values};
use crate::rng::GameRng;
use crate::state::GameState;

/// Longest condition of a trigger or a step.
const MAX_EXPRESSION_LEN: usize = 1024;
/// Most tokens a trigger may be limited to.
const MAX_TRIGGER_TOKENS: usize = 100;

/// Events for a trigger intent; `None` if `intent` is not about triggers.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateTrigger { id, definition } => (|| {
            if state.triggers.len() >= MAX_TRIGGERS {
                return Err(RejectReason::invalid("id"));
            }
            Ok(vec![Event::TriggerCreated {
                trigger: TriggerDto {
                    id: *id,
                    definition: valid_definition(state, definition)?,
                    fired: false,
                    fired_maps: Vec::new(),
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateTrigger { id, definition } => (|| {
            let mut trigger = state
                .triggers
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            trigger.definition = valid_definition(state, definition)?;
            // Edited: armed again.
            trigger.fired = false;
            trigger.fired_maps.clear();
            trigger.version = bump(trigger.version)?;
            Ok(vec![Event::TriggerUpdated { trigger }])
        })(),
        Intent::DeleteTrigger { id } => (|| {
            if !state.triggers.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            let mut events: Vec<_> = state.board_nodes.values()
                .filter(|n| matches!(n.content, dnd_protocol::BoardNodeContent::Trigger { id: target } if target == *id))
                .flat_map(|n| crate::boards::delete_node(state, n.id)).collect();
            events.push(Event::TriggerDeleted { id: *id });
            Ok(events)
        })(),
        Intent::FireTrigger { id, token } => (|| {
            let trigger = state.triggers.get(id).ok_or(RejectReason::NotFound)?;
            if trigger.definition.event != TriggerEvent::Manual {
                return Err(RejectReason::invalid("id"));
            }
            let scope = effective_maps(state, &trigger.definition);
            let selected = token
                .map(|id| {
                    state
                        .tokens
                        .get(&id)
                        .ok_or_else(|| RejectReason::invalid("token"))
                })
                .transpose()?;
            if let Some(selected) = selected
                && ((!scope.is_empty() && !scope.contains(&selected.map_id))
                    || (!trigger.definition.tokens.is_empty()
                        && !trigger.definition.tokens.contains(&selected.id)))
            {
                return Err(RejectReason::invalid("token"));
            }
            let maps: Vec<Option<MapId>> = if scope.is_empty() {
                vec![None]
            } else if let Some(selected) = selected {
                vec![Some(selected.map_id)]
            } else {
                scope
                    .into_iter()
                    .filter(|id| state.maps.contains_key(id))
                    .map(Some)
                    .collect()
            };
            let firings: Vec<_> = maps
                .into_iter()
                .filter(|map| available(trigger, *map))
                .map(|map_id| Firing {
                    character: selected.and_then(|t| t.character_id),
                    token: *token,
                    total: None,
                    map_id,
                })
                .collect();
            if firings.is_empty() {
                return Err(RejectReason::invalid("id"));
            }
            let mut work = state.clone();
            Ok(firings
                .iter()
                .flat_map(|firing| run(&mut work, *id, firing, ts_ms, rng))
                .collect())
        })(),
        _ => return None,
    };
    Some(result)
}

/// Who set a trigger off and with what roll.
pub(crate) struct Firing {
    pub(crate) map_id: Option<MapId>,
    character: Option<CharacterId>,
    token: Option<TokenId>,
    total: Option<i64>,
}

/// The events of the triggers that `events` (the intent's own) set off, in trigger order.
pub(crate) fn react(
    before: &GameState,
    intent: &Intent,
    events: &[Event],
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let armed = |kind: fn(&TriggerEvent) -> bool| {
        before
            .triggers
            .values()
            .any(|t| t.definition.enabled && kind(&t.definition.event))
    };
    let roll = match intent {
        Intent::RollDice {
            character_id: Some(character),
            roll: Some(roll),
            ..
        } => Some((*character, roll.as_str())),
        _ => None,
    };
    let moves = events.iter().any(|e| matches!(e, Event::TokenMoved { .. }));
    let items = events
        .iter()
        .any(|e| matches!(e, Event::CharacterUpdated { .. }));
    let wanted = (roll.is_some() && armed(|e| matches!(e, TriggerEvent::RollInZone { .. })))
        || (moves && armed(|e| matches!(e, TriggerEvent::TokenEnteredZone { .. })))
        || (items && armed(|e| matches!(e, TriggerEvent::ItemReceived { .. })));
    if !wanted {
        return Vec::new();
    }

    let mut after = before.clone();
    for event in events {
        evolve(&mut after, event);
    }
    let firings = matches(before, &after, roll, events);
    let mut work = after;
    let mut out = Vec::new();
    for (id, firing) in firings.into_iter().take(MAX_TRIGGERS_PER_INTENT) {
        out.extend(run(&mut work, id, &firing, ts_ms, rng));
    }
    out
}

/// The triggers `events` set off, with who set them off.
fn matches(
    before: &GameState,
    after: &GameState,
    roll: Option<(CharacterId, &str)>,
    events: &[Event],
) -> Vec<(TriggerId, Firing)> {
    let mut found = Vec::new();
    let total = events.iter().find_map(|e| match e {
        Event::ChatMessagePosted { message } => message.roll.as_ref().map(|r| r.total),
        _ => None,
    });
    for trigger in after.triggers.values() {
        let def = &trigger.definition;
        if !def.enabled {
            continue;
        }
        let scope = effective_maps(after, def);
        let allowed = |token: TokenId| def.tokens.is_empty() || def.tokens.contains(&token);
        let mut candidates = Vec::new();
        match &def.event {
            TriggerEvent::RollInZone { zone, rolls } => {
                let Some((character, roll)) = roll else {
                    continue;
                };
                if !rolls.is_empty() && !rolls.iter().any(|r| r == roll) {
                    continue;
                }
                let Some(anchor) = after.zones.get(zone) else {
                    continue;
                };
                for zone in after
                    .zones
                    .values()
                    .filter(|z| scope.contains(&z.map_id) && z.name.trim() == anchor.name.trim())
                {
                    if let Some(token) = after.tokens_on(zone.map_id).find(|t| {
                        t.character_id == Some(character)
                            && allowed(t.id)
                            && zone.shape.contains(t.x, t.y)
                    }) {
                        candidates.push(Firing {
                            character: Some(character),
                            token: Some(token.id),
                            total,
                            map_id: Some(zone.map_id),
                        });
                    }
                }
            }
            TriggerEvent::TokenEnteredZone { zone } => {
                let Some(anchor) = after.zones.get(zone) else {
                    continue;
                };
                for event in events {
                    let Event::TokenMoved { id, x, y } = event else {
                        continue;
                    };
                    let (Some(token), Some(was)) = (after.tokens.get(id), before.tokens.get(id))
                    else {
                        continue;
                    };
                    if !scope.contains(&token.map_id)
                        || !allowed(*id)
                        || (def.tokens.is_empty() && token.character_id.is_none())
                    {
                        continue;
                    }
                    // A logical named zone may have several shapes on the same map.
                    let named: Vec<_> = after
                        .zones
                        .values()
                        .filter(|z| z.map_id == token.map_id && z.name.trim() == anchor.name.trim())
                        .collect();
                    if named.iter().any(|z| z.shape.contains(*x, *y))
                        && !named.iter().any(|z| z.shape.contains(was.x, was.y))
                    {
                        candidates.push(Firing {
                            character: token.character_id,
                            token: Some(*id),
                            total: None,
                            map_id: Some(token.map_id),
                        });
                    }
                }
            }
            TriggerEvent::ItemReceived { name } => {
                let Some(rules) = after.rules() else {
                    continue;
                };
                for event in events {
                    let Event::CharacterUpdated { character } = event else {
                        continue;
                    };
                    let old = before
                        .characters
                        .get(&character.id)
                        .map(|c| inventory_names(rules, &c.sheet_json))
                        .unwrap_or_default();
                    let new = inventory_names(rules, &character.sheet_json);
                    let count = |names: &[String]| {
                        names
                            .iter()
                            .filter(|n| n.to_lowercase() == name.to_lowercase())
                            .count()
                    };
                    if count(&new) <= count(&old) {
                        continue;
                    }
                    if scope.is_empty() {
                        let token = after
                            .tokens
                            .values()
                            .find(|t| t.character_id == Some(character.id) && allowed(t.id))
                            .map(|t| t.id);
                        if !def.tokens.is_empty() && token.is_none() {
                            continue;
                        }
                        candidates.push(Firing {
                            character: Some(character.id),
                            token,
                            total: None,
                            map_id: None,
                        });
                    } else {
                        for token in after.tokens.values().filter(|t| {
                            t.character_id == Some(character.id)
                                && scope.contains(&t.map_id)
                                && allowed(t.id)
                        }) {
                            candidates.push(Firing {
                                character: Some(character.id),
                                token: Some(token.id),
                                total: None,
                                map_id: Some(token.map_id),
                            });
                        }
                    }
                }
            }
            TriggerEvent::Manual => {}
        }
        let mut seen = Vec::new();
        for firing in candidates {
            if !seen.contains(&firing.map_id)
                && available(trigger, firing.map_id)
                && condition_holds(after, def.condition.as_deref(), &firing, &BTreeMap::new())
            {
                seen.push(firing.map_id);
                found.push((trigger.id, firing));
            }
        }
    }
    found
}

/// Resolve old zone definitions without mutating snapshots or replayed events.
fn effective_maps(state: &GameState, def: &TriggerDef) -> Vec<MapId> {
    if !def.map_ids.is_empty() {
        return def.map_ids.clone();
    }
    let zone = match def.event {
        TriggerEvent::RollInZone { zone, .. } | TriggerEvent::TokenEnteredZone { zone } => {
            Some(zone)
        }
        _ => None,
    };
    zone.and_then(|id| state.zones.get(&id))
        .map(|z| vec![z.map_id])
        .unwrap_or_default()
}

fn available(trigger: &TriggerDto, map_id: Option<MapId>) -> bool {
    // A legacy spent record has no map list: preserve its spent state until explicitly edited.
    if trigger.fired && trigger.fired_maps.is_empty() {
        return false;
    }
    map_id.map_or(!trigger.fired, |id| !trigger.fired_maps.contains(&id))
}

/// Runs a trigger's steps against `work`, which follows every event produced. Returns the events.
fn run(
    work: &mut GameState,
    id: TriggerId,
    firing: &Firing,
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let Some(trigger) = work.triggers.get(&id).cloned() else {
        return Vec::new();
    };
    if !available(&trigger, firing.map_id) {
        return Vec::new();
    }
    let mut events = vec![Event::TriggerFired {
        fired: TriggerFiredDto {
            trigger: id,
            name: trigger.definition.name.clone(),
            character: firing.character,
            token: firing.token,
            map_id: firing.map_id,
        },
    }];
    if trigger.definition.once
        && let Ok(version) = bump(trigger.version)
    {
        let mut next = trigger.clone();
        next.version = version;
        if let Some(map_id) = firing.map_id {
            next.fired_maps.push(map_id);
            next.fired_maps.sort();
            next.fired_maps.dedup();
            next.fired = effective_maps(work, &next.definition)
                .iter()
                .all(|id| next.fired_maps.contains(id));
        } else {
            next.fired = true;
        }
        events.push(Event::TriggerUpdated { trigger: next });
    }
    for event in &events {
        evolve(work, event);
    }
    let mut vars = BTreeMap::new();
    if let Some(total) = firing.total {
        vars.insert("total".to_owned(), total);
    }
    events.extend(run_steps(work, &trigger, firing, &mut vars, 0, ts_ms, rng));
    events
}

fn set_action_result(vars: &mut BTreeMap<String, i64>, result: dnd_protocol::ActionResolution) {
    for (name, outcome) in [
        ("completed", dnd_protocol::ActionResolution::Completed),
        ("countered", dnd_protocol::ActionResolution::Countered),
        ("cancelled", dnd_protocol::ActionResolution::Cancelled),
        ("invalid", dnd_protocol::ActionResolution::Invalid),
    ] {
        vars.insert(format!("action.{name}"), i64::from(result == outcome));
    }
}
#[allow(clippy::too_many_arguments)]
fn run_steps(
    work: &mut GameState,
    trigger: &TriggerDto,
    firing: &Firing,
    vars: &mut BTreeMap<String, i64>,
    start: usize,
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let mut out = Vec::new();
    for (index, step) in trigger.definition.steps.iter().enumerate().skip(start) {
        if !condition_holds(work, step.when.as_deref(), firing, vars) {
            continue;
        }
        let existing: Vec<_> = work
            .plugin_runtime
            .action_stacks
            .iter()
            .map(|s| s.id)
            .collect();
        let events = perform(work, trigger, &step.action, firing, vars, ts_ms, rng);
        let succeeded = !events.is_empty();
        for event in events {
            evolve(work, &event);
            out.push(event);
        }
        if matches!(step.action, TriggerAction::PluginAction { .. }) {
            if let Some(stack) = work
                .plugin_runtime
                .action_stacks
                .iter()
                .find(|s| !existing.contains(&s.id))
            {
                if index + 1 < trigger.definition.steps.len()
                    && work.plugin_runtime.trigger_runs.len() < 128
                {
                    let run = dnd_protocol::PendingTriggerRun {
                        action: stack.id,
                        trigger: trigger.id,
                        version: work
                            .triggers
                            .get(&trigger.id)
                            .map_or(trigger.version, |t| t.version),
                        map: firing.map_id,
                        character: firing.character,
                        token: firing.token,
                        next_step: u16::try_from(index + 1).unwrap_or(u16::MAX),
                        variables: vars.clone(),
                    };
                    work.plugin_runtime.trigger_runs.push(run);
                    out.push(crate::mechanics::runtime_event(work));
                }
                break;
            }
            set_action_result(
                vars,
                if succeeded {
                    dnd_protocol::ActionResolution::Completed
                } else {
                    dnd_protocol::ActionResolution::Invalid
                },
            );
        }
    }
    out
}

/// Resume only from authoritative root results. Script effects never recursively fire triggers.
pub(crate) fn resume(
    before: &GameState,
    events: &[Event],
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    if before.plugin_runtime.trigger_runs.is_empty() && !events.iter().any(|e| matches!(e, Event::PluginRuntimeUpdated { runtime } if !runtime.trigger_runs.is_empty())) { return Vec::new(); }
    let mut work = before.clone();
    for event in events {
        evolve(&mut work, event);
    }
    let results: BTreeMap<_, _> = events
        .iter()
        .filter_map(|event| match event {
            Event::ChatMessagePosted { message } => match &message.system {
                Some(SystemMessage::PluginActionResolved {
                    action: Some(id),
                    outcome,
                    ..
                }) => Some((*id, *outcome)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for run in work.plugin_runtime.trigger_runs.clone() {
        let trigger = work
            .triggers
            .get(&run.trigger)
            .filter(|t| t.version == run.version && t.definition.enabled)
            .cloned();
        let context_exists = run.map.is_none_or(|id| work.maps.contains_key(&id))
            && run
                .character
                .is_none_or(|id| work.characters.contains_key(&id))
            && run.token.is_none_or(|id| {
                work.tokens
                    .get(&id)
                    .is_some_and(|t| run.character.is_none_or(|c| t.character_id == Some(c)))
            });
        let result = results.get(&run.action).copied();
        if trigger.is_some()
            && context_exists
            && result.is_none()
            && work
                .plugin_runtime
                .action_stacks
                .iter()
                .any(|s| s.id == run.action)
        {
            continue;
        }
        work.plugin_runtime
            .trigger_runs
            .retain(|r| r.action != run.action);
        out.push(crate::mechanics::runtime_event(&work));
        if let (Some(trigger), Some(result)) = (trigger, result)
            && context_exists
            && matches!(
                result,
                dnd_protocol::ActionResolution::Completed
                    | dnd_protocol::ActionResolution::Countered
            )
        {
            let firing = Firing {
                map_id: run.map,
                character: run.character,
                token: run.token,
                total: run.variables.get("total").copied(),
            };
            let mut vars = run.variables;
            set_action_result(&mut vars, result);
            out.extend(run_steps(
                &mut work,
                &trigger,
                &firing,
                &mut vars,
                usize::from(run.next_step),
                ts_ms,
                rng,
            ));
        }
    }
    out
}

/// One action's events (none when it does not apply, e.g. no character to act on).
fn perform(
    work: &GameState,
    trigger: &TriggerDto,
    action: &TriggerAction,
    firing: &Firing,
    vars: &mut BTreeMap<String, i64>,
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let character = firing
        .character
        .and_then(|id| work.characters.get(&id))
        .cloned();
    match action {
        TriggerAction::PluginAction {
            action,
            source,
            target,
            x,
            y,
        } => {
            let Some(source) = source.or(firing.token) else {
                return Vec::new();
            };
            let actor = crate::Actor {
                user_id: dnd_protocol::UserId(trigger.id.0),
                role: dnd_protocol::Role::Gm,
            };
            crate::mechanics::execute(work, source, action, *target, *x, *y, &actor, ts_ms, rng)
                .unwrap_or_default()
        }
        TriggerAction::PlaySound {
            asset,
            volume,
            looped,
        } => work
            .assets
            .get(asset)
            .and_then(|a| crate::library::sound(work, a, *volume, *looped).ok())
            .map(|sound| vec![Event::SoundPlayed { sound }])
            .unwrap_or_default(),
        TriggerAction::StopSound { asset } => vec![Event::SoundStopped { asset_id: *asset }],
        TriggerAction::SpawnTokens {
            character,
            count,
            placement,
        } => spawn_tokens(work, *character, *count, *placement, firing, rng),
        TriggerAction::StartCombat => start_combat(work, firing, rng),
        TriggerAction::ApplyEffect {
            label,
            condition,
            resource,
            dice,
            heal,
            duration,
        } => apply_effect(
            work, label, condition, resource, dice, *heal, *duration, firing, ts_ms, rng,
        ),
        TriggerAction::PlaySting { asset, volume } => work
            .assets
            .get(asset)
            .and_then(|a| crate::library::sound(work, a, *volume, false).ok())
            .map(|mut sound| {
                sound.sting = true;
                vec![Event::SoundPlayed { sound }]
            })
            .unwrap_or_default(),
        TriggerAction::Reveal { target } => reveal(work, *target),
        TriggerAction::SetNodeVisibility { node, visibility } => work
            .board_nodes
            .get(node)
            .filter(|n| n.visibility != *visibility)
            .and_then(|n| {
                let mut node = n.clone();
                node.visibility = *visibility;
                node.version = bump(node.version).ok()?;
                Some(vec![Event::BoardNodeUpdated { node }])
            })
            .unwrap_or_default(),
        TriggerAction::Chat { text, gm_only } => {
            let mut event = system(SystemMessage::Narration { text: text.clone() }, rng);
            if *gm_only && let Event::ChatMessagePosted { message } = &mut event {
                message.visibility = ChatVisibility::Gm;
            }
            if let Event::ChatMessagePosted { message } = &mut event {
                message.related_characters = firing.character.into_iter().collect();
            }
            vec![event]
        }
        TriggerAction::Roll { dice, label } => {
            let Some(roll) = roll_dice(work, dice, character.as_ref(), vars, rng) else {
                return Vec::new();
            };
            vars.insert("roll".to_owned(), roll.total);
            let name = if label.trim().is_empty() {
                trigger.definition.name.clone()
            } else {
                label.clone()
            };
            vec![posted(ChatMessageDto {
                id: message_id(rng),
                kind: ChatKind::Roll,
                sender: None,
                sender_name: name,
                body: dice.trim().to_owned(),
                roll: Some(roll_dto(&roll)),
                system: None,
                visibility: ChatVisibility::All,
                related_characters: firing.character.into_iter().collect(),
                at_ms: None,
            })]
        }
        TriggerAction::ChangeResource {
            resource,
            dice,
            heal,
            target,
            attack,
            manual,
        } => change_resource_for_targets(
            work,
            trigger,
            *target,
            resource,
            dice,
            *heal,
            attack.as_ref(),
            *manual,
            firing,
            character.as_ref(),
            vars,
            rng,
        ),
        TriggerAction::AddCondition { condition }
        | TriggerAction::RemoveCondition { condition } => (|| {
            let character = character?;
            let rules = work.rules()?;
            let add = matches!(action, TriggerAction::AddCondition { .. });
            if add == character.conditions.contains(condition) {
                return None;
            }
            let conditions: Vec<String> = rules
                .conditions
                .iter()
                .filter(|c| {
                    if c.id == *condition {
                        add
                    } else {
                        character.conditions.contains(&c.id)
                    }
                })
                .map(|c| c.id.clone())
                .collect();
            let updated = updated_character(work, &character, |c| c.conditions = conditions)?;
            Some(vec![
                updated,
                crate::chat::system_for_character(
                    character.id,
                    SystemMessage::ConditionChanged {
                        name: character.name.clone(),
                        condition: condition.clone(),
                        added: add,
                    },
                    rng,
                ),
            ])
        })()
        .unwrap_or_default(),
        TriggerAction::GiveItem { name } => (|| {
            let character = character?;
            let sheet = give_item(work, &character.sheet_json, name).ok()?;
            let updated = updated_character(work, &character, |c| c.sheet_json = sheet)?;
            Some(vec![
                updated,
                crate::chat::system_for_character(
                    character.id,
                    SystemMessage::ItemGiven {
                        name: character.name.clone(),
                        item: name.clone(),
                    },
                    rng,
                ),
            ])
        })()
        .unwrap_or_default(),
    }
}

/// A fresh id of any entity kind, drawn from the core's RNG (deterministic for a seed).
fn fresh_id(rng: &mut dyn GameRng) -> dnd_protocol::Uuid {
    let bits = (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64());
    uuid_v7_from_random(bits)
}

/// Tokens of `character` appear on the firing map; nothing spawns without a map context.
fn spawn_tokens(
    work: &GameState,
    character: CharacterId,
    count: u8,
    placement: SpawnPlacement,
    firing: &Firing,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let Some(map_id) = firing.map_id else {
        return Vec::new();
    };
    let Some(who) = work.characters.get(&character) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for n in 0..count {
        let Some((x, y)) = spawn_point(work, map_id, placement, n, rng) else {
            break;
        };
        events.push(Event::TokenCreated {
            token: TokenDto {
                id: TokenId(fresh_id(rng)),
                map_id,
                character_id: Some(character),
                asset_id: None,
                name: who.name.clone(),
                x,
                y,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                disposition: dnd_protocol::TokenDisposition::Neutral,
                visible: true,
                version: Version::INITIAL,
            },
        });
    }
    events
}

/// Where the `n`-th spawned token lands: a fanned-out point, or a random spot inside the zone.
fn spawn_point(
    state: &GameState,
    map_id: MapId,
    placement: SpawnPlacement,
    n: u8,
    rng: &mut dyn GameRng,
) -> Option<(i64, i64)> {
    let map = state.maps.get(&map_id)?;
    let point = match placement {
        SpawnPlacement::Point { x, y } => {
            let step = (map.grid.cell_size * 16.0).max(16.0) as i64;
            let (col, row) = (i64::from(n) % 5, i64::from(n) / 5);
            (x + col * step, y + row * step)
        }
        SpawnPlacement::Zone { id } => {
            let zone = state.zones.get(&id).filter(|z| z.map_id == map_id)?;
            let (min_x, min_y, max_x, max_y) = shape_bounds(&zone.shape)?;
            let mut found = None;
            for _ in 0..32 {
                let span_x = u64::try_from(max_x - min_x).unwrap_or(0).max(1);
                let span_y = u64::try_from(max_y - min_y).unwrap_or(0).max(1);
                let px = min_x + i64::try_from(rng.next_u64() % span_x).unwrap_or(0);
                let py = min_y + i64::try_from(rng.next_u64() % span_y).unwrap_or(0);
                if zone.shape.contains(px, py) {
                    found = Some((px, py));
                    break;
                }
            }
            found.unwrap_or(((min_x + max_x) / 2, (min_y + max_y) / 2))
        }
    };
    Some((
        point.0.clamp(0, map.width * 16),
        point.1.clamp(0, map.height * 16),
    ))
}

/// The bounding box of every part of a zone shape, in Units16.
fn shape_bounds(shape: &ZoneShape) -> Option<(i64, i64, i64, i64)> {
    let mut bounds: Option<(i64, i64, i64, i64)> = None;
    for part in shape.parts() {
        let part_bounds = match &part {
            ZoneShape::Rect {
                x,
                y,
                width,
                height,
            } => (*x, *y, x.saturating_add(*width), y.saturating_add(*height)),
            ZoneShape::Polygon { points } => {
                let xs = points.as_chunks::<2>().0.iter().map(|[x, _]| *x);
                let ys = points.as_chunks::<2>().0.iter().map(|[_, y]| *y);
                match (xs.clone().min(), ys.clone().min(), xs.max(), ys.max()) {
                    (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
                    _ => continue,
                }
            }
            ZoneShape::Union { .. } => continue,
        };
        bounds = Some(match bounds {
            None => part_bounds,
            Some((a, b, c, d)) => (
                a.min(part_bounds.0),
                b.min(part_bounds.1),
                c.max(part_bounds.2),
                d.max(part_bounds.3),
            ),
        });
    }
    bounds
}

/// Puts a temporary effect on the firing token; nothing applies without a token context.
#[allow(clippy::too_many_arguments)]
fn apply_effect(
    work: &GameState,
    label: &str,
    condition: &Option<String>,
    resource: &Option<String>,
    dice: &Option<String>,
    heal: bool,
    duration: dnd_protocol::EffectDurationDef,
    firing: &Firing,
    ts_ms: i64,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    use dnd_protocol::{EffectDuration, EffectDurationDef};
    let Some(token) = firing.token else {
        return Vec::new();
    };
    if work.effects.values().filter(|e| e.token == token).count()
        >= dnd_protocol::MAX_EFFECTS_PER_TOKEN
    {
        return Vec::new();
    }
    let duration = match duration {
        EffectDurationDef::Rounds { rounds } => EffectDuration::Rounds { remaining: rounds },
        EffectDurationDef::Turns { turns } => EffectDuration::Turns { remaining: turns },
        EffectDurationDef::RealTime { minutes } => EffectDuration::RealTime {
            until_ts: ts_ms.saturating_add(i64::from(minutes).saturating_mul(60_000)),
        },
    };
    vec![Event::TokenEffectApplied {
        effect: dnd_protocol::TokenEffectDto {
            id: dnd_protocol::EffectId(fresh_id(rng)),
            token,
            label: label.to_owned(),
            condition: condition.clone(),
            resource: resource.clone(),
            dice: dice.clone(),
            heal,
            duration,
            version: Version::INITIAL,
        },
    }]
}

/// Opens a combat on the firing map: visible character tokens join and roll initiative at once.
fn start_combat(work: &GameState, firing: &Firing, rng: &mut dyn GameRng) -> Vec<Event> {
    let Some(map_id) = firing.map_id else {
        return Vec::new();
    };
    if work.combats.values().any(|c| c.map_id == map_id) {
        return Vec::new();
    }
    let combat_id = CombatId(fresh_id(rng));
    let mut events = vec![Event::CombatStarted {
        combat: CombatDto {
            id: combat_id,
            map_id,
            round: 1,
            turn: None,
            version: Version::INITIAL,
        },
    }];
    let mut sort_order = 0;
    for token in work.tokens.values().filter(|t| {
        t.map_id == map_id && t.visible && t.layer != TokenLayer::Gm && t.character_id.is_some()
    }) {
        if sort_order >= MAX_COMBATANTS {
            break;
        }
        let Some(character_id) = token.character_id else {
            continue;
        };
        let character = work.characters.get(&character_id);
        let name = if token.name.is_empty() {
            character.map_or_else(String::new, |c| c.name.clone())
        } else {
            token.name.clone()
        };
        let sheet = character.map(|c| c.sheet_json.as_str());
        let expression = work
            .rules()
            .map_or_else(|| initiative_expression(sheet), |r| r.initiative.as_str());
        let initiative = dnd_dice::Expr::parse(expression)
            .and_then(|e| e.roll(&CharacterFields::new(work, sheet), &mut DiceRngAdapter(rng)))
            .ok();
        if let Some(roll) = &initiative {
            events.push(posted(ChatMessageDto {
                id: message_id(rng),
                kind: ChatKind::Roll,
                sender: None,
                sender_name: name.clone(),
                body: expression.to_owned(),
                roll: Some(roll_dto(roll)),
                system: None,
                visibility: ChatVisibility::All,
                related_characters: character.map(|c| c.id).into_iter().collect(),
                at_ms: None,
            }));
        }
        events.push(Event::CombatantAdded {
            combatant: CombatantDto {
                id: CombatantId(fresh_id(rng)),
                combat_id,
                token_id: Some(token.id),
                character_id: Some(character_id),
                name,
                initiative: initiative.map(|r| r.total),
                sort_order: i64::try_from(sort_order).unwrap_or(i64::MAX),
                version: Version::INITIAL,
            },
        });
        sort_order += 1;
    }
    let mut initialized = work.clone();
    for e in &events {
        evolve(&mut initialized, e);
    }
    if let Some(combat) = initialized.combats.get(&combat_id) {
        events.extend(crate::economy::refresh(&initialized, combat, true, rng));
    }

    events
}

/// Shows `target` to players; nothing if it is already shown or gone.
fn reveal(work: &GameState, target: RevealTarget) -> Vec<Event> {
    let event = match target {
        RevealTarget::Node { id } => work
            .board_nodes
            .get(&id)
            .filter(|n| n.visibility != BoardVisibility::Players)
            .and_then(|n| {
                let mut node = n.clone();
                node.visibility = BoardVisibility::Players;
                node.version = bump(node.version).ok()?;
                Some(Event::BoardNodeUpdated { node })
            }),
        RevealTarget::Board { id } => work
            .boards
            .get(&id)
            .filter(|b| !b.visible_to_players)
            .and_then(|b| {
                let mut board = b.clone();
                board.visible_to_players = true;
                board.version = bump(board.version).ok()?;
                Some(Event::BoardUpdated { board })
            }),
        RevealTarget::Map { id } => work
            .maps
            .get(&id)
            .filter(|m| !m.visible_to_players)
            .and_then(|m| {
                let mut map = m.clone();
                map.visible_to_players = true;
                map.version = bump(map.version).ok()?;
                Some(Event::MapUpdated { map })
            }),
        RevealTarget::Journal { id } => work
            .journal
            .get(&id)
            .filter(|e| e.visibility != JournalVisibility::Players)
            .and_then(|e| {
                let mut entry = e.clone();
                entry.visibility = JournalVisibility::Players;
                entry.version = bump(entry.version).ok()?;
                Some(Event::JournalEntryUpdated { entry })
            }),
        RevealTarget::Token { id } => work.tokens.get(&id).filter(|t| !t.visible).and_then(|t| {
            let mut token = t.clone();
            token.visible = true;
            token.version = bump(token.version).ok()?;
            Some(Event::TokenUpdated { token })
        }),
    };
    event.into_iter().collect()
}

fn updated_character(
    work: &GameState,
    character: &CharacterDto,
    edit: impl FnOnce(&mut CharacterDto),
) -> Option<Event> {
    let mut next = work.characters.get(&character.id)?.clone();
    edit(&mut next);
    next.sheet_json = normalize_sheet(work, &next.sheet_json).ok()?;
    next.version = bump(next.version).ok()?;
    Some(Event::CharacterUpdated { character: next })
}

/// Damage or healing of `character`'s `resource` by a dice roll, kept within 0..=max, with the
/// change announced in chat. Shared by the trigger action and the effects' recurring ticks.
pub(crate) fn change_resource(
    work: &GameState,
    character: &CharacterDto,
    resource: &str,
    dice: &str,
    heal: bool,
    vars: &BTreeMap<String, i64>,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let Some(amount) =
        roll_dice(work, dice, Some(character), vars, rng).map(|roll| roll.total.max(0))
    else {
        return Vec::new();
    };
    apply_resource_change(work, character, resource, amount, heal, rng)
}

/// Damage/healing of the step's targets. Zone targets match the zone name on the firing map;
/// without a map context zone/map targets change nothing. A character takes the change once,
/// however many of its tokens qualify.
/// Auto: one shared roll, or — with `attack` — a roll per target applying only on a hit.
/// Manual: a `RollRequested` event per target instead of any rolls; controllers fulfill them.
#[allow(clippy::too_many_arguments)]
fn change_resource_for_targets(
    work: &GameState,
    trigger: &TriggerDto,
    target: ResourceTarget,
    resource: &str,
    dice: &str,
    heal: bool,
    attack: Option<&AttackCheck>,
    manual: bool,
    firing: &Firing,
    activator: Option<&CharacterDto>,
    vars: &BTreeMap<String, i64>,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let pairs = target_pairs(work, target, firing, activator);
    if manual {
        return request_rolls(work, trigger, pairs, resource, dice, heal, attack, rng);
    }
    if pairs.is_empty() {
        return Vec::new();
    }
    if let Some(attack) = attack {
        // Per target: the attack roll uses the activator's fields and the step variables; the
        // `hit` condition reads the target's fields with `@total` = the attack roll. Damage on
        // a hit only; the roll is posted either way.
        let mut events = Vec::new();
        for (character, _) in &pairs {
            let Some(roll) = roll_dice(work, &attack.dice, activator, vars, rng) else {
                continue;
            };
            events.push(posted(ChatMessageDto {
                id: message_id(rng),
                kind: ChatKind::Roll,
                sender: None,
                sender_name: format!("{} → {}", trigger.definition.name, character.name),
                body: attack.dice.trim().to_owned(),
                roll: Some(roll_dto(&roll)),
                system: None,
                visibility: ChatVisibility::All,
                related_characters: [Some(character.id), activator.map(|c| c.id)]
                    .into_iter()
                    .flatten()
                    .collect(),
                at_ms: None,
            }));
            let mut hit_vars = vars.clone();
            hit_vars.insert("total".to_owned(), roll.total);
            if !expr_holds(work, character, &hit_vars, &attack.hit) {
                continue;
            }
            let Some(amount) = roll_dice(work, dice, activator, vars, rng).map(|r| r.total.max(0))
            else {
                continue;
            };
            events.extend(apply_resource_change(
                work, character, resource, amount, heal, rng,
            ));
        }
        return events;
    }
    let Some(amount) = roll_dice(work, dice, activator, vars, rng).map(|roll| roll.total.max(0))
    else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for (character, _) in &pairs {
        events.extend(apply_resource_change(
            work, character, resource, amount, heal, rng,
        ));
    }
    events
}

/// Distinct target characters with one qualifying token each.
fn target_pairs(
    work: &GameState,
    target: ResourceTarget,
    firing: &Firing,
    activator: Option<&CharacterDto>,
) -> Vec<(CharacterDto, Option<TokenId>)> {
    if matches!(target, ResourceTarget::Activator) {
        return activator
            .map(|character| (character.clone(), firing.token))
            .into_iter()
            .collect();
    }
    let Some(map_id) = firing.map_id else {
        return Vec::new();
    };
    let anchor = match target {
        ResourceTarget::Zone { id } => work.zones.get(&id),
        _ => None,
    };
    let inside = |token: &TokenDto| match target {
        ResourceTarget::Zone { .. } => anchor.is_some_and(|anchor| {
            work.zones
                .values()
                .filter(|z| z.map_id == map_id && z.name.trim() == anchor.name.trim())
                .any(|z| z.shape.contains(token.x, token.y))
        }),
        ResourceTarget::Map => true,
        ResourceTarget::Activator => unreachable!("handled above"),
    };
    let mut pairs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in work.tokens.values().filter(|token| token.map_id == map_id) {
        if !inside(token) {
            continue;
        }
        let Some(character_id) = token.character_id else {
            continue;
        };
        if !seen.insert(character_id) {
            continue;
        }
        let Some(character) = work.characters.get(&character_id) else {
            continue;
        };
        pairs.push((character.clone(), Some(token.id)));
    }
    pairs
}

/// Manual mode: a pending request per target character, rolled by whoever controls them.
#[allow(clippy::too_many_arguments)]
fn request_rolls(
    work: &GameState,
    trigger: &TriggerDto,
    pairs: Vec<(CharacterDto, Option<TokenId>)>,
    resource: &str,
    dice: &str,
    heal: bool,
    attack: Option<&AttackCheck>,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    let mut events = Vec::new();
    for (character, token) in pairs {
        let pending = events
            .iter()
            .filter(|e| matches!(e, Event::RollRequested { .. }))
            .count();
        if work.roll_requests.len() + pending >= dnd_protocol::MAX_ROLL_REQUESTS {
            break;
        }
        // One pending request per trigger and character: a re-fire stacks no debt.
        let duplicate = work.roll_requests.values().any(|r| {
            r.trigger == trigger.id
                && r.character == character.id
                && matches!(r.status, RollRequestStatus::Pending)
        });
        if duplicate {
            continue;
        }
        let request = RollRequestDto {
            id: RollRequestId(fresh_id(rng)),
            label: trigger.definition.name.clone(),
            trigger: trigger.id,
            character: character.id,
            token,
            dice: attack
                .map(|a| a.dice.clone())
                .unwrap_or_else(|| dice.to_owned()),
            hit: attack.map(|a| a.hit.clone()),
            resource: resource.to_owned(),
            damage_dice: dice.to_owned(),
            heal,
            status: RollRequestStatus::Pending,
            total: None,
            delta: None,
            version: Version::INITIAL,
        };
        let mut notification = crate::chat::system_for_character(
            character.id,
            SystemMessage::RollRequested {
                name: character.name.clone(),
                label: trigger.definition.name.clone(),
            },
            rng,
        );
        if let Event::ChatMessagePosted { message } = &mut notification {
            message.visibility = ChatVisibility::Gm;
        }
        events.push(notification);
        events.push(Event::RollRequested { request });
    }
    events
}

/// A condition against an explicit character and variables (per-target `hit` checks).
pub(crate) fn expr_holds(
    state: &GameState,
    character: &CharacterDto,
    vars: &BTreeMap<String, i64>,
    expression: &str,
) -> bool {
    if expression.trim().is_empty() {
        return true;
    }
    let scope = Scope::new(state, Some(character), vars);
    match Expr::parse(expression).and_then(|e| e.eval(&scope)) {
        Ok(value) => value
            .as_bool()
            .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0)),
        Err(_) => false,
    }
}

/// One resource change of a known amount applied to one character, announced in chat.
fn apply_resource_change(
    work: &GameState,
    character: &CharacterDto,
    resource: &str,
    amount: i64,
    heal: bool,
    rng: &mut dyn GameRng,
) -> Vec<Event> {
    apply_resource_change_delta(work, character, resource, amount, heal, rng).0
}

/// Like [`apply_resource_change`], also returning the applied delta (for roll requests).
pub(crate) fn apply_resource_change_delta(
    work: &GameState,
    character: &CharacterDto,
    resource: &str,
    amount: i64,
    heal: bool,
    rng: &mut dyn GameRng,
) -> (Vec<Event>, Option<i64>) {
    let applied = (|| {
        let path = resource_path(work, resource)?;
        let (_, current, max) = sheet_values(work, &character.sheet_json)?
            .resources
            .into_iter()
            .find(|(id, _, _)| id == resource)?;
        let next = if heal {
            current.saturating_add(amount).min(max.max(0))
        } else {
            current.saturating_sub(amount).max(0)
        };
        let sheet = set_integer(&character.sheet_json, &path, next)?;
        let updated = updated_character(work, character, |c| c.sheet_json = sheet)?;
        Some((
            vec![
                updated,
                crate::chat::system_for_character(
                    character.id,
                    SystemMessage::ResourceChanged {
                        name: character.name.clone(),
                        resource: resource.to_owned(),
                        delta: next - current,
                    },
                    rng,
                ),
            ],
            next - current,
        ))
    })();
    match applied {
        Some((events, delta)) => (events, Some(delta)),
        None => (Vec::new(), None),
    }
}

/// The sheet path a resource's value reads (`@hp.current` → `hp.current`), if it is a plain
/// field; computed resources cannot be changed.
pub(crate) fn resource_path(state: &GameState, resource: &str) -> Option<String> {
    let def = state.rules()?.resources.iter().find(|r| r.id == resource)?;
    let path = def.value.trim().strip_prefix('@')?;
    let plain = !path.is_empty()
        && path
            .split('.')
            .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    plain.then(|| path.to_owned())
}

fn set_integer(sheet_json: &str, path: &str, value: i64) -> Option<String> {
    let mut sheet: Value = serde_json::from_str(sheet_json).ok()?;
    let mut node = &mut sheet;
    for segment in path.split('.') {
        node = node
            .as_object_mut()?
            .entry(segment)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    *node = Value::from(value);
    serde_json::to_string(&sheet).ok()
}

/// What `@name` means in trigger expressions: `@total`, `@roll`, then the character's fields.
struct Scope<'a> {
    fields: Option<CharacterFields>,
    vars: &'a BTreeMap<String, i64>,
}

impl<'a> Scope<'a> {
    fn new(
        state: &GameState,
        character: Option<&CharacterDto>,
        vars: &'a BTreeMap<String, i64>,
    ) -> Self {
        Self {
            fields: character.and_then(|c| crate::modifiers::effective_fields(state, c).ok()),
            vars,
        }
    }
}

impl Context for Scope<'_> {
    fn lookup(&self, path: &str) -> Result<ExprValue, LookupError> {
        if let Some(value) = self.vars.get(path) {
            return Ok(ExprValue::from(*value));
        }
        match &self.fields {
            Some(fields) => Context::lookup(fields, path),
            None => Err(LookupError::Unknown),
        }
    }
}

impl dnd_dice::FieldLookup for Scope<'_> {
    fn lookup(&self, path: &str) -> Result<i64, dnd_dice::LookupError> {
        if let Some(value) = self.vars.get(path) {
            return Ok(*value);
        }
        match &self.fields {
            Some(fields) => dnd_dice::FieldLookup::lookup(fields, path),
            None => Err(dnd_dice::LookupError::Unknown),
        }
    }
}

/// No condition, or one that evaluates to `true` (or a non-zero number). Errors count as false.
fn condition_holds(
    state: &GameState,
    condition: Option<&str>,
    firing: &Firing,
    vars: &BTreeMap<String, i64>,
) -> bool {
    let Some(condition) = condition.filter(|c| !c.trim().is_empty()) else {
        return true;
    };
    let mut all = vars.clone();
    if let Some(total) = firing.total {
        all.entry("total".to_owned()).or_insert(total);
    }
    let character = firing.character.and_then(|id| state.characters.get(&id));
    let scope = Scope::new(state, character, &all);
    match Expr::parse(condition).and_then(|e| e.eval(&scope)) {
        Ok(value) => value
            .as_bool()
            .unwrap_or_else(|| value.as_integer().is_ok_and(|n| n != 0)),
        Err(_) => false,
    }
}

pub(crate) fn roll_dice(
    state: &GameState,
    dice: &str,
    character: Option<&CharacterDto>,
    vars: &BTreeMap<String, i64>,
    rng: &mut dyn GameRng,
) -> Option<dnd_dice::Roll> {
    let scope = Scope::new(state, character, vars);
    dnd_dice::Expr::parse(dice)
        .ok()?
        .roll(&scope, &mut DiceRngAdapter(rng))
        .ok()
}

/// A definition whose references exist and whose expressions and dice parse; names trimmed.
fn valid_definition(state: &GameState, def: &TriggerDef) -> Result<TriggerDef, RejectReason> {
    let mut def = def.clone();
    def.name = valid_name(&def.name, "name")?;
    if def.map_ids.len() > 100 || def.map_ids.iter().any(|id| !state.maps.contains_key(id)) {
        return Err(RejectReason::invalid("map_ids"));
    }
    def.map_ids.sort();
    def.map_ids.dedup();
    if def.map_ids.is_empty() {
        def.map_ids = effective_maps(state, &def);
    }
    let rules = state.rules();
    match &mut def.event {
        TriggerEvent::RollInZone { zone, rolls } => {
            if !state.zones.contains_key(zone) {
                return Err(RejectReason::invalid("event"));
            }
            let known = |id: &String| rules.is_some_and(|r| r.rolls.iter().any(|d| d.id == *id));
            if !rolls.iter().all(known) {
                return Err(RejectReason::invalid("event"));
            }
        }
        TriggerEvent::TokenEnteredZone { zone } => {
            if !state.zones.contains_key(zone) {
                return Err(RejectReason::invalid("event"));
            }
        }
        TriggerEvent::Manual => {}
        TriggerEvent::ItemReceived { name } => {
            *name = valid_name(name, "event")?;
        }
    }
    if def.tokens.len() > MAX_TRIGGER_TOKENS
        || !def.tokens.iter().all(|t| state.tokens.contains_key(t))
    {
        return Err(RejectReason::invalid("tokens"));
    }
    if let Some(condition) = &def.condition {
        valid_expression(condition, "condition")?;
    }
    if def.steps.is_empty() || def.steps.len() > MAX_TRIGGER_STEPS {
        return Err(RejectReason::invalid("steps"));
    }
    for step in &mut def.steps {
        if let Some(when) = &step.when {
            valid_expression(when, "steps")?;
        }
        valid_action(state, &mut step.action)?;
    }
    Ok(def)
}

fn valid_expression(expression: &str, field: &str) -> Result<(), RejectReason> {
    if expression.len() > MAX_EXPRESSION_LEN || Expr::parse(expression).is_err() {
        return Err(RejectReason::invalid(field));
    }
    Ok(())
}

fn valid_action(state: &GameState, action: &mut TriggerAction) -> Result<(), RejectReason> {
    let invalid = || RejectReason::invalid("steps");
    let dice_ok = |dice: &str| dice.len() <= 256 && dnd_dice::Expr::parse(dice).is_ok();
    let rules = state.rules();
    let ok = match action {
        TriggerAction::PluginAction {
            action,
            source,
            target,
            x,
            y,
        } => {
            rules.is_some_and(|r| {
                r.mechanics
                    .actions
                    .iter()
                    .any(|a| a.id == *action && a.interrupt.is_none())
            }) && source.is_none_or(|s| state.tokens.contains_key(&s))
                && target.is_none_or(|t| state.tokens.contains_key(&t))
                && *x >= 0
                && *y >= 0
        }
        TriggerAction::Reveal { target } => match target {
            RevealTarget::Node { id } => state.board_nodes.contains_key(id),
            RevealTarget::Board { id } => state.boards.contains_key(id),
            RevealTarget::Map { id } => state.maps.contains_key(id),
            RevealTarget::Journal { id } => state.journal.contains_key(id),
            RevealTarget::Token { id } => state.tokens.contains_key(id),
        },
        TriggerAction::SetNodeVisibility { node, .. } => state.board_nodes.contains_key(node),
        TriggerAction::PlaySound { asset, volume, .. } => {
            *volume <= dnd_protocol::MAX_VOLUME
                && state
                    .assets
                    .get(asset)
                    .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }
        TriggerAction::StopSound { asset } => asset.is_none_or(|a| {
            state
                .assets
                .get(&a)
                .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }),
        TriggerAction::SpawnTokens {
            character,
            count,
            placement,
        } => {
            state.characters.contains_key(character)
                && *count >= 1
                && usize::from(*count) <= MAX_SPAWN_TOKENS
                && match placement {
                    SpawnPlacement::Point { .. } => true,
                    SpawnPlacement::Zone { id } => state.zones.contains_key(id),
                }
        }
        TriggerAction::StartCombat => true,
        TriggerAction::ApplyEffect {
            label,
            condition,
            resource,
            dice,
            duration,
            ..
        } => {
            *label = label.trim().to_owned();
            let span_ok = match duration {
                dnd_protocol::EffectDurationDef::Rounds { rounds }
                | dnd_protocol::EffectDurationDef::Turns { turns: rounds } => {
                    *rounds >= 1 && *rounds <= dnd_protocol::MAX_EFFECT_SPAN
                }
                dnd_protocol::EffectDurationDef::RealTime { minutes } => {
                    *minutes >= 1 && *minutes <= dnd_protocol::MAX_EFFECT_MINUTES
                }
            };
            let condition_ok = condition.as_ref().is_none_or(|c| {
                rules.is_some_and(|r| r.conditions.iter().any(|known| known.id == *c))
            });
            let change_ok = match (resource, dice) {
                (None, None) => true,
                (Some(resource), Some(dice)) => {
                    dice_ok(dice) && resource_path(state, resource).is_some()
                }
                _ => false,
            };
            span_ok
                && condition_ok
                && change_ok
                && !label.is_empty()
                && label.chars().count() <= dnd_protocol::MAX_EFFECT_LABEL
        }
        TriggerAction::PlaySting { asset, volume } => {
            *volume <= dnd_protocol::MAX_VOLUME
                && state
                    .assets
                    .get(asset)
                    .is_some_and(|a| a.usage == dnd_protocol::AssetUsage::Audio)
        }
        TriggerAction::Chat { text, .. } => {
            *text = text.trim().to_owned();
            !text.is_empty() && text.chars().count() <= MAX_TRIGGER_TEXT
        }
        TriggerAction::Roll { dice, label } => {
            *label = label.trim().to_owned();
            dice_ok(dice) && label.chars().count() <= MAX_NAME_LEN
        }
        TriggerAction::ChangeResource {
            resource,
            dice,
            target,
            attack,
            ..
        } => {
            dice_ok(dice)
                && resource_path(state, resource).is_some()
                && match target {
                    ResourceTarget::Activator | ResourceTarget::Map => true,
                    ResourceTarget::Zone { id } => state.zones.contains_key(id),
                }
                && attack.as_ref().is_none_or(|check| {
                    dice_ok(&check.dice)
                        && !check.hit.trim().is_empty()
                        && Expr::parse(&check.hit).is_ok()
                })
        }
        TriggerAction::AddCondition { condition }
        | TriggerAction::RemoveCondition { condition } => {
            rules.is_some_and(|r| r.conditions.iter().any(|c| c.id == *condition))
        }
        TriggerAction::GiveItem { name } => {
            *name = valid_name(name, "steps")?;
            rules.is_some_and(|r| r.inventory.is_some())
        }
    };
    if ok { Ok(()) } else { Err(invalid()) }
}

/// Binding visuals are part of the same transaction as edits/placing a trigger card.
/// Edges between trigger and map cards express bindings; map cards themselves are never removed.
pub(crate) fn sync_board_bindings(
    state: &GameState,
    intent: &Intent,
    events: &[Event],
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    use dnd_protocol::{
        BoardEdgeDto, BoardEdgeId, BoardLink, BoardNodeContent, BoardNodeDto, BoardNodeId,
        MAX_BOARD_COORD, MAX_BOARD_EDGES, MAX_BOARD_NODES,
    };
    let trigger_id = match intent {
        Intent::CreateTrigger { id, .. } | Intent::UpdateTrigger { id, .. } => *id,
        Intent::CreateBoardNode {
            content: BoardNodeContent::Trigger { id },
            ..
        }
        | Intent::UpdateBoardNode {
            content: Some(BoardNodeContent::Trigger { id }),
            ..
        } => *id,
        _ => return Ok(Vec::new()),
    };
    let mut work = state.clone();
    for event in events {
        evolve(&mut work, event);
    }
    let Some(trigger) = work.triggers.get(&trigger_id) else {
        return Ok(Vec::new());
    };
    let maps = effective_maps(&work, &trigger.definition);
    let cards: Vec<_> = work
        .board_nodes
        .values()
        .filter(|n| matches!(n.content, BoardNodeContent::Trigger { id } if id == trigger_id))
        .cloned()
        .collect();
    let mut out = Vec::new();
    let fresh = |work: &GameState, rng: &mut dyn GameRng| {
        // Bounded retries also handle deterministic test RNGs which repeat IDs across intents.
        for _ in 0..4096 {
            let id = message_id(rng).0;
            if !crate::apply::id_in_use(work, id) {
                return Ok(id);
            }
        }
        Err(RejectReason::invalid("board_id"))
    };
    let emit = |work: &mut GameState, out: &mut Vec<Event>, event: Event| {
        evolve(work, &event);
        out.push(event);
    };
    for card in cards {
        let removed: Vec<_> = work.board_edges.values().filter(|e| e.from == card.id || e.to == card.id).filter(|e| {
            let other = if e.from == card.id { e.to } else { e.from };
            work.board_nodes.get(&other).is_some_and(|n| matches!(n.content, BoardNodeContent::Link { target: BoardLink::Map { id } } if !maps.contains(&id)))
        }).map(|e| e.id).collect();
        for id in removed {
            emit(&mut work, &mut out, Event::BoardEdgeDeleted { id });
        }
        for (index, map_id) in maps.iter().enumerate() {
            if !work.maps.contains_key(map_id) {
                continue;
            }
            let existing = work.board_nodes.values().find(|n| n.board_id == card.board_id && matches!(n.content, BoardNodeContent::Link { target: BoardLink::Map { id } } if id == *map_id)).map(|n| n.id);
            let map_card = if let Some(id) = existing {
                id
            } else {
                if work
                    .board_nodes
                    .values()
                    .filter(|n| n.board_id == card.board_id)
                    .count()
                    >= MAX_BOARD_NODES
                {
                    return Err(RejectReason::invalid("board_id"));
                }
                let id = BoardNodeId(fresh(&work, rng)?);
                emit(
                    &mut work,
                    &mut out,
                    Event::BoardNodeCreated {
                        node: BoardNodeDto {
                            id,
                            board_id: card.board_id,
                            content: BoardNodeContent::Link {
                                target: BoardLink::Map { id: *map_id },
                            },
                            x: card
                                .x
                                .saturating_add(card.width)
                                .saturating_add(640)
                                .clamp(-MAX_BOARD_COORD, MAX_BOARD_COORD),
                            y: card
                                .y
                                .saturating_add((index as i64).saturating_mul(2560))
                                .clamp(-MAX_BOARD_COORD, MAX_BOARD_COORD),
                            width: 3200,
                            height: 1920,
                            visibility: BoardVisibility::Gm,
                            version: Version::INITIAL,
                        },
                    },
                );
                id
            };
            if work.board_edges.values().any(|e| {
                (e.from == card.id && e.to == map_card) || (e.to == card.id && e.from == map_card)
            }) {
                continue;
            }
            if work
                .board_edges
                .values()
                .filter(|e| e.board_id == card.board_id)
                .count()
                >= MAX_BOARD_EDGES
            {
                return Err(RejectReason::invalid("board_id"));
            }
            let id = BoardEdgeId(fresh(&work, rng)?);
            emit(
                &mut work,
                &mut out,
                Event::BoardEdgeCreated {
                    edge: BoardEdgeDto {
                        id,
                        board_id: card.board_id,
                        from: card.id,
                        to: map_card,
                        label: String::new(),
                        directed: true,
                        visibility: BoardVisibility::Gm,
                        version: Version::INITIAL,
                    },
                },
            );
        }
    }
    Ok(out)
}
