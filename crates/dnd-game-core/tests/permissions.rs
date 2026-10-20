//! Role × intent permission matrix (CLAUDE.md §5) for all four roles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::authorize;
use dnd_protocol::arbitrary::{pooled_map_id, pooled_token_id};
use dnd_protocol::*;

fn intents() -> Vec<(&'static str, Intent)> {
    vec![
        (
            "update_campaign_meta",
            Intent::UpdateCampaignMeta {
                name: Some("Renamed".into()),
                description: None,
            },
        ),
        (
            "create_map",
            Intent::CreateMap {
                id: pooled_map_id(2),
                name: "New".into(),
                background_asset_id: None,
                width: 4000,
                height: 3000,
                grid: Grid::default(),
                visible_to_players: true,
            },
        ),
        (
            "update_map",
            Intent::UpdateMap {
                id: pooled_map_id(0),
                name: Some("Renamed".into()),
                grid: None,
                visible_to_players: None,
            },
        ),
        (
            "delete_map",
            Intent::DeleteMap {
                id: pooled_map_id(0),
            },
        ),
        (
            "create_token",
            Intent::CreateToken {
                id: pooled_token_id(5),
                map_id: pooled_map_id(0),
                character_id: None,
                asset_id: None,
                name: "Orc".into(),
                x: 1,
                y: 2,
                rotation: 0.0,
                scale: 1.0,
                layer: TokenLayer::Tokens,
                visible: true,
            },
        ),
        (
            "update_token",
            Intent::UpdateToken {
                id: pooled_token_id(0),
                name: Some("Renamed".into()),
                rotation: None,
                scale: None,
                layer: None,
                visible: None,
            },
        ),
        (
            "move_own_token",
            Intent::MoveToken {
                id: pooled_token_id(0),
                x: 10,
                y: 10,
            },
        ),
        (
            "move_foreign_token",
            Intent::MoveToken {
                id: pooled_token_id(1),
                x: 10,
                y: 10,
            },
        ),
        (
            "move_own_token_on_gm_layer",
            Intent::MoveToken {
                id: pooled_token_id(2),
                x: 10,
                y: 10,
            },
        ),
        (
            "move_npc_token",
            Intent::MoveToken {
                id: pooled_token_id(3),
                x: 10,
                y: 10,
            },
        ),
        (
            "move_own_token_on_hidden_map",
            Intent::MoveToken {
                id: pooled_token_id(4),
                x: 10,
                y: 10,
            },
        ),
        (
            "move_missing_token",
            Intent::MoveToken {
                id: pooled_token_id(5),
                x: 10,
                y: 10,
            },
        ),
        (
            "delete_token",
            Intent::DeleteToken {
                id: pooled_token_id(0),
            },
        ),
    ]
}

fn allowed(role: Role, case: &str) -> bool {
    match role {
        Role::Gm | Role::CoGm => true,
        Role::Player => case == "move_own_token",
        Role::Spectator => false,
    }
}

#[test]
fn permission_matrix() {
    let state = table_state();
    let mut failures = Vec::new();
    for role in Role::ALL {
        for (case, intent) in intents() {
            let result = authorize(&actor(role), &intent, &state);
            let expected = allowed(role, case);
            if result.is_ok() != expected {
                failures.push(format!(
                    "{role:?} / {case}: expected allowed={expected}, got {result:?}"
                ));
            }
            if let Err(reason) = result {
                assert_eq!(reason, RejectReason::Forbidden, "{role:?} / {case}");
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn player_move_goes_through_full_pipeline() {
    let mut state = table_state();
    let events = run(
        &mut state,
        Role::Player,
        Intent::MoveToken {
            id: pooled_token_id(0),
            x: 35,
            y: 70,
        },
        None,
    )
    .unwrap();
    assert_eq!(events.len(), 1);
    let token = &state.tokens[&pooled_token_id(0)];
    assert_eq!((token.x, token.y), (35, 70));
}

#[test]
fn spectator_cannot_even_move() {
    let mut state = table_state();
    let result = run(
        &mut state,
        Role::Spectator,
        Intent::MoveToken {
            id: pooled_token_id(0),
            x: 1,
            y: 1,
        },
        None,
    );
    assert_eq!(result, Err(RejectReason::Forbidden));
    assert_eq!(state, table_state());
}
