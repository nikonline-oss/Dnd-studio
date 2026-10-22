//! `evolve`: applies an event to state. Used by the authority after `apply` and by every replica.

use dnd_protocol::{Event, FogDto};

use crate::state::GameState;

/// Applies one event. Idempotent for create/update/delete events, so replaying a batch twice
/// (e.g. after a resync) is harmless.
pub fn evolve(state: &mut GameState, event: &Event) {
    match event {
        Event::CampaignMetaUpdated { campaign } => state.campaign = campaign.clone(),
        Event::MapCreated { map } | Event::MapUpdated { map } => {
            state.maps.insert(map.id, map.clone());
        }
        Event::MapDeleted { id } => {
            state.maps.remove(id);
            state.tokens.retain(|_, t| t.map_id != *id);
            state.fog.remove(id);
        }
        // Fog of an unknown map is ignored, like a move of an unknown token.
        Event::FogApplied { map_id, op } if state.maps.contains_key(map_id) => {
            let fog = state.fog.entry(*map_id).or_insert_with(|| FogDto {
                map_id: *map_id,
                covered: false,
                ops: Vec::new(),
            });
            // Idempotent: a replayed event must not draw the shape twice.
            if !fog.ops.iter().any(|o| o.id == op.id) {
                fog.ops.push(op.clone());
            }
        }
        Event::FogReset { map_id, covered } if state.maps.contains_key(map_id) => {
            state.fog.insert(
                *map_id,
                FogDto {
                    map_id: *map_id,
                    covered: *covered,
                    ops: Vec::new(),
                },
            );
        }
        Event::FogApplied { .. } | Event::FogReset { .. } => {}
        Event::TokenCreated { token } | Event::TokenUpdated { token } => {
            state.tokens.insert(token.id, token.clone());
        }
        Event::TokenMoved { id, x, y } => {
            if let Some(token) = state.tokens.get_mut(id) {
                token.x = *x;
                token.y = *y;
            }
        }
        Event::TokenDeleted { id } => {
            state.tokens.remove(id);
        }
        Event::CharacterCreated { character } | Event::CharacterUpdated { character } => {
            state.characters.insert(character.id, character.clone());
        }
        Event::JournalEntryCreated { entry } | Event::JournalEntryUpdated { entry } => {
            state.journal.insert(entry.id, entry.clone());
        }
        Event::JournalEntryDeleted { id } => {
            state.journal.remove(id);
        }
        Event::AssetRegistered { asset } => {
            state.assets.insert(asset.id, asset.as_ref().clone());
        }
        // Chat history is not part of the game state (see `chat`).
        Event::ChatMessagePosted { .. } => {}
    }
}

/// Applies a sequence of events in order.
pub fn evolve_all<'a>(state: &mut GameState, events: impl IntoIterator<Item = &'a Event>) {
    for event in events {
        evolve(state, event);
    }
}
