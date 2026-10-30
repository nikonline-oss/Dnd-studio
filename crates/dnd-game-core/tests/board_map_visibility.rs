//! Effective map visibility through shared board links (ADR-0024).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;
use dnd_game_core::{
    ForPlayers, GameState, PlayersSee, evolve_all, filter_for_players, may_change_visibility,
    players_state, snapshot_for,
};
use dnd_protocol::arbitrary::{pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;

fn board(n: u128) -> BoardId {
    BoardId(v7(0xB000 + n))
}
fn node(n: u128) -> BoardNodeId {
    BoardNodeId(v7(0xB100 + n))
}
fn link(n: u128) -> BoardNodeDto {
    BoardNodeDto {
        id: node(n),
        board_id: board(n),
        content: BoardNodeContent::Link {
            target: BoardLink::Map {
                id: pooled_map_id(1),
            },
        },
        x: 0,
        y: 0,
        width: 3200,
        height: 1600,
        visibility: BoardVisibility::Players,
        version: Version::INITIAL,
    }
}
fn state() -> GameState {
    let mut state = table_state();
    for n in 0..2 {
        state.boards.insert(
            board(n),
            BoardDto {
                id: board(n),
                name: format!("Board {n}"),
                sort_order: 0,
                visible_to_players: true,
                version: Version::INITIAL,
            },
        );
    }
    state
}
fn transition(before: &GameState, events: Vec<Event>) -> (GameState, ForPlayers) {
    let mut after = before.clone();
    evolve_all(&mut after, &events);
    let batch = EventBatch {
        seq: Seq(100),
        caused_by: None,
        events,
    };
    let filtered = filter_for_players(before, &after, &batch);
    (after, filtered)
}

#[test]
fn map_link_requires_both_shared_board_and_shared_card() {
    for board_visible in [false, true] {
        for card_visible in [BoardVisibility::Gm, BoardVisibility::Players] {
            let mut state = state();
            state.boards.get_mut(&board(0)).unwrap().visible_to_players = board_visible;
            let mut card = link(0);
            card.visibility = card_visible;
            state.board_nodes.insert(card.id, card);
            let visible = board_visible && card_visible == BoardVisibility::Players;
            for role in [Role::Player, Role::Spectator, Role::Gm, Role::CoGm] {
                let snapshot = snapshot_for(role, &state, Seq(1), []);
                assert_eq!(
                    snapshot.maps.iter().any(|m| m.id == pooled_map_id(1)),
                    visible || matches!(role, Role::Gm | Role::CoGm),
                    "{role:?}"
                );
            }
            assert!(!state.maps[&pooled_map_id(1)].visible_to_players);
        }
    }
}

#[test]
fn shared_link_cascades_background_fog_visible_tokens_and_combat() {
    let mut state = state();
    state.board_nodes.insert(node(0), link(0));
    for n in 0..3 {
        let id = AssetId(v7(0xB200 + n));
        state.assets.insert(
            id,
            AssetDto {
                id,
                usage: if n == 0 {
                    AssetUsage::Map
                } else {
                    AssetUsage::Token
                },
                sha256: format!("{n:064x}"),
                mime: "image/png".into(),
                size_bytes: 100,
                original_name: None,
                width: 10,
                height: 10,
                display_sha256: Some(format!("{n:064x}")),
                thumbnail_sha256: format!("{n:064x}"),
                tiles: vec![],
                folder_id: None,
                version: Version::INITIAL,
            },
        );
    }
    state
        .maps
        .get_mut(&pooled_map_id(1))
        .unwrap()
        .background_asset_id = Some(AssetId(v7(0xB200)));
    state.tokens.get_mut(&pooled_token_id(4)).unwrap().asset_id = Some(AssetId(v7(0xB201)));
    let mut gm_token = token(5, 1, None, TokenLayer::Gm);
    gm_token.asset_id = Some(AssetId(v7(0xB202)));
    state.tokens.insert(gm_token.id, gm_token);
    let mut hidden = token(6, 1, None, TokenLayer::Tokens);
    hidden.visible = false;
    state.tokens.insert(hidden.id, hidden);
    state.fog.insert(
        pooled_map_id(1),
        FogDto {
            map_id: pooled_map_id(1),
            covered: true,
            ops: vec![],
        },
    );
    let combat = CombatDto {
        id: CombatId(v7(0xB300)),
        map_id: pooled_map_id(1),
        round: 1,
        turn: None,
        version: Version::INITIAL,
    };
    state.combats.insert(combat.id, combat.clone());
    for n in 4..7 {
        let id = CombatantId(v7(0xB400 + n));
        state.combatants.insert(
            id,
            CombatantDto {
                id,
                combat_id: combat.id,
                token_id: Some(pooled_token_id(n)),
                character_id: None,
                name: format!("Fighter {n}"),
                initiative: None,
                sort_order: 0,
                version: Version::INITIAL,
            },
        );
    }
    let view = players_state(&state);
    assert!(view.maps.contains_key(&pooled_map_id(1)));
    assert_eq!(view.assets.len(), 2);
    assert!(view.assets.contains_key(&AssetId(v7(0xB200))));
    assert!(view.assets.contains_key(&AssetId(v7(0xB201))));
    assert_eq!(view.fog, state.fog);
    assert!(view.tokens.contains_key(&pooled_token_id(4)));
    assert!(!view.tokens.contains_key(&pooled_token_id(5)));
    assert!(!view.tokens.contains_key(&pooled_token_id(6)));
    assert_eq!(view.combats, state.combats);
    assert_eq!(view.combatants.len(), 1);
    let (after, filtered) = transition(&state, vec![Event::BoardNodeDeleted { id: node(0) }]);
    assert_eq!(filtered, ForPlayers::Resnapshot);
    let view = players_state(&after);
    assert!(
        view.assets.is_empty()
            && view.fog.is_empty()
            && view.combats.is_empty()
            && view.combatants.is_empty()
    );
    assert!(!view.tokens.contains_key(&pooled_token_id(4)));
}

#[test]
fn creating_revealing_or_retargeting_a_link_resnapshots() {
    let state = state();
    let (mut shown, result) = transition(&state, vec![Event::BoardNodeCreated { node: link(0) }]);
    assert_eq!(result, ForPlayers::Resnapshot);
    let mut hidden = shown.clone();
    hidden.board_nodes.get_mut(&node(0)).unwrap().visibility = BoardVisibility::Gm;
    assert_eq!(
        transition(&hidden, vec![Event::BoardNodeUpdated { node: link(0) }]).1,
        ForPlayers::Resnapshot
    );
    let mut hidden = shown.clone();
    hidden.boards.get_mut(&board(0)).unwrap().visible_to_players = false;
    assert_eq!(
        transition(
            &hidden,
            vec![Event::BoardUpdated {
                board: shown.boards[&board(0)].clone()
            }]
        )
        .1,
        ForPlayers::Resnapshot
    );
    let mut retargeted = link(0);
    retargeted.content = BoardNodeContent::Link {
        target: BoardLink::Map {
            id: pooled_map_id(0),
        },
    };
    assert_eq!(
        transition(&shown, vec![Event::BoardNodeUpdated { node: retargeted }]).1,
        ForPlayers::Resnapshot
    );
    // Actual GM intent path, as used by the server, also causes a visibility-aware batch.
    shown.board_nodes.get_mut(&node(0)).unwrap().visibility = BoardVisibility::Gm;
    let before = shown.clone();
    let events = run(
        &mut shown,
        Role::CoGm,
        Intent::UpdateBoardNode {
            id: node(0),
            content: None,
            width: None,
            height: None,
            visibility: Some(BoardVisibility::Players),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    let batch = EventBatch {
        seq: Seq(101),
        caused_by: None,
        events,
    };
    assert!(may_change_visibility(&batch));
    assert_eq!(
        filter_for_players(&before, &shown, &batch),
        ForPlayers::Resnapshot
    );
}

#[test]
fn revoke_only_after_last_visible_link_and_without_explicit_flag() {
    for second_link in [false, true] {
        for explicit in [false, true] {
            let mut state = state();
            state
                .maps
                .get_mut(&pooled_map_id(1))
                .unwrap()
                .visible_to_players = explicit;
            state.board_nodes.insert(node(0), link(0));
            if second_link {
                state.board_nodes.insert(node(1), link(1));
            }
            let mut hidden_board = state.boards[&board(0)].clone();
            hidden_board.visible_to_players = false;
            let mut hidden_card = link(0);
            hidden_card.visibility = BoardVisibility::Gm;
            for event in [
                Event::BoardDeleted { id: board(0) },
                Event::BoardNodeDeleted { id: node(0) },
                Event::BoardUpdated {
                    board: hidden_board,
                },
                Event::BoardNodeUpdated { node: hidden_card },
            ] {
                assert!(PlayersSee::of(&state).maps.contains(&pooled_map_id(1)));
                let (after, result) = transition(&state, vec![event]);
                assert_eq!(
                    PlayersSee::of(&after).maps.contains(&pooled_map_id(1)),
                    second_link || explicit
                );
                if !second_link && !explicit {
                    assert_eq!(result, ForPlayers::Resnapshot);
                }
            }
        }
    }
}

#[test]
fn cheap_batches_use_effective_visibility_for_moves_fog_and_combat() {
    for linked in [false, true] {
        let mut state = state();
        if linked {
            state.board_nodes.insert(node(0), link(0));
        }
        let batch = EventBatch {
            seq: Seq(100),
            caused_by: None,
            events: vec![
                Event::TokenMoved {
                    id: pooled_token_id(4),
                    x: 10,
                    y: 20,
                },
                Event::FogReset {
                    map_id: pooled_map_id(1),
                    covered: true,
                },
                Event::CombatUpdated {
                    combat: CombatDto {
                        id: CombatId(v7(0xB300)),
                        map_id: pooled_map_id(1),
                        round: 2,
                        turn: None,
                        version: Version(2),
                    },
                },
                Event::TokenMoved {
                    id: pooled_token_id(2),
                    x: 10,
                    y: 20,
                },
            ],
        };
        assert!(!may_change_visibility(&batch));
        let ForPlayers::Batch(filtered) = filter_for_players(&state, &state, &batch) else {
            panic!("expected cheap batch")
        };
        assert_eq!(
            filtered.events[..3],
            if linked {
                batch.events[..3].to_vec()
            } else {
                vec![Event::Redacted; 3]
            }
        );
        assert_eq!(filtered.events[3], Event::Redacted);
    }
}

#[test]
fn dangling_links_and_notes_do_not_create_map_access() {
    let mut state = state();
    let mut card = link(0);
    card.content = BoardNodeContent::Note {
        markdown: format!("[map](map:{})", pooled_map_id(1)),
    };
    state.board_nodes.insert(card.id, card);
    assert!(!PlayersSee::of(&state).maps.contains(&pooled_map_id(1)));
    let mut card = link(0);
    card.board_id = board(99);
    state.board_nodes.insert(card.id, card);
    assert!(!PlayersSee::of(&state).maps.contains(&pooled_map_id(1)));
    state.board_nodes.insert(node(0), link(0));
    state.maps.remove(&pooled_map_id(1));
    assert!(!PlayersSee::of(&state).maps.contains(&pooled_map_id(1)));
}

#[test]
fn trigger_revealing_a_map_link_resnapshots_the_map() {
    let mut state = state();
    let mut card = link(0);
    card.visibility = BoardVisibility::Gm;
    state.board_nodes.insert(card.id, card);
    let trigger_id = TriggerId(v7(0xB500));
    run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger_id,
            definition: TriggerDef {
                name: "Reveal route".into(),
                enabled: true,
                once: false,
                event: TriggerEvent::Manual,
                tokens: vec![],
                condition: None,
                steps: vec![TriggerStep {
                    when: None,
                    action: TriggerAction::Reveal {
                        target: RevealTarget::Node { id: node(0) },
                    },
                }],
            },
        },
        None,
    )
    .unwrap();
    let before = state.clone();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger_id,
            token: None,
        },
        None,
    )
    .unwrap();
    let batch = EventBatch {
        seq: Seq(100),
        caused_by: None,
        events,
    };
    assert!(may_change_visibility(&batch));
    assert_eq!(
        filter_for_players(&before, &state, &batch),
        ForPlayers::Resnapshot
    );
    assert!(players_state(&state).maps.contains_key(&pooled_map_id(1)));
    assert!(players_state(&state).triggers.is_empty());
}

