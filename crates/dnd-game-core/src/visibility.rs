//! What each recipient may see (ADR-0022). GM and Co-GM see everything; players and spectators
//! see the players' view:
//!
//! - maps marked `visible_to_players`, with their fog and combats;
//! - tokens that are `visible`, not on the GM layer, on a visible map;
//! - combatants of visible combats whose token (if any) is visible;
//! - journal entries open to players, and the folders on their way to the root (ADR-0021);
//! - assets used by a visible map or token;
//! - chat messages not hidden for GM roles;
//! - boards shown to players, their nodes open to players, and edges open to players between two
//!   visible nodes (ADR-0024); pictures on visible nodes;
//! - characters, users and campaign metadata as they are;
//! - never zones or triggers (ADR-0024).
//!
//! A batch reaches players with hidden events replaced by [`Event::Redacted`], so sequence numbers
//! stay contiguous. When an entity that exists before and after the batch becomes visible or
//! hidden (a map revealed, a token moved off the GM layer, an entry shared), patching the
//! players' copy event by event is not possible: they get a fresh snapshot instead.

use std::collections::BTreeSet;

use dnd_protocol::{
    AssetId, BoardEdgeId, BoardId, BoardNodeContent, BoardNodeId, BoardVisibility, ChatMessageDto,
    ChatVisibility, CombatId, CombatantId, Event, EventBatch, JournalEntryId, JournalFolderId,
    JournalVisibility, MapId, Role, Seq, Snapshot, TokenId, TokenLayer,
};

use crate::state::GameState;

/// The pictures a journal text shows: `![…](asset:<asset id>)` (ADR-0024).
pub fn journal_pictures(markdown: &str) -> impl Iterator<Item = AssetId> + '_ {
    markdown.split("](asset:").skip(1).filter_map(|rest| {
        let id = rest.split(')').next()?.trim();
        id.parse().ok().map(AssetId)
    })
}

/// Whether `role` sees the full state.
#[must_use]
pub fn sees_everything(role: Role) -> bool {
    matches!(role, Role::Gm | Role::CoGm)
}

/// Whether players may read a chat message.
#[must_use]
pub fn chat_visible_to_players(message: &ChatMessageDto) -> bool {
    message.visibility == ChatVisibility::All
}

/// Ids visible to players in one state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PlayersSee {
    pub maps: BTreeSet<MapId>,
    pub tokens: BTreeSet<TokenId>,
    pub entries: BTreeSet<JournalEntryId>,
    pub folders: BTreeSet<JournalFolderId>,
    pub assets: BTreeSet<AssetId>,
    pub combats: BTreeSet<CombatId>,
    pub combatants: BTreeSet<CombatantId>,
    pub boards: BTreeSet<BoardId>,
    pub board_nodes: BTreeSet<BoardNodeId>,
    pub board_edges: BTreeSet<BoardEdgeId>,
}

impl PlayersSee {
    #[must_use]
    pub fn of(state: &GameState) -> Self {
        let maps: BTreeSet<_> = state
            .maps
            .values()
            .filter(|m| m.visible_to_players)
            .map(|m| m.id)
            .collect();
        let tokens: BTreeSet<_> = state
            .tokens
            .values()
            .filter(|t| t.visible && t.layer != TokenLayer::Gm && maps.contains(&t.map_id))
            .map(|t| t.id)
            .collect();
        let entries: BTreeSet<_> = state
            .journal
            .values()
            .filter(|e| e.visibility == JournalVisibility::Players)
            .map(|e| e.id)
            .collect();
        let mut folders = BTreeSet::new();
        for entry in entries.iter().filter_map(|id| state.journal.get(id)) {
            let mut next = entry.folder_id;
            // `insert` returning false also stops a (never expected) cycle.
            while let Some(id) = next {
                if !folders.insert(id) {
                    break;
                }
                next = state.journal_folders.get(&id).and_then(|f| f.parent);
            }
        }
        let boards: BTreeSet<_> = state
            .boards
            .values()
            .filter(|b| b.visible_to_players)
            .map(|b| b.id)
            .collect();
        let board_nodes: BTreeSet<_> = state
            .board_nodes
            .values()
            .filter(|n| {
                n.visibility == BoardVisibility::Players
                    && boards.contains(&n.board_id)
                    && !matches!(n.content, BoardNodeContent::Trigger { .. })
            })
            .map(|n| n.id)
            .collect();
        let board_edges = state
            .board_edges
            .values()
            .filter(|e| {
                e.visibility == BoardVisibility::Players
                    && board_nodes.contains(&e.from)
                    && board_nodes.contains(&e.to)
            })
            .map(|e| e.id)
            .collect();
        let assets = state
            .maps
            .values()
            .filter(|m| maps.contains(&m.id))
            .filter_map(|m| m.background_asset_id)
            .chain(
                state
                    .tokens
                    .values()
                    .filter(|t| tokens.contains(&t.id))
                    .filter_map(|t| t.asset_id),
            )
            .chain(
                state
                    .board_nodes
                    .values()
                    .filter(|n| board_nodes.contains(&n.id))
                    .filter_map(|n| match n.content {
                        BoardNodeContent::Image { asset_id } => Some(asset_id),
                        _ => None,
                    }),
            )
            .chain(
                state
                    .journal
                    .values()
                    .filter(|e| entries.contains(&e.id))
                    .flat_map(|e| journal_pictures(&e.markdown)),
            )
            .collect();
        let combats: BTreeSet<_> = state
            .combats
            .values()
            .filter(|c| maps.contains(&c.map_id))
            .map(|c| c.id)
            .collect();
        let combatants = state
            .combatants
            .values()
            .filter(|c| {
                combats.contains(&c.combat_id) && c.token_id.is_none_or(|t| tokens.contains(&t))
            })
            .map(|c| c.id)
            .collect();
        Self {
            maps,
            tokens,
            entries,
            folders,
            assets,
            combats,
            combatants,
            boards,
            board_nodes,
            board_edges,
        }
    }
}

