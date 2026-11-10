//! What each recipient may see (ADR-0022). GM and Co-GM see everything; players and spectators
//! see the players' view:
//!
//! - maps marked `visible_to_players` or linked by a shared card on a shared board, with their fog and combats;
//! - tokens that are `visible`, not on the GM layer, on a visible map;
//! - combatants of visible combats whose token (if any) is visible;
//! - journal entries open to players, and the folders on their way to the root (ADR-0021);
//! - assets used by a visible map or token;
//! - chat messages not hidden for GM roles;
//! - boards shown to players, their nodes open to players, and edges open to players between two
//!   visible nodes (ADR-0024); pictures on visible nodes;
//! - characters players may know about (active PCs, characters on visible tokens or in visible
//!   combats), with sheets reduced to the fields their system marks `visible_to_players` (ADR-0049);
//! - users and campaign metadata as they are;
//! - effects on visible tokens (ADR-0024);
//! - never zones or triggers (ADR-0024).
//!
//! Every wire snapshot is built by [`personal_snapshot`] from a [`Viewer`] (the role and, when
//! known, the person); `snapshot_for` and `snapshot_for_user` are shorthands for it (ADR-0054).
//!
//! A batch reaches players with hidden events replaced by [`Event::Redacted`], so sequence numbers
//! stay contiguous. When an entity that exists before and after the batch becomes visible or
//! hidden (a map revealed, a token moved off the GM layer, an entry shared), patching the
//! players' copy event by event is not possible: they get a fresh snapshot instead.

use std::collections::BTreeSet;

use dnd_protocol::{
    AssetId, BoardEdgeId, BoardId, BoardLink, BoardNodeContent, BoardNodeId, BoardVisibility,
    CharacterDto, CharacterId, CharacterKind, CharacterStatus, ChatMessageDto, ChatVisibility,
    CombatId, CombatantId, EffectId, Event, EventBatch, JournalEntryId, JournalFolderId,
    JournalVisibility, MapId, Role, Seq, Snapshot, TokenDto, TokenField, TokenId,
    TokenInspectorView, TokenLayer,
};

use crate::state::GameState;

/// The pictures a journal text shows: `![…](asset:<asset id>)` (ADR-0024).
pub fn journal_pictures(markdown: &str) -> impl Iterator<Item = AssetId> + '_ {
    markdown.split("](asset:").skip(1).filter_map(|rest| {
        let id = rest.split(')').next()?.trim();
        id.parse().ok().map(AssetId)
    })
}