#[test]
fn owned_tokens_are_playable_on_link_revealed_maps_only_for_allowed_roles() {
    let mut shared = state();
    shared.board_nodes.insert(node(0), link(0));
    for role in [Role::Gm, Role::CoGm, Role::Player, Role::Spectator] {
        let mut state = shared.clone();
        let result = run(
            &mut state,
            role,
            Intent::MoveToken {
                id: pooled_token_id(4),
                x: 160,
                y: 320,
            },
            None,
        );
        if role == Role::Spectator {
            assert_eq!(result, Err(RejectReason::Forbidden));
        } else {
            assert!(result.is_ok(), "{role:?}: {result:?}");
            assert_eq!(state.tokens[&pooled_token_id(4)].x, 160);
        }
        assert!(!state.maps[&pooled_map_id(1)].visible_to_players);
    }
    for token in [
        token(7, 1, Some(1), TokenLayer::Tokens),
        token(8, 1, Some(0), TokenLayer::Gm),
    ] {
        shared.tokens.insert(token.id, token.clone());
        assert_eq!(
            run(
                &mut shared,
                Role::Player,
                Intent::MoveToken {
                    id: token.id,
                    x: 160,
                    y: 320,
                },
                None
            ),
            Err(RejectReason::Forbidden)
        );
    }
}