/// The players' copy of `state` (see the module docs).
#[must_use]
pub fn players_state(state: &GameState) -> GameState {
    let see = PlayersSee::of(state);
    let mut view = state.clone();
    view.maps.retain(|id, _| see.maps.contains(id));
    view.tokens.retain(|id, _| see.tokens.contains(id));
    view.journal.retain(|id, _| see.entries.contains(id));
    view.journal_folders
        .retain(|id, _| see.folders.contains(id));
    view.assets.retain(|id, _| see.assets.contains(id));
    view.fog.retain(|id, _| see.maps.contains(id));
    view.combats.retain(|id, _| see.combats.contains(id));
    view.combatants.retain(|id, _| see.combatants.contains(id));
    view.boards.retain(|id, _| see.boards.contains(id));
    view.board_nodes
        .retain(|id, _| see.board_nodes.contains(id));
    view.board_edges
        .retain(|id, _| see.board_edges.contains(id));
    view.zones.clear();
    view.triggers.clear();
    view.library_folders.clear();
    for asset in view.assets.values_mut() {
        asset.folder_id = None;
    }
    view
}

/// What `role` receives on connect: the state (filtered unless GM role) and the recent chat.
#[must_use]
pub fn snapshot_for<'a>(
    role: Role,
    state: &GameState,
    seq: Seq,
    chat: impl IntoIterator<Item = &'a ChatMessageDto>,
) -> Snapshot {
    let everything = sees_everything(role);
    let mut snapshot = if everything {
        state.to_snapshot(seq)
    } else {
        players_state(state).to_snapshot(seq)
    };
    snapshot.chat = chat
        .into_iter()
        .filter(|m| everything || chat_visible_to_players(m))
        .cloned()
        .collect();
    snapshot
}

/// A batch as players receive it.
#[derive(Clone, Debug, PartialEq)]
pub enum ForPlayers {
    /// Same sequence numbers; hidden events are [`Event::Redacted`].
    Batch(EventBatch),
    /// Visibility changed in a way events cannot express: send a fresh snapshot.
    Resnapshot,
}

/// Whether `batch` might change what players can see. When it cannot (token drags, chat, fog —
/// the common case), [`filter_for_players`] decides event by event and never looks at `before`,
/// so the caller need not keep a copy of the state.
#[must_use]
pub fn may_change_visibility(batch: &EventBatch) -> bool {
    !batch.events.iter().all(|e| {
        matches!(
            e,
            Event::TokenMoved { .. }
                | Event::BoardNodeMoved { .. }
                | Event::ChatMessagePosted { .. }
                | Event::FogApplied { .. }
                | Event::FogReset { .. }
                | Event::CharacterCreated { .. }
                | Event::CharacterUpdated { .. }
                | Event::CampaignMetaUpdated { .. }
                | Event::UserJoined { .. }
                | Event::CombatUpdated { .. }
                | Event::Redacted
        )
    })
}