pub fn journal_playlists(markdown: &str) -> impl Iterator<Item = dnd_protocol::PlaylistId> + '_ {
    markdown.split("](playlist:").skip(1).filter_map(|rest| {
        rest.split(')')
            .next()?
            .trim()
            .parse()
            .ok()
            .map(dnd_protocol::PlaylistId)
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

/// Effective visibility, without changing the map's explicit flag (ADR-0024).
pub(crate) fn map_visible_to_players(state: &GameState, id: &MapId) -> bool {
    state.maps.get(id).is_some_and(|map| {
        map.visible_to_players
            || state.board_nodes.values().any(|node| {
                node.visibility == BoardVisibility::Players
                    && matches!(node.content, BoardNodeContent::Link {
                        target: BoardLink::Map { id: target },
                    } if target == *id)
                    && state
                        .boards
                        .get(&node.board_id)
                        .is_some_and(|board| board.visible_to_players)
            })
    })
}

/// Ids visible to players in one state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PlayersSee {
    pub playlists: BTreeSet<dnd_protocol::PlaylistId>,
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
    /// Characters players may know about (ADR-0049): active PCs and anyone attached to a visible
    /// token, combatant or board card. Their sheets are still reduced to the public fields.
    pub characters: BTreeSet<CharacterId>,
    /// Effects ride on their token's visibility (ADR-0024).
    pub effects: BTreeSet<EffectId>,
}

impl PlayersSee {
    #[must_use]
    pub fn of(state: &GameState) -> Self {
        let maps: BTreeSet<_> = state
            .maps
            .values()
            .filter(|m| map_visible_to_players(state, &m.id))
            .map(|m| m.id)
            .collect();
        let tokens: BTreeSet<_> = state
            .tokens
            .values()
            .filter(|t| t.visible && t.layer != TokenLayer::Gm && maps.contains(&t.map_id))
            .map(|t| t.id)
            .collect();
        let effects: BTreeSet<_> = state
            .effects
            .values()
            .filter(|e| tokens.contains(&e.token))
            .map(|e| e.id)
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
            .chain(state.presented_image.iter().map(|p| p.asset_id))
            .collect();
        let combats: BTreeSet<_> = state
            .combats
            .values()
            .filter(|c| maps.contains(&c.map_id))
            .map(|c| c.id)
            .collect();
        let combatants: BTreeSet<_> = state
            .combatants
            .values()
            .filter(|c| {
                combats.contains(&c.combat_id) && c.token_id.is_none_or(|t| tokens.contains(&t))
            })
            .map(|c| c.id)
            .collect();
        let linked: BTreeSet<_> = state
            .tokens
            .values()
            .filter(|t| tokens.contains(&t.id))
            .filter_map(|t| t.character_id)
            .chain(
                state
                    .combatants
                    .values()
                    .filter(|c| combatants.contains(&c.id))
                    .filter_map(|c| c.character_id),
            )
            .chain(state.board_nodes.values().filter_map(|node| {
                if !board_nodes.contains(&node.id) {
                    return None;
                }
                match &node.content {
                    BoardNodeContent::Link {
                        target: BoardLink::Character { id },
                    } => Some(*id),
                    _ => None,
                }
            }))
            .collect();
        let characters = state
            .characters
            .values()
            .filter(|c| {
                linked.contains(&c.id)
                    || (c.kind == CharacterKind::Pc && c.status == CharacterStatus::Active)
            })
            .map(|c| c.id)
            .collect();
        Self {
            playlists: state
                .journal
                .values()
                .filter(|e| entries.contains(&e.id))
                .flat_map(|e| journal_playlists(&e.markdown))
                .filter(|id| state.playlists.contains_key(id))
                .collect(),
            maps,
            tokens,
            effects,
            entries,
            folders,
            assets,
            combats,
            combatants,
            boards,
            board_nodes,
            board_edges,
            characters,
        }
    }
}

/// The inspector rows one token shows this recipient (ADR-0061), in inspector order.
///
/// The campaign policy opens the default set, the token's override adds and closes fields, and a
/// `conceal` always wins over a `reveal` of the same source. An active effect's `reveal` is added
/// last and cannot be closed: a spell that reveals a field shows it until the effect expires.
#[must_use]
pub fn token_fields(state: &GameState, token: &TokenDto, viewer: Viewer) -> Vec<TokenField> {
    if viewer.sees_everything() {
        return TokenField::ALL.to_vec();
    }
    let mut open: BTreeSet<TokenField> = BTreeSet::new();
    let add = |open: &mut BTreeSet<TokenField>, policy: &dnd_protocol::TokenInspectorPolicy| {
        open.extend(policy.reveal.iter().copied());
        for field in &policy.conceal {
            open.remove(field);
        }
    };
    add(&mut open, &state.campaign.token_inspector);
    if let Some(override_policy) = &token.inspector {
        add(&mut open, override_policy);
    }
    for effect in state.effects.values().filter(|e| e.token == token.id) {
        open.extend(effect.reveal.iter().copied());
    }
    TokenField::ALL
        .iter()
        .filter(|field| open.contains(field))
        .copied()
        .collect()
}

/// The character as players and spectators receive it (ADR-0049): the owner and GM roles get the
/// full sheet, everyone else only the fields the system marks as public, with the GM's overrides
/// on top (ADR-0063). A hidden name travels empty; the client shows a placeholder.
#[must_use]
pub fn public_character(state: &GameState, character: &CharacterDto) -> CharacterDto {
    CharacterDto {
        name: if state.campaign.visibility.name_public() {
            character.name.clone()
        } else {
            String::new()
        },
        sheet_json: public_sheet(state, &character.sheet_json),
        ..character.clone()
    }
}

fn public_sheet(state: &GameState, sheet: &str) -> String {
    let Some(schema) = state.rules().map(|rules| &rules.sheet_schema) else {
        return "{}".to_owned();
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(sheet) else {
        return "{}".to_owned();
    };
    let mut public = dnd_protocol::sheet_schema::public_data(schema, &data);
    // The GM's overrides come last, on the data itself: a rule may open a field the schema hides,
    // close a field it opens, and a deeper rule wins over a shallower one (ADR-0063).
    for rule in &state.campaign.visibility.sheet {
        let segments: Vec<&str> = rule.path.split('.').collect();
        if rule.visible {
            if let Some(value) = get_path(&data, &segments) {
                set_path(&mut public, &segments, value.clone());
            }
        } else {
            remove_path(&mut public, &segments);
        }
    }
    public.to_string()
}

fn get_path<'a>(value: &'a serde_json::Value, segments: &[&str]) -> Option<&'a serde_json::Value> {
    let mut node = value;
    for segment in segments {
        node = node.get(*segment)?;
    }
    Some(node)
}

