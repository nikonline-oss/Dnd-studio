//! What players see (ADR-0022): the view, filtered batches and the invariant tying them together.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{
    ForPlayers, GameState, SplitMix64, apply, evolve_all, filter_for_players, join, players_state,
    snapshot_for,
};
use dnd_protocol::arbitrary::{self, pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;
use proptest::prelude::*;
use proptest::sample::select;

fn entry(
    n: u128,
    folder: Option<JournalFolderId>,
    visibility: JournalVisibility,
) -> JournalEntryDto {
    JournalEntryDto {
        id: JournalEntryId(v7(0x7000 + n)),
        folder_id: folder,
        title: format!("Entry {n}"),
        markdown: "text".into(),
        sort_order: 0,
        visibility,
        players_can_edit: false,
        author: None,
        version: Version::INITIAL,
    }
}

fn folder(n: u128, parent: Option<JournalFolderId>) -> JournalFolderDto {
    JournalFolderDto {
        id: JournalFolderId(v7(0x6000 + n)),
        parent,
        name: format!("Folder {n}"),
        sort_order: 0,
        version: Version::INITIAL,
    }
}

fn message(n: u128, visibility: ChatVisibility) -> ChatMessageDto {
    ChatMessageDto {
        id: ChatMessageId(v7(0x8000 + n)),
        kind: ChatKind::Text,
        sender: Some(GM),
        sender_name: "GM".into(),
        body: format!("message {n}"),
        roll: None,
        system: None,
        visibility,
        at_ms: None,
    }
}

/// `table_state` plus a journal: Lore > Places > shared entry; Secrets > GM entry; a hidden token.
fn world() -> GameState {
    let mut state = table_state();
    let lore = folder(0, None);
    let places = folder(1, Some(lore.id));
    let secrets = folder(2, None);
    for f in [lore, places.clone(), secrets.clone()] {
        state.journal_folders.insert(f.id, f);
    }
    for e in [
        entry(0, Some(places.id), JournalVisibility::Players),
        entry(1, Some(secrets.id), JournalVisibility::Gm),
    ] {
        state.journal.insert(e.id, e);
    }
    let mut invisible = token(5, 0, None, TokenLayer::Tokens);
    invisible.visible = false;
    state.tokens.insert(invisible.id, invisible);
    state
}

fn batch(events: Vec<Event>) -> EventBatch {
    EventBatch {
        seq: Seq(10),
        caused_by: None,
        events,
    }
}

fn filter(before: &GameState, events: Vec<Event>) -> ForPlayers {
    let mut after = before.clone();
    evolve_all(&mut after, &events);
    filter_for_players(before, &after, &batch(events))
}

#[test]
fn players_see_shared_things_only() {
    let view = players_state(&world());
    assert_eq!(
        view.maps.keys().copied().collect::<Vec<_>>(),
        [pooled_map_id(0)]
    );
    // Hidden map's token 4, GM layer token 2 and invisible token 5 are gone.
    assert_eq!(
        view.tokens.keys().copied().collect::<Vec<_>>(),
        [0, 1, 3].map(pooled_token_id)
    );
    assert_eq!(view.journal.len(), 1);
    // The folders on the shared entry's way to the root, not the GM's "Secrets".
    let mut folders: Vec<_> = view
        .journal_folders
        .values()
        .map(|f| f.name.clone())
        .collect();
    folders.sort();
    assert_eq!(folders, ["Folder 0", "Folder 1"]);
    assert_eq!(view.characters.len(), world().characters.len());
}

#[test]
fn snapshots_drop_hidden_chat_for_players_only() {
    let chat = [
        message(0, ChatVisibility::All),
        message(1, ChatVisibility::Gm),
    ];
    let player = snapshot_for(Role::Player, &world(), Seq(3), &chat);
    let gm = snapshot_for(Role::Gm, &world(), Seq(3), &chat);
    assert_eq!(player.chat.len(), 1);
    assert_eq!(gm.chat.len(), 2);
    assert_eq!(player.seq, Seq(3));
    assert_eq!(gm.tokens.len(), world().tokens.len());
    assert_eq!(snapshot_for(Role::CoGm, &world(), Seq(3), &chat), gm);
}

#[test]
fn hidden_events_become_placeholders_with_the_same_numbers() {
    let state = world();
    let events = vec![
        Event::TokenMoved {
            id: pooled_token_id(2), // GM layer
            x: 16,
            y: 16,
        },
        Event::TokenMoved {
            id: pooled_token_id(0),
            x: 32,
            y: 32,
        },
        Event::ChatMessagePosted {
            message: message(2, ChatVisibility::Gm),
        },
    ];
    let ForPlayers::Batch(filtered) = filter(&state, events.clone()) else {
        panic!("expected a batch");
    };
    assert_eq!(filtered.seq, Seq(10));
    assert_eq!(
        filtered.events,
        [Event::Redacted, events[1].clone(), Event::Redacted]
    );
}

#[test]
fn revealing_or_hiding_sends_a_snapshot() {
    let state = world();
    let mut shown = state.maps[&pooled_map_id(1)].clone();
    shown.visible_to_players = true;
    assert_eq!(
        filter(&state, vec![Event::MapUpdated { map: shown }]),
        ForPlayers::Resnapshot
    );

    let mut to_gm = state.tokens[&pooled_token_id(0)].clone();
    to_gm.layer = TokenLayer::Gm;
    assert_eq!(
        filter(&state, vec![Event::TokenUpdated { token: to_gm }]),
        ForPlayers::Resnapshot
    );

    let mut shared = entry(1, Some(folder(2, None).id), JournalVisibility::Players);
    shared.version = Version(2);
    assert_eq!(
        filter(&state, vec![Event::JournalEntryUpdated { entry: shared }]),
        ForPlayers::Resnapshot
    );
}

#[test]
fn renaming_a_visible_map_and_creating_hidden_things_stay_events() {
    let state = world();
    let mut renamed = state.maps[&pooled_map_id(0)].clone();
    renamed.name = "Renamed".into();
    let hidden_token = token(5, 1, None, TokenLayer::Tokens); // on the hidden map
    let gm_entry = entry(9, None, JournalVisibility::Gm);
    let ForPlayers::Batch(filtered) = filter(
        &state,
        vec![
            Event::MapUpdated {
                map: renamed.clone(),
            },
            Event::TokenCreated {
                token: hidden_token,
            },
            Event::JournalEntryCreated { entry: gm_entry },
            Event::JournalFolderCreated {
                folder: folder(9, None), // empty: nothing shared inside
            },
        ],
    ) else {
        panic!("expected a batch");
    };
    assert_eq!(
        filtered.events,
        [
            Event::MapUpdated { map: renamed },
            Event::Redacted,
            Event::Redacted,
            Event::Redacted
        ]
    );
}

#[test]
fn deleting_a_hidden_token_that_fights_reveals_its_combatant() {
    // A combatant with a hidden token is hidden; once the token is gone it fights without one and
    // players see it — a visibility change of an existing combatant.
    let mut state = world();
    let combat = CombatDto {
        id: CombatId(v7(0x9000)),
        map_id: pooled_map_id(0),
        round: 1,
        turn: None,
        version: Version::INITIAL,
    };
    let combatant = CombatantDto {
        id: CombatantId(v7(0x9100)),
        combat_id: combat.id,
        token_id: Some(pooled_token_id(2)),
        character_id: None,
        name: "Lurker".into(),
        initiative: None,
        sort_order: 0,
        version: Version::INITIAL,
    };
    state.combats.insert(combat.id, combat);
    state.combatants.insert(combatant.id, combatant);
    assert_eq!(
        filter(
            &state,
            vec![Event::TokenDeleted {
                id: pooled_token_id(2)
            }]
        ),
        ForPlayers::Resnapshot
    );
}

#[test]
fn joining_adds_a_user_once() {
    let state = base_state();
    let id = UserId(v7(0xC1));
    let events = join(&state, id, "  Mira ", Role::Player).unwrap();
    let Some(Event::UserJoined { user }) = events.first() else {
        panic!("expected UserJoined");
    };
    assert_eq!(user.name, "Mira");

    let mut joined = state.clone();
    evolve_all(&mut joined, &events);
    assert!(join(&joined, id, "Mira", Role::Player).unwrap().is_empty());
    assert_eq!(
        join(&joined, id, "Mira the Bold", Role::Player)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        join(&joined, id, "   ", Role::Player),
        Err(RejectReason::invalid("name"))
    );
}

/// Version of the target of a checked intent, so random steps are mostly accepted.
fn current_version(state: &GameState, intent: &Intent) -> Option<Version> {
    match intent {
        Intent::UpdateCampaignMeta { .. } => Some(state.campaign.version),
        Intent::UpdateMap { id, .. }
        | Intent::DeleteMap { id }
        | Intent::ResetFog { map_id: id, .. } => state.maps.get(id).map(|m| m.version),
        Intent::UpdateToken { id, .. }
        | Intent::DeleteToken { id }
        | Intent::MoveToken { id, .. } => state.tokens.get(id).map(|t| t.version),
        Intent::UpdateJournalEntry { id, .. }
        | Intent::MoveJournalEntry { id, .. }
        | Intent::DeleteJournalEntry { id } => state.journal.get(id).map(|e| e.version),
        Intent::RenameJournalFolder { id, .. }
        | Intent::MoveJournalFolder { id, .. }
        | Intent::DeleteJournalFolder { id } => state.journal_folders.get(id).map(|f| f.version),
        other => other
            .character_id()
            .and_then(|id| state.characters.get(&id).map(|c| c.version)),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Players who apply their filtered batches to their view end up with exactly the view of the
    /// new state — or are told to take a snapshot.
    #[test]
    fn filtered_batches_keep_the_players_view_exact(
        steps in proptest::collection::vec(
            (select(vec![Role::Gm, Role::Gm, Role::Gm, Role::Player]), arbitrary::intent_envelope()),
            1..60,
        ),
    ) {
        let mut state = world();
        let mut players = players_state(&state);
        let mut rng = SplitMix64::new(3);
        for (role, mut envelope) in steps {
            envelope.expected_version = current_version(&state, &envelope.intent);
            let Ok(events) = apply(&state, &envelope, &actor(role), &mut rng) else {
                continue;
            };
            let before = state.clone();
            evolve_all(&mut state, &events);
            let batch = EventBatch { seq: Seq(1), caused_by: None, events };
            match filter_for_players(&before, &state, &batch) {
                ForPlayers::Batch(filtered) => {
                    prop_assert_eq!(filtered.events.len(), batch.events.len());
                    evolve_all(&mut players, &filtered.events);
                }
                ForPlayers::Resnapshot => players = players_state(&state),
            }
            prop_assert_eq!(&players, &players_state(&state));
        }
    }
}
