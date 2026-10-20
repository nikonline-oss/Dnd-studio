//! Example-based tests for versions (ADR-0003), cascades and validation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_protocol::arbitrary::{pooled_character_id, pooled_map_id, pooled_token_id};
use dnd_protocol::*;

fn rename_map(name: &str) -> Intent {
    Intent::UpdateMap {
        id: pooled_map_id(0),
        name: Some(name.into()),
        grid: None,
        visible_to_players: None,
    }
}

#[test]
fn update_bumps_version_by_one() {
    let mut state = table_state();
    run(&mut state, Role::Gm, rename_map("Crypt"), Some(Version(1))).unwrap();
    let map = &state.maps[&pooled_map_id(0)];
    assert_eq!(map.name, "Crypt");
    assert_eq!(map.version, Version(2));
}

#[test]
fn stale_version_is_rejected_with_current() {
    let mut state = table_state();
    run(&mut state, Role::Gm, rename_map("First"), Some(Version(1))).unwrap();
    // Co-GM edited based on version 1, which is now stale.
    let result = run(
        &mut state,
        Role::CoGm,
        rename_map("Second"),
        Some(Version(1)),
    );
    assert_eq!(
        result,
        Err(RejectReason::StaleVersion {
            current: Version(2)
        })
    );
    assert_eq!(state.maps[&pooled_map_id(0)].name, "First");
}

#[test]
fn checked_intent_requires_expected_version() {
    let mut state = table_state();
    let result = run(&mut state, Role::Gm, rename_map("X"), None);
    assert_eq!(result, Err(RejectReason::MissingExpectedVersion));
}

#[test]
fn gm_has_no_implicit_priority_over_versions() {
    // ADR-0003: priority is only explicit; a stale GM edit is rejected like anyone else's.
    let mut state = table_state();
    run(
        &mut state,
        Role::CoGm,
        rename_map("Co-GM"),
        Some(Version(1)),
    )
    .unwrap();
    let result = run(&mut state, Role::Gm, rename_map("GM"), Some(Version(1)));
    assert!(matches!(result, Err(RejectReason::StaleVersion { .. })));
}