fn set_path(target: &mut serde_json::Value, segments: &[&str], value: serde_json::Value) {
    let Some((first, rest)) = segments.split_first() else {
        *target = value;
        return;
    };
    if !target.is_object() {
        *target = serde_json::Value::Object(serde_json::Map::new());
    }
    let Some(object) = target.as_object_mut() else {
        return;
    };
    let child = object
        .entry((*first).to_owned())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    set_path(child, rest, value);
}

fn remove_path(target: &mut serde_json::Value, segments: &[&str]) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    if let Some(object) = target.as_object_mut() {
        if rest.is_empty() {
            object.remove(*first);
        } else if let Some(child) = object.get_mut(*first) {
            remove_path(child, rest);
        }
    }
}

/// The token as players receive it: the GM's override is not theirs to read (ADR-0061), and a
/// hidden picture leaves the token drawn by its initial (ADR-0063).
#[must_use]
pub fn public_token(state: &GameState, token: &TokenDto) -> TokenDto {
    TokenDto {
        inspector: None,
        asset_id: if state.campaign.visibility.icon_public() {
            token.asset_id
        } else {
            None
        },
        ..token.clone()
    }
}

/// The players' copy of `state` (see the module docs).
#[must_use]
pub fn players_state(state: &GameState) -> GameState {
    let see = PlayersSee::of(state);
    let mut view = state.clone();
    view.maps.retain(|id, _| see.maps.contains(id));
    // Player-facing flags describe effective access, including board links. Authority and GM
    // snapshots keep the explicit flag; clients need not reconstruct server visibility rules.
    for map in view.maps.values_mut() {
        map.visible_to_players = true;
    }
    view.tokens.retain(|id, _| see.tokens.contains(id));
    // The inspector override is GM bookkeeping; players get resolved fields (ADR-0061). A hidden
    // picture leaves the token drawn by its initial (ADR-0063).
    for token in view.tokens.values_mut() {
        token.inspector = None;
        if !state.campaign.visibility.icon_public() {
            token.asset_id = None;
        }
    }
    // Characters keep their identity for token and combatant references, but their sheets are
    // reduced to the fields the system marks public (ADR-0049).
    view.characters = std::mem::take(&mut view.characters)
        .into_iter()
        .filter(|(id, _)| see.characters.contains(id))
        .map(|(id, character)| (id, public_character(state, &character)))
        .collect();
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
    view.effects.retain(|id, _| see.effects.contains(id));
    view.items.retain(|_, i| i.visible_to_players);
    view.zones.clear();
    view.triggers.clear();
    // Ownership is recipient-specific; the common players projection must never disclose requests.
    view.roll_requests.clear();
    view.plugin_runtime = Default::default();
    view.library_folders.clear();
    view.playlists.retain(|id, _| see.playlists.contains(id));
    for playlist in view.playlists.values_mut() {
        playlist.folder_id = None;
    }
    for asset in view.assets.values_mut() {
        asset.folder_id = None;
    }
    view
}

