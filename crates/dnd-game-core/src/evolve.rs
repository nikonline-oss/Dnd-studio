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
            // The map's combat goes with it (FK cascade in the database).
            let ended: Vec<_> = state
                .combats
                .values()
                .filter(|c| c.map_id == *id)
                .map(|c| c.id)
                .collect();
            for combat in ended {
                end_combat(state, combat);
            }
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
            // Its combatant stays in the fight without a token (ON DELETE SET NULL).
            for combatant in state.combatants.values_mut() {
                if combatant.token_id == Some(*id) {
                    combatant.token_id = None;
                }
            }
        }
        Event::CombatStarted { combat } | Event::CombatUpdated { combat } => {
            state.combats.insert(combat.id, combat.clone());
        }
        Event::CombatEnded { id } => end_combat(state, *id),
        Event::CombatantAdded { combatant } | Event::CombatantUpdated { combatant } => {
            state.combatants.insert(combatant.id, combatant.clone());
        }
        Event::CombatantRemoved { id } => {
            state.combatants.remove(id);
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
        Event::JournalFolderCreated { folder } | Event::JournalFolderUpdated { folder } => {
            state.journal_folders.insert(folder.id, folder.clone());
        }
        Event::JournalFolderDeleted { id } => {
            state.journal_folders.remove(id);
        }
        Event::AssetRegistered { asset } => {
            state.assets.insert(asset.id, asset.as_ref().clone());
        }
        Event::PluginInstalled { plugin } => {
            state
                .plugins
                .insert(plugin.id.clone(), plugin.as_ref().clone());
        }
        Event::PluginUninstalled { id } => {
            state.plugins.remove(id);
        }
        Event::SystemPluginSet { id } => state.system_plugin.clone_from(id),
        Event::BoardCreated { board } | Event::BoardUpdated { board } => {
            state.boards.insert(board.id, board.clone());
        }
        Event::BoardDeleted { id } => {
            // Its nodes and edges went first; dropping leftovers keeps replicas consistent.
            state.boards.remove(id);
            state.board_nodes.retain(|_, n| n.board_id != *id);
            state.board_edges.retain(|_, e| e.board_id != *id);
        }
        Event::BoardNodeCreated { node } | Event::BoardNodeUpdated { node } => {
            state.board_nodes.insert(node.id, node.clone());
        }
        Event::BoardNodeMoved { id, x, y } => {
            if let Some(node) = state.board_nodes.get_mut(id) {
                node.x = *x;
                node.y = *y;
            }
        }
        Event::BoardNodeDeleted { id } => {
            state.board_nodes.remove(id);
            state
                .board_edges
                .retain(|_, e| e.from != *id && e.to != *id);
        }
        Event::BoardEdgeCreated { edge } | Event::BoardEdgeUpdated { edge } => {
            state.board_edges.insert(edge.id, edge.clone());
        }
        Event::BoardEdgeDeleted { id } => {
            state.board_edges.remove(id);
        }
        Event::UserJoined { user } => {
            state.users.insert(user.id, user.clone());
        }
        // Chat history is not part of the game state (see `chat`); redacted events hide nothing
        // to apply.
        Event::ChatMessagePosted { .. } | Event::Redacted => {}
    }
}

fn end_combat(state: &mut GameState, id: dnd_protocol::CombatId) {
    state.combats.remove(&id);
    state.combatants.retain(|_, c| c.combat_id != id);
}

/// Applies a sequence of events in order.
pub fn evolve_all<'a>(state: &mut GameState, events: impl IntoIterator<Item = &'a Event>) {
    for event in events {
        evolve(state, event);
    }
}