#[test]
fn removing_last_link_revokes_movement_but_other_grants_keep_it_playable() {
    for other_link in [false, true] {
        for explicit in [false, true] {
            let mut shared = state();
            shared
                .maps
                .get_mut(&pooled_map_id(1))
                .unwrap()
                .visible_to_players = explicit;
            shared.board_nodes.insert(node(0), link(0));
            if other_link {
                shared.board_nodes.insert(node(1), link(1));
            }
            let mut hidden_board = shared.boards[&board(0)].clone();
            hidden_board.visible_to_players = false;
            let mut hidden_card = link(0);
            hidden_card.visibility = BoardVisibility::Gm;
            for event in [
                Event::BoardDeleted { id: board(0) },
                Event::BoardNodeDeleted { id: node(0) },
                Event::BoardUpdated {
                    board: hidden_board,
                },
                Event::BoardNodeUpdated { node: hidden_card },
            ] {
                let (mut revoked, _) = transition(&shared, vec![event]);
                let movement = Intent::MoveToken {
                    id: pooled_token_id(4),
                    x: 160,
                    y: 320,
                };
                assert_eq!(
                    run(&mut revoked, Role::Spectator, movement.clone(), None),
                    Err(RejectReason::Forbidden)
                );
                let result = run(&mut revoked, Role::Player, movement, None);
                assert_eq!(result.is_ok(), other_link || explicit);
                if !other_link && !explicit {
                    assert_eq!(result, Err(RejectReason::Forbidden));
                }
            }
        }
    }
}