/// A recipient of a personal view: the role and, when the caller knows it, the user (ADR-0054).
///
/// The pair is one value on purpose. `snapshot_for(role, …)` used to mean "this role in general",
/// which for a player silently dropped owner-specific data; with a [`Viewer`] the caller states
/// whether it has a specific person in mind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewer {
    role: Role,
    user: Option<dnd_protocol::UserId>,
}

impl Viewer {
    #[must_use]
    pub fn gm() -> Self {
        Self {
            role: Role::Gm,
            user: None,
        }
    }

    #[must_use]
    pub fn co_gm() -> Self {
        Self {
            role: Role::CoGm,
            user: None,
        }
    }

    #[must_use]
    pub fn player(user: dnd_protocol::UserId) -> Self {
        Self {
            role: Role::Player,
            user: Some(user),
        }
    }

    #[must_use]
    pub fn spectator(user: dnd_protocol::UserId) -> Self {
        Self {
            role: Role::Spectator,
            user: Some(user),
        }
    }

    /// The role alone, when no specific person is known.
    #[must_use]
    pub fn of(role: Role, user: Option<dnd_protocol::UserId>) -> Self {
        Self { role, user }
    }

    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    #[must_use]
    pub fn user(&self) -> Option<dnd_protocol::UserId> {
        self.user
    }

    #[must_use]
    pub fn sees_everything(&self) -> bool {
        sees_everything(self.role)
    }

    /// Whether this recipient holds the character's full sheet: GM roles always, a player for a
    /// character they own. A spectator gets the public sheet even if it is recorded as the
    /// owner — the session role, not ownership alone, decides what a view may carry (ADR-0054).
    #[must_use]
    pub fn owns(&self, character: &CharacterDto) -> bool {
        self.sees_everything()
            || (self.role == Role::Player && self.user.is_some() && character.owner == self.user)
    }
}

