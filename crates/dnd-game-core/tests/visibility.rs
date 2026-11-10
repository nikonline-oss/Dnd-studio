//! What players see (ADR-0022): the view, filtered batches and the invariant tying them together.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{
    ForPlayers, GameState, SplitMix64, Viewer, apply, evolve_all, filter_for_players, join,
    personal_snapshot, players_state, snapshot_for, snapshot_for_user,
};
use dnd_protocol::arbitrary::{self, pooled_character_id, pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;
use proptest::prelude::*;
use proptest::sample::select;
use serde_json::json;

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
        table: None,
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
        related_characters: Vec::new(),
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
    // Without a system schema nothing is public (ADR-0049).
    assert!(view.characters.values().all(|c| c.sheet_json == "{}"));
}

/// `world()` with a test system: `name` is public, `notes` and `hp` are private.
fn with_public_schema(mut state: GameState) -> GameState {
    let rules = SystemRulesDto {
        mechanics: PluginMechanics::default(),
        features: Vec::new(),
        sheet_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "visible_to_players": true },
                "notes": { "type": "string" },
                "hp": { "type": "integer" }
            }
        }),
        sheet_layout: json!({ "type": "section", "children": [] }),
        creature_sheet_layout: None,
        computed: std::collections::BTreeMap::new(),
        initiative: "1d20".into(),
        resources: Vec::new(),
        health_thresholds: vec![75, 25, 0],
        rolls: Vec::new(),
        conditions: Vec::new(),
        inventory: None,
    };
    let plugin = PluginDto {
        feature_settings: std::collections::BTreeMap::new(),
        id: "test-system".into(),
        version: "0.1.0".into(),
        kind: PluginKind::System,
        name: Label::Text("Test".into()),
        license: "MIT".into(),
        attribution: None,
        archive_sha256: "0".repeat(64),
        system: Some(rules),
        theme: None,
        trigger_templates: Vec::new(),
    };
    state.system_plugin = Some(plugin.id.clone());
    state.plugins.insert(plugin.id.clone(), plugin);
    for character in state.characters.values_mut() {
        let name = character.name.clone();
        character.sheet_json = json!({
            "name": name,
            "notes": "secret",
            "hp": 7
        })
        .to_string();
    }
    state
}

fn sheet(snapshot: &Snapshot, id: CharacterId) -> serde_json::Value {
    let character = snapshot
        .characters
        .iter()
        .find(|c| c.id == id)
        .expect("the character is in the snapshot");
    serde_json::from_str(&character.sheet_json).expect("the sheet is JSON")
}

fn character(
    id: CharacterId,
    kind: CharacterKind,
    owner: Option<UserId>,
    status: CharacterStatus,
) -> CharacterDto {
    CharacterDto {
        id,
        kind,
        name: format!("Character {}", id.0),
        owner,
        status,
        sheet_json: json!({ "name": "seen", "notes": "secret", "hp": 7 }).to_string(),
        conditions: Vec::new(),
        version: Version::INITIAL,
    }
}

#[test]
fn owners_and_gm_get_full_sheets_others_only_public_fields() {
    let state = with_public_schema(world());
    let hero = pooled_character_id(0);
    let mine = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    let other = snapshot_for_user(Role::Player, OTHER_PLAYER, &state, Seq(1), []);
    let spectator = snapshot_for_user(Role::Spectator, SPECTATOR, &state, Seq(1), []);
    let gm = snapshot_for_user(Role::Gm, GM, &state, Seq(1), []);

    let full = json!({ "name": "Hero 0", "notes": "secret", "hp": 7 });
    let public = json!({ "name": "Hero 0" });
    assert_eq!(sheet(&mine, hero), full);
    assert_eq!(sheet(&other, hero), public);
    assert_eq!(sheet(&spectator, hero), public);
    assert_eq!(sheet(&gm, hero), full);
}