/// Filters a committed `batch` for players; `before` and `after` are the full states around it.
#[must_use]
pub fn filter_for_players(before: &GameState, after: &GameState, batch: &EventBatch) -> ForPlayers {
    if !may_change_visibility(batch) {
        return ForPlayers::Batch(EventBatch {
            events: batch.events.iter().map(|e| cheap_event(after, e)).collect(),
            ..batch.clone()
        });
    }

    let was = PlayersSee::of(before);
    let now = PlayersSee::of(after);
    if flipped(&was.maps, &now.maps, |id| {
        before.maps.contains_key(id) && after.maps.contains_key(id)
    }) || flipped(&was.tokens, &now.tokens, |id| {
        before.tokens.contains_key(id) && after.tokens.contains_key(id)
    }) || flipped(&was.entries, &now.entries, |id| {
        before.journal.contains_key(id) && after.journal.contains_key(id)
    }) || flipped(&was.folders, &now.folders, |id| {
        before.journal_folders.contains_key(id) && after.journal_folders.contains_key(id)
    }) || flipped(&was.assets, &now.assets, |id| {
        before.assets.contains_key(id) && after.assets.contains_key(id)
    }) || flipped(&was.combats, &now.combats, |id| {
        before.combats.contains_key(id) && after.combats.contains_key(id)
    }) || flipped(&was.combatants, &now.combatants, |id| {
        before.combatants.contains_key(id) && after.combatants.contains_key(id)
    }) || flipped(&was.boards, &now.boards, |id| {
        before.boards.contains_key(id) && after.boards.contains_key(id)
    }) || flipped(&was.board_nodes, &now.board_nodes, |id| {
        before.board_nodes.contains_key(id) && after.board_nodes.contains_key(id)
    }) || flipped(&was.board_edges, &now.board_edges, |id| {
        before.board_edges.contains_key(id) && after.board_edges.contains_key(id)
    }) {
        return ForPlayers::Resnapshot;
    }

    // Created or changed: visible if it is visible after the batch (or, when a later event of the
    // batch deleted it, before). Deleted: visible if it was visible before.
    let shown = |now_has: bool, now_sees: bool, was_sees: bool| {
        if now_has { now_sees } else { was_sees }
    };
    let events = batch
        .events
        .iter()
        .map(|event| {
            let visible = match event {
                        Event::MapCreated { map } | Event::MapUpdated { map } => shown(
                            after.maps.contains_key(&map.id),
                            now.maps.contains(&map.id),
                            was.maps.contains(&map.id),
                        ),
                        Event::MapDeleted { id } => was.maps.contains(id),
                        Event::FogApplied { map_id, .. } | Event::FogReset { map_id, .. } => shown(
                            after.maps.contains_key(map_id),
                            now.maps.contains(map_id),
                            was.maps.contains(map_id),
                        ),
                        Event::TokenCreated { token } | Event::TokenUpdated { token } => shown(
                            after.tokens.contains_key(&token.id),
                            now.tokens.contains(&token.id),
                            was.tokens.contains(&token.id),
                        ),
                        Event::TokenMoved { id, .. } => shown(
                            after.tokens.contains_key(id),
                            now.tokens.contains(id),
                            was.tokens.contains(id),
                        ),
                        Event::TokenDeleted { id } => was.tokens.contains(id),
                        Event::JournalEntryCreated { entry }
                        | Event::JournalEntryUpdated { entry } => shown(
                            after.journal.contains_key(&entry.id),
                            now.entries.contains(&entry.id),
                            was.entries.contains(&entry.id),
                        ),
                        Event::JournalEntryDeleted { id } => was.entries.contains(id),
                        Event::JournalFolderCreated { folder }
                        | Event::JournalFolderUpdated { folder } => shown(
                            after.journal_folders.contains_key(&folder.id),
                            now.folders.contains(&folder.id),
                            was.folders.contains(&folder.id),
                        ),
                        Event::JournalFolderDeleted { id } => was.folders.contains(id),
                        Event::AssetRegistered { asset } => shown(
                            after.assets.contains_key(&asset.id),
                            now.assets.contains(&asset.id),
                            was.assets.contains(&asset.id),
                        ),
                        Event::CombatStarted { combat } | Event::CombatUpdated { combat } => shown(
                            after.combats.contains_key(&combat.id),
                            now.combats.contains(&combat.id),
                            was.combats.contains(&combat.id),
                        ),
                        Event::CombatEnded { id } => was.combats.contains(id),
                        Event::CombatantAdded { combatant }
                        | Event::CombatantUpdated { combatant } => shown(
                            after.combatants.contains_key(&combatant.id),
                            now.combatants.contains(&combatant.id),
                            was.combatants.contains(&combatant.id),
                        ),
                        Event::CombatantRemoved { id } => was.combatants.contains(id),
                        Event::BoardCreated { board } | Event::BoardUpdated { board } => shown(
                            after.boards.contains_key(&board.id),
                            now.boards.contains(&board.id),
                            was.boards.contains(&board.id),
                        ),
                        Event::BoardDeleted { id } => was.boards.contains(id),
                        Event::BoardNodeCreated { node } | Event::BoardNodeUpdated { node } => {
                            shown(
                                after.board_nodes.contains_key(&node.id),
                                now.board_nodes.contains(&node.id),
                                was.board_nodes.contains(&node.id),
                            )
                        }
                        Event::BoardNodeMoved { id, .. } => shown(
                            after.board_nodes.contains_key(id),
                            now.board_nodes.contains(id),
                            was.board_nodes.contains(id),
                        ),
                        Event::BoardNodeDeleted { id } => was.board_nodes.contains(id),
                        Event::BoardEdgeCreated { edge } | Event::BoardEdgeUpdated { edge } => {
                            shown(
                                after.board_edges.contains_key(&edge.id),
                                now.board_edges.contains(&edge.id),
                                was.board_edges.contains(&edge.id),
                            )
                        }
                        Event::BoardEdgeDeleted { id } => was.board_edges.contains(id),
                        // Zones are the GM's tool (ADR-0024).
                        Event::ZoneCreated { .. }
                        | Event::ZoneUpdated { .. }
                        | Event::ZoneDeleted { .. }
                        | Event::TriggerCreated { .. }
                        | Event::TriggerUpdated { .. }
                        | Event::TriggerDeleted { .. }
                        | Event::TriggerFired { .. }
                        // Libraries are the GM's (ADR-0026).
                        | Event::LibraryFolderCreated { .. }
                        | Event::LibraryFolderUpdated { .. }
                        | Event::LibraryFolderDeleted { .. } => false,
                        Event::AssetUpdated { asset } => shown(
                            after.assets.contains_key(&asset.id),
                            now.assets.contains(&asset.id),
                            was.assets.contains(&asset.id),
                        ),
                        Event::ChatMessagePosted { message } => chat_visible_to_players(message),
                        Event::CampaignMetaUpdated { .. }
                        | Event::CharacterCreated { .. }
                        | Event::CharacterUpdated { .. }
                        | Event::UserJoined { .. }
                        // Rules are the same for everyone.
                        | Event::PluginInstalled { .. }
                        | Event::PluginUninstalled { .. }
                        | Event::SystemPluginSet { .. }
                        // Everyone hears the table's sounds.
                        | Event::SoundPlayed { .. }
                        | Event::SoundStopped { .. }
                        | Event::Redacted => true,
                    };
            if visible {
                event.clone()
            } else {
                Event::Redacted
            }
        })
        .collect();
    ForPlayers::Batch(EventBatch {
        events,
        ..batch.clone()
    })
}