/// What one recipient receives on connect: the state filtered for the role and, when the user is
/// known, for that person (ADR-0022, ADR-0049, ADR-0054). Every wire snapshot goes through here.
pub fn personal_snapshot<'a>(
    state: &GameState,
    viewer: Viewer,
    seq: Seq,
    chat: impl IntoIterator<Item = &'a ChatMessageDto>,
) -> Snapshot {
    let everything = viewer.sees_everything();
    let mut snapshot = if everything {
        state.to_snapshot(seq)
    } else {
        players_state(state).to_snapshot(seq)
    };
    // The resolved inspector rows ride with the snapshot, per recipient (ADR-0061).
    snapshot.token_inspector = if everything {
        snapshot
            .tokens
            .iter()
            .map(|token| TokenInspectorView {
                token: token.id,
                fields: TokenField::ALL.to_vec(),
            })
            .collect()
    } else {
        snapshot
            .tokens
            .iter()
            .map(|token| TokenInspectorView {
                token: token.id,
                fields: state
                    .tokens
                    .get(&token.id)
                    .map(|full| token_fields(state, full, viewer))
                    .unwrap_or_default(),
            })
            .collect()
    };
    snapshot.chat = chat
        .into_iter()
        .filter(|m| everything || chat_visible_to_players(m))
        .cloned()
        .collect();
    let Some(user) = viewer.user() else {
        // No specific person: the role-level projection stands, nothing owner-specific is added.
        return snapshot;
    };
    if !everything {
        let see = PlayersSee::of(state);
        snapshot.characters = state
            .characters
            .values()
            .filter_map(|character| {
                if viewer.owns(character) {
                    Some(character.clone())
                } else {
                    see.characters
                        .contains(&character.id)
                        .then(|| public_character(state, character))
                }
            })
            .collect();
        snapshot.rules_view = crate::rules_view::view(state);
        let known: BTreeSet<_> = snapshot.characters.iter().map(|c| c.id).collect();
        snapshot
            .rules_view
            .characters
            .retain(|c| known.contains(&c.character));
        // Action availability is per actor and is computed from the character's own resources
        // (ADR-0049): a recipient gets it only for characters they own, so spectators get none.
        snapshot.rules_view.tokens.retain(|view| {
            snapshot.tokens.iter().any(|v| v.id == view.token)
                && state
                    .tokens
                    .get(&view.token)
                    .and_then(|token| token.character_id)
                    .and_then(|id| state.characters.get(&id))
                    .is_some_and(|c| c.owner == Some(user))
        });
    }
    if viewer.role() != Role::Player {
        return snapshot;
    }
    snapshot.items = state
        .items
        .values()
        .filter(|i| {
            i.visible_to_players
                || i.owner
                    .and_then(|id| state.characters.get(&id))
                    .is_some_and(|c| c.owner == Some(user))
        })
        .cloned()
        .collect();
    let mut runtime = state.plugin_runtime.clone();
    let see = PlayersSee::of(state);
    runtime.trigger_runs.clear();
    runtime.saves.retain_mut(|g| {
        g.targets.retain(|t| {
            see.tokens.contains(&t.token)
                && state
                    .characters
                    .get(&t.character)
                    .is_some_and(|c| c.owner == Some(user))
        });
        if g.source.is_some_and(|id| !see.tokens.contains(&id)) {
            g.source = None;
        }
        !g.targets.is_empty()
    });
    runtime.action_stacks.retain_mut(|stack| {
        stack.frames.retain_mut(|frame| {
            let own = state
                .characters
                .get(&frame.owner)
                .is_some_and(|c| c.owner == Some(user));
            frame.offers.retain(|offer| {
                see.tokens.contains(&offer.source)
                    && see.tokens.contains(&frame.source)
                    && state
                        .tokens
                        .get(&offer.source)
                        .and_then(|t| t.character_id)
                        .and_then(|id| state.characters.get(&id))
                        .is_some_and(|c| c.owner == Some(user))
            });
            // A GM may declare an owned character's action against a hidden target.
            let declared_by_user = frame.controller == Some(user);
            if !own {
                frame.controller = None;
                frame.deferred.clear();
            }
            if !own || !declared_by_user || frame.target.is_some_and(|id| !see.tokens.contains(&id))
            {
                frame.target = None;
                frame.x = 0;
                frame.y = 0;
            }
            see.tokens.contains(&frame.source) && (own || !frame.offers.is_empty())
        });
        !stack.frames.is_empty() || see.maps.contains(&stack.map)
    });
    runtime.maintained.retain_mut(|entry| {
        let own = state
            .characters
            .get(&entry.owner)
            .is_some_and(|c| c.owner == Some(user))
            && see.tokens.contains(&entry.source);
        entry.effects.retain(|id| see.effects.contains(id));
        own
    });
    runtime.movements.retain_mut(|m| {
        m.handled.clear();
        m.offers.retain(|o| {
            state
                .tokens
                .get(&o.source)
                .and_then(|t| t.character_id)
                .and_then(|id| state.characters.get(&id))
                .is_some_and(|c| c.owner == Some(user))
                && see.tokens.contains(&o.source)
                && see.tokens.contains(&m.token)
        });
        !m.offers.is_empty()
            || (see.tokens.contains(&m.token)
                && state
                    .tokens
                    .get(&m.token)
                    .and_then(|t| t.character_id)
                    .and_then(|id| state.characters.get(&id))
                    .is_some_and(|c| c.owner == Some(user)))
    });
    runtime
        .visuals
        .retain(|v| see.maps.contains(&v.map) && see.tokens.contains(&v.source));
    snapshot.plugin_runtime = runtime;
    snapshot.roll_requests = state
        .roll_requests
        .values()
        .filter(|r| {
            state
                .characters
                .get(&r.character)
                .is_some_and(|c| c.owner == Some(user))
        })
        .cloned()
        .collect();
    snapshot
}

