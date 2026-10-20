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
    for (x, expected) in [(10.0, None), (20.0, Some(Version(42)))] {
        run(
            &mut state,
            Role::Gm,
            Intent::MoveToken { id, x, y: 0.0 },
            expected,
        )
        .unwrap();
    }
    let token = &state.tokens[&id];
    assert_eq!(token.x, 20.0);
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
        x: 0.0,
        y: 0.0,
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
                x: f64::NAN,
                y: 0.0,
            },
            "x",
        ),
        (
            Intent::MoveToken {
                id: pooled_token_id(0),
                x: 0.0,
                y: f64::INFINITY,
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