#[test]
fn hidden_npcs_and_unapproved_pcs_stay_hidden_until_linked() {
    let mut state = with_public_schema(world());
    let npc = pooled_character_id(5);
    let draft = pooled_character_id(6);
    state.characters.insert(
        npc,
        character(npc, CharacterKind::Npc, None, CharacterStatus::Active),
    );
    state.characters.insert(
        draft,
        character(
            draft,
            CharacterKind::Pc,
            Some(PLAYER),
            CharacterStatus::Draft,
        ),
    );

    let other = snapshot_for_user(Role::Player, OTHER_PLAYER, &state, Seq(1), []);
    assert!(!other.characters.iter().any(|c| c.id == npc));
    assert!(!other.characters.iter().any(|c| c.id == draft));
    assert!(
        !other
            .rules_view
            .characters
            .iter()
            .any(|c| c.character == npc)
    );

    let owner = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    assert_eq!(
        sheet(&owner, draft),
        json!({ "name": "seen", "notes": "secret", "hp": 7 })
    );
    assert!(!owner.characters.iter().any(|c| c.id == npc));

    let gm = snapshot_for_user(Role::Gm, GM, &state, Seq(1), []);
    assert!(gm.characters.iter().any(|c| c.id == npc));
    assert!(gm.characters.iter().any(|c| c.id == draft));
    assert!(gm.rules_view.characters.iter().any(|c| c.character == npc));

    // A visible token reveals its NPC, and the reference stays usable.
    let visible = token(6, 0, Some(5), TokenLayer::Tokens);
    state.tokens.insert(visible.id, visible);
    let other = snapshot_for_user(Role::Player, OTHER_PLAYER, &state, Seq(1), []);
    assert_eq!(sheet(&other, npc), json!({ "name": "seen" }));
    assert!(other.tokens.iter().any(|t| t.character_id == Some(npc)));
    assert!(
        other
            .rules_view
            .characters
            .iter()
            .any(|c| c.character == npc)
    );
}

#[test]
fn public_character_events_carry_only_public_fields() {
    let state = with_public_schema(world());
    let mut updated = state.characters[&pooled_character_id(0)].clone();
    updated.sheet_json = json!({ "name": "Mira", "notes": "secret", "hp": 9 }).to_string();
    updated.version = Version(2);
    let ForPlayers::Batch(filtered) =
        filter(&state, vec![Event::CharacterUpdated { character: updated }])
    else {
        panic!("expected a batch");
    };
    let Event::CharacterUpdated { character } = &filtered.events[0] else {
        panic!("expected the character event");
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&character.sheet_json).expect("JSON"),
        json!({ "name": "Mira" })
    );
}

#[test]
fn a_new_owner_gets_the_full_sheet_and_the_previous_one_only_public_fields() {
    let mut state = with_public_schema(world());
    let hero = pooled_character_id(0);
    state
        .characters
        .get_mut(&hero)
        .expect("the hero exists")
        .owner = Some(OTHER_PLAYER);

    let previous = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    let next = snapshot_for_user(Role::Player, OTHER_PLAYER, &state, Seq(1), []);
    assert_eq!(sheet(&previous, hero), json!({ "name": "Hero 0" }));
    assert_eq!(
        sheet(&next, hero),
        json!({ "name": "Hero 0", "notes": "secret", "hp": 7 })
    );
}