#[test]
fn player_projection_exposes_effective_flag_without_changing_authority() {
    let mut state = state();
    state.board_nodes.insert(node(0), link(0));
    for role in [Role::Player, Role::Spectator, Role::Gm, Role::CoGm] {
        let snapshot = snapshot_for(role, &state, Seq(1), []);
        let map = snapshot
            .maps
            .iter()
            .find(|m| m.id == pooled_map_id(1))
            .unwrap();
        assert_eq!(
            map.visible_to_players,
            matches!(role, Role::Player | Role::Spectator)
        );
        assert_eq!(map.version, Version::INITIAL);
    }
    assert!(!state.maps[&pooled_map_id(1)].visible_to_players);
}

#[test]
fn map_events_keep_player_effective_flag_and_projection_consistent() {
    let mut before = state();
    before.board_nodes.insert(node(0), link(0));
    let mut renamed = before.maps[&pooled_map_id(1)].clone();
    renamed.name = "Accessible route".into();
    renamed.version = Version(2);
    for create in [false, true] {
        let mut before = before.clone();
        let event = if create {
            before.maps.remove(&renamed.id);
            before.tokens.retain(|_, token| token.map_id != renamed.id);
            Event::MapCreated {
                map: renamed.clone(),
            }
        } else {
            Event::MapUpdated {
                map: renamed.clone(),
            }
        };
        let (after, filtered) = transition(&before, vec![event]);
        let ForPlayers::Batch(batch) = filtered else {
            panic!("map change should remain an event")
        };
        let projected_map = match &batch.events[0] {
            Event::MapCreated { map } | Event::MapUpdated { map } => map,
            _ => panic!("map event missing"),
        };
        assert!(projected_map.visible_to_players);
        assert!(!after.maps[&renamed.id].visible_to_players);
        let mut replica = players_state(&before);
        evolve_all(&mut replica, &batch.events);
        assert_eq!(replica, players_state(&after));
    }
}