/// [`filter_for_players`] for events that cannot change visibility: decided on the spot.
fn cheap_event(after: &GameState, event: &Event) -> Event {
    let token_shown = |id: &TokenId| {
        after.tokens.get(id).is_some_and(|t| {
            t.visible
                && t.layer != TokenLayer::Gm
                && after
                    .maps
                    .get(&t.map_id)
                    .is_some_and(|m| m.visible_to_players)
        })
    };
    let map_shown = |id: &MapId| after.maps.get(id).is_some_and(|m| m.visible_to_players);
    let visible = match event {
        Event::TokenMoved { id, .. } => token_shown(id),
        Event::BoardNodeMoved { id, .. } => after.board_nodes.get(id).is_some_and(|n| {
            n.visibility == BoardVisibility::Players
                && after
                    .boards
                    .get(&n.board_id)
                    .is_some_and(|b| b.visible_to_players)
        }),
        Event::ChatMessagePosted { message } => chat_visible_to_players(message),
        Event::FogApplied { map_id, .. } | Event::FogReset { map_id, .. } => map_shown(map_id),
        Event::CombatUpdated { combat } => map_shown(&combat.map_id),
        _ => true,
    };
    if visible {
        event.clone()
    } else {
        Event::Redacted
    }
}

/// True when an id present in both states (`in_both`) is in exactly one of the visible sets.
fn flipped<T: Ord>(was: &BTreeSet<T>, now: &BTreeSet<T>, in_both: impl Fn(&T) -> bool) -> bool {
    was.symmetric_difference(now).any(in_both)
}