#[test]
fn move_token_is_last_write_wins_and_keeps_version() {
    let mut state = table_state();
    let id = pooled_token_id(3);
    for (x, expected) in [(10, None), (20, Some(Version(42)))] {
        run(
            &mut state,
            Role::Gm,
            Intent::MoveToken { id, x, y: 0 },
            expected,
        )
        .unwrap();
    }
    let token = &state.tokens[&id];
    assert_eq!(token.x, 20);
    assert_eq!(token.version, Version::INITIAL);

    // A concurrent property edit based on the pre-move version still applies.
    run(
        &mut state,
        Role::CoGm,
        Intent::UpdateToken {
            id,
            name: None,
            rotation: None,
            scale: None,
            layer: None,
            visible: Some(false),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert!(!state.tokens[&id].visible);
}

#[test]
fn delete_map_cascades_to_tokens() {
    let mut state = table_state();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteMap {
            id: pooled_map_id(0),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(events.len(), 5, "4 tokens + the map");
    assert!(matches!(events.last(), Some(Event::MapDeleted { .. })));
    assert!(state.tokens.values().all(|t| t.map_id != pooled_map_id(0)));
    assert_eq!(state.tokens.len(), 1);
}

#[test]
fn create_rejects_duplicates_and_dangling_references() {
    let mut state = table_state();
    let create = |id, map_n, character: Option<u128>| Intent::CreateToken {
        id: pooled_token_id(id),
        map_id: pooled_map_id(map_n),
        character_id: character.map(pooled_character_id),
        asset_id: None,
        name: "T".into(),
        x: 0,
        y: 0,
        rotation: 0.0,
        scale: 1.0,
        layer: TokenLayer::Tokens,
        visible: true,
    };
    assert_eq!(
        run(&mut state, Role::Gm, create(0, 0, None), None),
        Err(RejectReason::AlreadyExists)
    );
    assert_eq!(
        run(&mut state, Role::Gm, create(5, 2, None), None),
        Err(RejectReason::NotFound)
    );
    assert_eq!(
        run(&mut state, Role::Gm, create(5, 0, Some(99)), None),
        Err(RejectReason::NotFound)
    );
    run(&mut state, Role::Gm, create(5, 0, Some(0)), None).unwrap();
}

#[test]
fn validation() {
    let mut state = table_state();
    let cases = [
        (rename_map("   "), "name"),
        (rename_map(&"x".repeat(201)), "name"),
        (
            Intent::UpdateMap {
                id: pooled_map_id(0),
                name: None,
                grid: Some(Grid {
                    cell_size: 0.0,
                    ..Grid::default()
                }),
                visible_to_players: None,
            },
            "grid.cell_size",
        ),
        (
            Intent::UpdateMap {
                id: pooled_map_id(0),
                name: None,
                grid: None,
                visible_to_players: None,
            },
            "intent",
        ),
        (
            Intent::MoveToken {
                id: pooled_token_id(0),
                x: -1,
                y: 0,
            },
            "x",
        ),
        (
            Intent::MoveToken {
                id: pooled_token_id(0),
                x: 0,
                y: 3000 * 16 + 1,
            },
            "y",
        ),
        (
            Intent::UpdateToken {
                id: pooled_token_id(0),
                name: None,
                rotation: None,
                scale: Some(-1.0),
                layer: None,
                visible: None,
            },
            "scale",
        ),
    ];
    for (intent, field) in cases {
        let result = run(&mut state, Role::Gm, intent.clone(), Some(Version(1)));
        assert_eq!(result, Err(RejectReason::invalid(field)), "{intent:?}");
    }
    assert_eq!(state, table_state());
}

#[test]
fn rotation_is_normalized_and_names_trimmed() {
    let mut state = table_state();
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateToken {
            id: pooled_token_id(0),
            name: Some("  Aria  ".into()),
            rotation: Some(-90.0),
            scale: None,
            layer: None,
            visible: None,
        },
        Some(Version(1)),
    )
    .unwrap();
    let token = &state.tokens[&pooled_token_id(0)];
    assert_eq!(token.rotation, 270.0);
    assert_eq!(token.name, "Aria");
}

#[test]
fn snapshot_roundtrip() {
    let state = table_state();
    let restored = dnd_game_core::GameState::from_snapshot(state.to_snapshot(Seq(7)));
    assert_eq!(restored, state);
}

fn move_token(n: u128) -> Intent {
    Intent::MoveToken {
        id: pooled_token_id(n),
        x: 140,
        y: 140,
    }
}

/// A move that arrives after the token was deleted (e.g. a late 10–15 Hz drag update) must not
/// resurrect it: the core rejects it, and even a stray `TokenMoved` event is a no-op.
#[test]
fn stale_move_after_delete_does_not_resurrect() {
    for role in [Role::Gm, Role::CoGm] {
        let mut state = table_state();
        run(
            &mut state,
            Role::Gm,
            Intent::DeleteToken {
                id: pooled_token_id(3),
            },
            Some(Version(1)),
        )
        .unwrap();
        let after_delete = state.clone();

        assert_eq!(
            run(&mut state, role, move_token(3), None),
            Err(RejectReason::NotFound)
        );
        assert_eq!(state, after_delete);
    }

    let mut state = table_state();
    dnd_game_core::evolve_all(
        &mut state,
        &[
            Event::TokenDeleted {
                id: pooled_token_id(3),
            },
            Event::TokenMoved {
                id: pooled_token_id(3),
                x: 1,
                y: 1,
            },
        ],
    );
    assert!(!state.tokens.contains_key(&pooled_token_id(3)));
}

/// Same for a token removed together with its map.
#[test]
fn stale_move_after_map_delete_is_rejected() {
    let mut state = table_state();
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteMap {
            id: pooled_map_id(0),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        run(&mut state, Role::Gm, move_token(0), None),
        Err(RejectReason::NotFound)
    );
    // Players get Forbidden, not NotFound: they must not learn which ids exist.
    assert_eq!(
        run(&mut state, Role::Player, move_token(0), None),
        Err(RejectReason::Forbidden)
    );
}

/// Ownership is checked against the state at apply time. A player's move that arrives after the
/// character changed owner (or the token was unlinked) is rejected.
#[test]
fn stale_player_move_after_owner_change_is_rejected() {
    let mut state = table_state();
    assert!(run(&mut state, Role::Player, move_token(0), None).is_ok());

    // Character 0 is handed to another player (character intents arrive in stage 2a).
    state
        .characters
        .get_mut(&pooled_character_id(0))
        .unwrap()
        .owner = Some(OTHER_PLAYER);
    let before = state.clone();
    assert_eq!(
        run(&mut state, Role::Player, move_token(0), None),
        Err(RejectReason::Forbidden)
    );
    assert_eq!(state, before);

    // Token unlinked from the character by the GM.
    let mut state = table_state();
    state
        .tokens
        .get_mut(&pooled_token_id(0))
        .unwrap()
        .character_id = None;
    assert_eq!(
        run(&mut state, Role::Player, move_token(0), None),
        Err(RejectReason::Forbidden)
    );

    // Map hidden from players after the drag started.
    let mut state = table_state();
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateMap {
            id: pooled_map_id(0),
            name: None,
            grid: None,
            visible_to_players: Some(false),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        run(&mut state, Role::Player, move_token(0), None),
        Err(RejectReason::Forbidden)
    );
}

/// ADR-0015: ids are unique across entity kinds; a new map cannot take a token's or a
/// character's id, and vice versa.
#[test]
fn created_ids_must_not_collide_with_any_entity_kind() {
    let mut state = table_state();
    let create_map = |id: dnd_protocol::Uuid| Intent::CreateMap {
        id: MapId(id),
        name: "M".into(),
        background_asset_id: None,
        width: 4000,
        height: 3000,
        grid: Grid::default(),
        visible_to_players: false,
    };
    for taken in [
        pooled_token_id(0).0,
        pooled_character_id(0).0,
        pooled_map_id(0).0,
    ] {
        assert_eq!(
            run(&mut state, Role::Gm, create_map(taken), None),
            Err(RejectReason::AlreadyExists)
        );
    }
    let token_with_map_id = Intent::CreateToken {
        id: TokenId(pooled_map_id(1).0),
        map_id: pooled_map_id(0),
        character_id: None,
        asset_id: None,
        name: "T".into(),
        x: 0,
        y: 0,
        rotation: 0.0,
        scale: 1.0,
        layer: TokenLayer::Tokens,
        visible: true,
    };
    assert_eq!(
        run(&mut state, Role::Gm, token_with_map_id, None),
        Err(RejectReason::AlreadyExists)
    );
    assert_eq!(state, table_state());
}

/// Malformed envelopes are rejected before permissions, for every role.
#[test]
fn malformed_envelopes_are_rejected_first() {
    let state = table_state();
    for role in Role::ALL {
        let mut env = envelope(rename_map("X"), Some(Version(1)));
        env.intent_id = IntentId(dnd_protocol::Uuid::nil());
        let result = dnd_game_core::apply(
            &state,
            &env,
            &actor(role),
            &mut dnd_game_core::SplitMix64::new(0),
        );
        assert_eq!(result, Err(RejectReason::invalid("intent_id")), "{role:?}");

        let env = envelope(
            rename_map("X"),
            Some(Version(dnd_protocol::MAX_SAFE_INTEGER + 1)),
        );
        let result = dnd_game_core::apply(
            &state,
            &env,
            &actor(role),
            &mut dnd_game_core::SplitMix64::new(0),
        );
        assert_eq!(
            result,
            Err(RejectReason::invalid("expected_version")),
            "{role:?}"
        );
    }
}

/// A version at the JS-safe limit cannot be bumped further.
#[test]
fn version_overflow_is_rejected() {
    let mut state = table_state();
    let max = Version(dnd_protocol::MAX_SAFE_INTEGER);
    state.maps.get_mut(&pooled_map_id(0)).unwrap().version = max;
    assert_eq!(
        run(&mut state, Role::Gm, rename_map("X"), Some(max)),
        Err(RejectReason::invalid("version"))
    );
}

/// ADR-0016: positions are Units16 within `0..=width*16` × `0..=height*16`, edges inclusive.
#[test]
fn positions_must_lie_on_the_map() {
    let mut state = table_state();
    let id = pooled_token_id(3);
    let (w, h) = (4000 * 16, 3000 * 16);
    for (x, y) in [(0, 0), (w, h), (w, 0), (0, h)] {
        assert!(
            run(&mut state, Role::Gm, Intent::MoveToken { id, x, y }, None).is_ok(),
            "({x}, {y})"
        );
    }
    for (x, y, field) in [(-1, 0, "x"), (w + 1, 0, "x"), (0, -1, "y"), (0, h + 1, "y")] {
        assert_eq!(
            run(&mut state, Role::Gm, Intent::MoveToken { id, x, y }, None),
            Err(RejectReason::invalid(field)),
            "({x}, {y})"
        );
    }
}

/// ADR-0016: map size is `1..=MAX_MAP_SIZE` logical units.
#[test]
fn map_size_is_bounded() {
    let create = |width: i64, height: i64| Intent::CreateMap {
        id: pooled_map_id(2),
        name: "M".into(),
        background_asset_id: None,
        width,
        height,
        grid: Grid::default(),
        visible_to_players: false,
    };
    let max = dnd_protocol::MAX_MAP_SIZE;
    for (w, h, field) in [
        (0, 10, "width"),
        (max + 1, 10, "width"),
        (10, 0, "height"),
        (10, -5, "height"),
    ] {
        let mut state = table_state();
        assert_eq!(
            run(&mut state, Role::Gm, create(w, h), None),
            Err(RejectReason::invalid(field))
        );
    }
    let mut state = table_state();
    run(&mut state, Role::Gm, create(max, 1), None).unwrap();
    assert_eq!(state.maps[&pooled_map_id(2)].width, max);
}