/// The role-level view: what a player or spectator receives when no person is known.
#[must_use]
pub fn snapshot_for<'a>(
    role: Role,
    state: &GameState,
    seq: Seq,
    chat: impl IntoIterator<Item = &'a ChatMessageDto>,
) -> Snapshot {
    personal_snapshot(state, Viewer::of(role, None), seq, chat)
}

/// The view of one person: owner-specific sheets, requests and runtime.
pub fn snapshot_for_user<'a>(
    role: Role,
    user: dnd_protocol::UserId,
    state: &GameState,
    seq: Seq,
    chat: impl IntoIterator<Item = &'a ChatMessageDto>,
) -> Snapshot {
    personal_snapshot(state, Viewer::of(role, Some(user)), seq, chat)
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
                | Event::GameTimeAdvanced { .. }
                | Event::CampaignMetaUpdated { .. }
                | Event::UserJoined { .. }
                | Event::CombatUpdated { .. }
                | Event::RollRequested { .. }
                | Event::RollRequestUpdated { .. }
                | Event::Redacted
        )
    })
}

/// Filters a committed `batch` for players; `before` and `after` are the full states around it.
#[must_use]
pub fn filter_for_players(before: &GameState, after: &GameState, batch: &EventBatch) -> ForPlayers {
    // Inspector rows are per recipient and do not travel in a batch (ADR-0061): a new visible
    // token, a changed policy or a changed override needs a fresh snapshot.
    if batch.events.iter().any(|e| {
        matches!(
            e,
            Event::CampaignMetaUpdated { .. }
                | Event::TokenCreated { .. }
                | Event::TokenUpdated { .. }
                | Event::TokenEffectApplied { .. }
                | Event::TokenEffectUpdated { .. }
                | Event::TokenEffectRemoved { .. }
        )
    }) && inspector_rows_changed(before, after, batch)
    {
        return ForPlayers::Resnapshot;
    }
    if batch.events.iter().any(|e| {
        matches!(
            e,
            Event::PluginRuntimeUpdated { .. }
                | Event::RulesViewUpdated { .. }
                | Event::ItemCreated { .. }
                | Event::ItemUpdated { .. }
                | Event::ItemDeleted { .. }
                | Event::SystemPluginSet { .. }
        )
    }) || batch.events.iter().any(|e| match e {
        // Replacing or removing the system changes which sheet fields are public (ADR-0049).
        Event::PluginInstalled { plugin } => {
            after.system_plugin.as_deref() == Some(plugin.id.as_str())
        }
        Event::PluginUninstalled { id } => before.system_plugin.as_deref() == Some(id.as_str()),
        _ => false,
    }) {
        return ForPlayers::Resnapshot;
    }
    if (!before.plugin_runtime.saves.is_empty()
        || !before.plugin_runtime.movements.is_empty()
        || !before.plugin_runtime.maintained.is_empty()
        || !before.plugin_runtime.action_stacks.is_empty())
        && batch.events.iter().any(|e| {
            matches!(
                e,
                Event::CharacterUpdated { .. } | Event::TokenUpdated { .. }
            )
        })
    {
        return ForPlayers::Resnapshot;
    }
    if before.roll_requests.values().any(|r| {
        before.characters.get(&r.character).and_then(|c| c.owner)
            != after.characters.get(&r.character).and_then(|c| c.owner)
    }) {
        return ForPlayers::Resnapshot;
    }
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
    }) || flipped(&was.characters, &now.characters, |id| {
        before.characters.contains_key(id) && after.characters.contains_key(id)
    }) {
        return ForPlayers::Resnapshot;
    }
    // The full sheet is recipient-specific (ADR-0049), so one shared batch cannot express an
    // ownership change.
    if before.characters.iter().any(|(id, character)| {
        after
            .characters
            .get(id)
            .is_some_and(|next| next.owner != character.owner)
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
                        Event::TokenEffectApplied { effect }
                        | Event::TokenEffectUpdated { effect } => shown(
                            after.effects.contains_key(&effect.id),
                            now.effects.contains(&effect.id),
                            was.effects.contains(&effect.id),
                        ),
                        Event::TokenEffectRemoved { id, .. } => was.effects.contains(id),
                        // The connection adds back only this recipient's private requests.
                        Event::RulesViewUpdated { .. } | Event::ItemCreated { .. } | Event::ItemUpdated { .. } | Event::ItemDeleted { .. } | Event::PluginRuntimeUpdated { .. } | Event::RollRequested { .. } | Event::RollRequestUpdated { .. } => false,
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
                        Event::PlaylistCreated { playlist } | Event::PlaylistUpdated { playlist } => now.playlists.contains(&playlist.id),
                        Event::PlaylistDeleted { id } => was.playlists.contains(id),
                        Event::ChatMessagePosted { message } => chat_visible_to_players(message),
                        Event::CharacterCreated { character }
                        | Event::CharacterUpdated { character } => shown(
                            after.characters.contains_key(&character.id),
                            now.characters.contains(&character.id),
                            was.characters.contains(&character.id),
                        ),
                        // Deletion carries only an id, and an owner must learn that their hidden
                        // draft is gone; sending it to everyone reveals no character data.
                        Event::CharacterDeleted { .. } => true,
                        Event::ImagePresentationChanged { .. }
                        | Event::GameTimeAdvanced { .. }
                        | Event::CampaignMetaUpdated { .. }
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
                // Keep incremental map events consistent with the player snapshot projection.
                match event {
                    Event::PlaylistCreated { playlist } => { let mut playlist = playlist.clone(); playlist.folder_id = None; Event::PlaylistCreated { playlist } }
                    Event::PlaylistUpdated { playlist } => { let mut playlist = playlist.clone(); playlist.folder_id = None; Event::PlaylistUpdated { playlist } }
                    Event::MapCreated { map } => {
                        let mut map = map.clone();
                        map.visible_to_players = true;
                        Event::MapCreated { map }
                    }
                    Event::MapUpdated { map } => {
                        let mut map = map.clone();
                        map.visible_to_players = true;
                        Event::MapUpdated { map }
                    }
                    Event::TokenCreated { token } => Event::TokenCreated {
                        token: public_token(after, token),
                    },
                    Event::TokenUpdated { token } => Event::TokenUpdated {
                        token: public_token(after, token),
                    },
                    Event::CharacterCreated { character } => Event::CharacterCreated {
                        character: public_character(after, character),
                    },
                    Event::CharacterUpdated { character } => Event::CharacterUpdated {
                        character: public_character(after, character),
                    },
                    _ => event.clone(),
                }
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

/// Whether the batch changes which inspector fields players see (ADR-0061).
fn inspector_rows_changed(before: &GameState, after: &GameState, batch: &EventBatch) -> bool {
    let see = PlayersSee::of(after);
    if batch.events.iter().any(|e| match e {
        // A token that becomes visible arrives without its rows.
        Event::TokenCreated { token } => see.tokens.contains(&token.id),
        _ => false,
    }) {
        return true;
    }
    if before.campaign.token_inspector != after.campaign.token_inspector {
        return true;
    }
    let viewer = Viewer::of(Role::Player, None);
    before
        .tokens
        .iter()
        // Hidden tokens have no rows, so their overrides do not matter.
        .filter(|(id, _)| see.tokens.contains(*id) && after.tokens.contains_key(id))
        .any(|(id, token)| {
            after.tokens.get(id).is_some_and(|next| {
                token_fields(before, token, viewer) != token_fields(after, next, viewer)
            })
        })
}

/// [`filter_for_players`] for events that cannot change visibility: decided on the spot.
fn cheap_event(after: &GameState, event: &Event) -> Event {
    let token_shown = |id: &TokenId| {
        after.tokens.get(id).is_some_and(|t| {
            t.visible && t.layer != TokenLayer::Gm && map_visible_to_players(after, &t.map_id)
        })
    };
    let map_shown = |id: &MapId| map_visible_to_players(after, id);
    let visible = match event {
        Event::PluginRuntimeUpdated { .. }
        | Event::RollRequested { .. }
        | Event::RollRequestUpdated { .. } => false,
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