/// ADR-0054: a viewer is one value, and the two public entry points are shorthands for it.
#[test]
fn a_viewer_states_who_receives_the_view() {
    let state = with_public_schema(world());
    let hero = pooled_character_id(0);
    // The role-level view knows no person: the owner gets the same public sheet as everyone.
    let role_level = snapshot_for(Role::Player, &state, Seq(1), []);
    assert_eq!(sheet(&role_level, hero), json!({ "name": "Hero 0" }));
    assert!(role_level.plugin_runtime.saves.is_empty());
    // The person-level view adds owner-specific data, and equals the named shorthand.
    let mine = personal_snapshot(&state, Viewer::player(PLAYER), Seq(1), []);
    assert_eq!(
        mine,
        snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), [])
    );
    assert_eq!(
        sheet(&mine, hero),
        json!({ "name": "Hero 0", "notes": "secret", "hp": 7 })
    );
    // A GM role sees everything whichever constructor is used.
    assert_eq!(
        personal_snapshot(&state, Viewer::gm(), Seq(1), []),
        snapshot_for_user(Role::Gm, GM, &state, Seq(1), [])
    );
    assert_eq!(
        personal_snapshot(&state, Viewer::co_gm(), Seq(1), []),
        personal_snapshot(&state, Viewer::gm(), Seq(1), [])
    );
}

#[test]
fn a_viewer_owns_only_its_own_character() {
    let state = with_public_schema(world());
    let hero = state.characters[&pooled_character_id(0)].clone();
    assert!(Viewer::player(PLAYER).owns(&hero));
    assert!(!Viewer::player(OTHER_PLAYER).owns(&hero));
    assert!(!Viewer::spectator(PLAYER).owns(&hero));
    assert!(Viewer::gm().owns(&hero));
}

#[test]
fn action_availability_is_private_to_the_owner() {
    let state = with_public_schema(world());
    let mine = snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), []);
    let other = snapshot_for_user(Role::Player, OTHER_PLAYER, &state, Seq(1), []);
    let spectator = snapshot_for_user(Role::Spectator, SPECTATOR, &state, Seq(1), []);
    let gm = snapshot_for_user(Role::Gm, GM, &state, Seq(1), []);

    // Token 0 belongs to PLAYER, token 1 to OTHER_PLAYER, both on the visible map.
    let tokens = |s: &Snapshot| {
        s.rules_view
            .tokens
            .iter()
            .map(|t| t.token)
            .collect::<Vec<_>>()
    };
    assert_eq!(tokens(&mine), [pooled_token_id(0)]);
    assert_eq!(tokens(&other), [pooled_token_id(1)]);
    assert!(spectator.rules_view.tokens.is_empty());
    assert!(gm.rules_view.tokens.len() >= 2);
}

#[test]
fn handing_a_character_to_another_player_requires_a_snapshot() {
    let state = with_public_schema(world());
    let mut updated = state.characters[&pooled_character_id(0)].clone();
    updated.owner = Some(OTHER_PLAYER);
    updated.version = Version(2);
    assert_eq!(
        filter(&state, vec![Event::CharacterUpdated { character: updated }]),
        ForPlayers::Resnapshot
    );
}

#[test]
fn deleting_a_hidden_character_is_delivered_without_its_data() {
    let mut state = world();
    let npc = pooled_character_id(5);
    state.characters.insert(
        npc,
        character(npc, CharacterKind::Npc, None, CharacterStatus::Active),
    );
    let ForPlayers::Batch(filtered) = filter(&state, vec![Event::CharacterDeleted { id: npc }])
    else {
        panic!("expected a batch");
    };
    assert_eq!(filtered.events, vec![Event::CharacterDeleted { id: npc }]);
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
        Intent::UpdateBoard { id, .. } | Intent::DeleteBoard { id } => {
            state.boards.get(id).map(|b| b.version)
        }
        Intent::UpdateBoardNode { id, .. } | Intent::DeleteBoardNode { id } => {
            state.board_nodes.get(id).map(|n| n.version)
        }
        Intent::UpdateBoardEdge { id, .. } | Intent::DeleteBoardEdge { id } => {
            state.board_edges.get(id).map(|e| e.version)
        }
        Intent::UpdateZone { id, .. }
        | Intent::DeleteZone { id }
        | Intent::MergeZones { into: id, .. } => state.zones.get(id).map(|z| z.version),
        Intent::UpdateTrigger { id, .. } | Intent::DeleteTrigger { id } => {
            state.triggers.get(id).map(|t| t.version)
        }
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
