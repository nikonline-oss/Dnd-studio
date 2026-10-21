//! `apply`: intent → events. Pure: never mutates state, never performs I/O.

use dnd_protocol::{
    Event, Grid, Intent, IntentEnvelope, MapDto, RejectReason, TokenDto, Version, VersionPolicy,
};

use crate::permissions::{Actor, authorize};
use crate::rng::GameRng;
use crate::state::GameState;

/// Error returned by [`apply`]; the reason is sent to the client as `Rejected`.
pub type RuleError = RejectReason;

/// Maximum length of user-visible names, in characters.
pub const MAX_NAME_LEN: usize = 200;
/// Maximum length of the campaign description, in characters.
pub const MAX_DESCRIPTION_LEN: usize = 20_000;

/// Decides whether `envelope` may be applied to `state` by `actor` and returns the resulting events.
///
/// Checks run in this order: permissions, existence, version (ADR-0003), value validation.
/// Apply the returned events with [`crate::evolve`] to obtain the next state.
pub fn apply(
    state: &GameState,
    envelope: &IntentEnvelope,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RuleError> {
    let intent = &envelope.intent;
    envelope.validate()?;
    authorize(actor, intent, state)?;
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
        Intent::UpdateCampaignMeta { name, description } => {
            if name.is_none() && description.is_none() {
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
            campaign.version = bump(campaign.version)?;
            Ok(vec![Event::CampaignMetaUpdated { campaign }])
        }

        Intent::CreateMap {
            id,
            name,
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
            grid,
            visible_to_players,
        } => {
            if name.is_none() && grid.is_none() && visible_to_players.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            let mut map = state.maps.get(id).cloned().ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                map.name = valid_name(name, "name")?;
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

        Intent::DeleteMap { id } => {
            let mut events: Vec<Event> = state
                .tokens_on(*id)
                .map(|t| Event::TokenDeleted { id: t.id })
                .collect();
            events.push(Event::MapDeleted { id: *id });
            Ok(events)
        }

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
                visible: *visible,
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
        } => {
            if name.is_none()
                && rotation.is_none()
                && scale.is_none()
                && layer.is_none()
                && visible.is_none()
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

        Intent::DeleteToken { id } => Ok(vec![Event::TokenDeleted { id: *id }]),

        Intent::CreateCharacter { .. }
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

        Intent::SendMessage { .. } | Intent::RollDice { .. } => {
            crate::chat::apply(state, intent, actor, rng)
                .unwrap_or(Err(RejectReason::invalid("intent")))
        }

        Intent::CreateJournalEntry { .. }
        | Intent::UpdateJournalEntry { .. }
        | Intent::MoveJournalEntry { .. }
        | Intent::DeleteJournalEntry { .. } => crate::journal::apply(state, intent, actor)
            .unwrap_or(Err(RejectReason::invalid("intent"))),
    }
}

/// Current version of the entity a checked intent targets.
fn current_version(state: &GameState, intent: &Intent) -> Result<Version, RejectReason> {
    let version = match intent {
        Intent::UpdateCampaignMeta { .. } => Some(state.campaign.version),
        Intent::UpdateMap { id, .. } | Intent::DeleteMap { id } => {
            state.maps.get(id).map(|m| m.version)
        }
        Intent::UpdateToken { id, .. }
        | Intent::DeleteToken { id }
        | Intent::MoveToken { id, .. } => state.tokens.get(id).map(|t| t.version),
        Intent::UpdateCharacter { id, .. }
        | Intent::SubmitCharacter { id }
        | Intent::ApproveCharacter { id }
        | Intent::ReturnCharacter { id }
        | Intent::ArchiveCharacter { id }
        | Intent::AssignCharacterOwner { id, .. } => state.characters.get(id).map(|c| c.version),
        Intent::UpdateJournalEntry { id, .. }
        | Intent::MoveJournalEntry { id, .. }
        | Intent::DeleteJournalEntry { id } => state.journal.get(id).map(|e| e.version),
        Intent::CreateMap { .. }
        | Intent::CreateToken { .. }
        | Intent::CreateCharacter { .. }
        | Intent::CreateJournalEntry { .. }
        | Intent::SendMessage { .. }
        | Intent::RollDice { .. }
        | Intent::RegisterAsset { .. } => None,
    };
    version.ok_or(RejectReason::NotFound)
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
    if cell_size <= 0.0 {
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
fn id_in_use(state: &GameState, id: dnd_protocol::Uuid) -> bool {
    state.maps.contains_key(&dnd_protocol::MapId(id))
        || state.tokens.contains_key(&dnd_protocol::TokenId(id))
        || state
            .characters
            .contains_key(&dnd_protocol::CharacterId(id))
        || state
            .journal
            .contains_key(&dnd_protocol::JournalEntryId(id))
        || state.assets.contains_key(&dnd_protocol::AssetId(id))
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
fn on_map(map: &MapDto, x: i64, y: i64) -> Result<(i64, i64), RejectReason> {
    let per = dnd_protocol::UNITS16_PER_UNIT;
    if !(0..=map.width.saturating_mul(per)).contains(&x) {
        return Err(RejectReason::invalid("x"));
    }
    if !(0..=map.height.saturating_mul(per)).contains(&y) {
        return Err(RejectReason::invalid("y"));
    }
    Ok((x, y))
}
