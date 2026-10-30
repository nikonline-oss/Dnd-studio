//! Boards (ADR-0024): rights of all four roles, nodes and edges, cascades, limits, visibility.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{ForPlayers, GameState, filter_for_players, players_state};
use dnd_protocol::arbitrary::{pooled_map_id, v7};
use dnd_protocol::*;

fn board(n: u128) -> BoardId {
    BoardId(v7(0x7000 + n))
}

fn node(n: u128) -> BoardNodeId {
    BoardNodeId(v7(0x7100 + n))
}

fn edge(n: u128) -> BoardEdgeId {
    BoardEdgeId(v7(0x7200 + n))
}

fn note(text: &str) -> BoardNodeContent {
    BoardNodeContent::Note {
        markdown: text.into(),
    }
}

fn add_node(state: &mut GameState, n: u128, b: u128, visibility: BoardVisibility) {
    run(
        state,
        Role::Gm,
        Intent::CreateBoardNode {
            id: node(n),
            board_id: board(b),
            content: note(&format!("Note {n}")),
            x: 0,
            y: 0,
            width: 3200,
            height: 1600,
            visibility,
        },
        None,
    )
    .unwrap();
}

fn add_edge(state: &mut GameState, n: u128, from: u128, to: u128, visibility: BoardVisibility) {
    run(
        state,
        Role::Gm,
        Intent::CreateBoardEdge {
            id: edge(n),
            board_id: board(0),
            from: node(from),
            to: node(to),
            label: "leads to".into(),
            directed: true,
            visibility,
        },
        None,
    )
    .unwrap();
}

/// Board 0 (shown to players): node 0 and 1 open, node 2 for the GM; edge 0: 0→1 open,
/// edge 1: 1→2 open (but node 2 is hidden). Board 1 hidden, with node 3 open.
fn boards() -> GameState {
    let mut state = table_state();
    for (n, name) in [(0, "Plot"), (1, "Secrets")] {
        run(
            &mut state,
            Role::Gm,
            Intent::CreateBoard {
                id: board(n),
                name: name.into(),
            },
            None,
        )
        .unwrap();
    }
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateBoard {
            id: board(0),
            name: None,
            visible_to_players: Some(true),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    add_node(&mut state, 0, 0, BoardVisibility::Players);
    add_node(&mut state, 1, 0, BoardVisibility::Players);
    add_node(&mut state, 2, 0, BoardVisibility::Gm);
    add_node(&mut state, 3, 1, BoardVisibility::Players);
    add_edge(&mut state, 0, 0, 1, BoardVisibility::Players);
    add_edge(&mut state, 1, 1, 2, BoardVisibility::Players);
    state
}

#[test]
fn deleting_map_card_removes_map_and_every_other_reference() {
    for role in [Role::Gm, Role::CoGm] {
        let mut state = boards();
        let map_id = pooled_map_id(1);
        for id in [node(0), node(3)] {
            state.board_nodes.get_mut(&id).unwrap().content = BoardNodeContent::Link {
                target: BoardLink::Map { id: map_id },
            };
        }
        run(
            &mut state,
            role,
            Intent::DeleteBoardNode { id: node(0) },
            Some(Version::INITIAL),
        )
        .unwrap();
        assert!(!state.maps.contains_key(&map_id));
        assert!(!state.board_nodes.contains_key(&node(0)));
        assert!(!state.board_nodes.contains_key(&node(3)));
        assert!(
            state
                .board_edges
                .values()
                .all(|e| e.from != node(0) && e.to != node(0))
        );
        assert!(state.tokens.values().all(|t| t.map_id != map_id));
    }
}

#[test]
fn only_gm_roles_change_boards() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let state = boards();
        let intents = [
            (
                Intent::CreateBoard {
                    id: board(9),
                    name: "New".into(),
                },
                None,
            ),
            (
                Intent::UpdateBoard {
                    id: board(1),
                    name: Some("Renamed".into()),
                    visible_to_players: None,
                },
                Some(state.boards[&board(1)].version),
            ),
            (
                Intent::MoveBoardNode {
                    id: node(0),
                    x: 160,
                    y: 320,
                },
                None,
            ),
            (
                Intent::UpdateBoardNode {
                    id: node(2),
                    content: None,
                    width: None,
                    height: None,
                    visibility: Some(BoardVisibility::Players),
                },
                Some(Version::INITIAL),
            ),
            (
                Intent::DeleteBoardEdge { id: edge(0) },
                Some(Version::INITIAL),
            ),
            (
                Intent::DeleteBoard { id: board(1) },
                Some(state.boards[&board(1)].version),
            ),
        ];
        for (intent, version) in intents {
            let result = run(&mut state.clone(), role, intent.clone(), version);
            if allowed {
                assert!(result.is_ok(), "{role:?} {intent:?}: {result:?}");
            } else {
                assert_eq!(result, Err(RejectReason::Forbidden), "{role:?} {intent:?}");
            }
        }
    }
}

#[test]
fn deleting_cascades_edges_before_nodes_before_the_board() {
    let mut state = boards();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteBoardNode { id: node(1) },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::BoardEdgeDeleted { id: edge(0) },
            Event::BoardEdgeDeleted { id: edge(1) },
            Event::BoardNodeDeleted { id: node(1) },
        ]
    );

    let version = state.boards[&board(0)].version;
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteBoard { id: board(0) },
        Some(version),
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::BoardNodeDeleted { id: node(0) },
            Event::BoardNodeDeleted { id: node(2) },
            Event::BoardDeleted { id: board(0) },
        ]
    );
    assert!(state.board_nodes.values().all(|n| n.board_id == board(1)));
}

#[test]
fn edges_join_two_different_nodes_of_the_same_board() {
    let state = boards();
    let attempt = |from, to, board_n| {
        run(
            &mut state.clone(),
            Role::Gm,
            Intent::CreateBoardEdge {
                id: edge(9),
                board_id: board(board_n),
                from: node(from),
                to: node(to),
                label: String::new(),
                directed: false,
                visibility: BoardVisibility::Gm,
            },
            None,
        )
    };
    assert_eq!(attempt(0, 0, 0), Err(RejectReason::invalid("to")));
    assert_eq!(attempt(0, 3, 0), Err(RejectReason::invalid("to")));
    assert_eq!(attempt(3, 0, 1), Err(RejectReason::invalid("to")));
    assert!(attempt(0, 2, 0).is_ok());
}

#[test]
fn nodes_check_their_content_and_geometry() {
    let state = boards();
    let create = |content: BoardNodeContent, x: i64, width: i64| {
        run(
            &mut state.clone(),
            Role::Gm,
            Intent::CreateBoardNode {
                id: node(9),
                board_id: board(0),
                content,
                x,
                y: 0,
                width,
                height: MIN_NODE_SIZE,
                visibility: BoardVisibility::Gm,
            },
            None,
        )
    };
    let link = |id| BoardNodeContent::Link {
        target: BoardLink::Map { id },
    };
    assert!(create(link(pooled_map_id(0)), 0, MIN_NODE_SIZE).is_ok());
    assert_eq!(
        create(link(pooled_map_id(7)), 0, MIN_NODE_SIZE),
        Err(RejectReason::invalid("content"))
    );
    assert_eq!(
        create(note(&"x".repeat(MAX_NOTE_LEN + 1)), 0, MIN_NODE_SIZE),
        Err(RejectReason::invalid("content"))
    );
    assert_eq!(
        create(note(""), MAX_BOARD_COORD + 1, MIN_NODE_SIZE),
        Err(RejectReason::invalid("x"))
    );
    assert_eq!(
        create(note(""), 0, MIN_NODE_SIZE - 1),
        Err(RejectReason::invalid("width"))
    );
    assert_eq!(
        create(
            BoardNodeContent::Image {
                asset_id: AssetId(v7(0x9999))
            },
            0,
            MIN_NODE_SIZE
        ),
        Err(RejectReason::invalid("content"))
    );
}

#[test]
fn moves_are_last_write_wins_and_keep_the_version() {
    let mut state = boards();
    let events = run(
        &mut state,
        Role::Gm,
        Intent::MoveBoardNode {
            id: node(0),
            x: -480,
            y: 960,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        events,
        vec![Event::BoardNodeMoved {
            id: node(0),
            x: -480,
            y: 960
        }]
    );
    let moved = &state.board_nodes[&node(0)];
    assert_eq!(
        (moved.x, moved.y, moved.version),
        (-480, 960, Version::INITIAL)
    );
}

#[test]
fn players_see_open_elements_of_shown_boards_only() {
    let view = players_state(&boards());
    assert_eq!(view.boards.keys().copied().collect::<Vec<_>>(), [board(0)]);
    assert_eq!(
        view.board_nodes.keys().copied().collect::<Vec<_>>(),
        [node(0), node(1)]
    );
    // Edge 1 is open, but its end (node 2) is not.
    assert_eq!(
        view.board_edges.keys().copied().collect::<Vec<_>>(),
        [edge(0)]
    );
}

#[test]
fn revealing_a_node_sends_a_snapshot_and_hidden_moves_are_redacted() {
    let state = boards();
    let mut after = state.clone();
    let events = run(
        &mut after,
        Role::Gm,
        Intent::UpdateBoardNode {
            id: node(2),
            content: None,
            width: None,
            height: None,
            visibility: Some(BoardVisibility::Players),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    let batch = EventBatch {
        seq: Seq(1),
        caused_by: None,
        events,
    };
    assert_eq!(
        filter_for_players(&state, &after, &batch),
        ForPlayers::Resnapshot
    );

    let mut after = state.clone();
    let mut events = Vec::new();
    for n in [0, 2] {
        events.extend(
            run(
                &mut after,
                Role::Gm,
                Intent::MoveBoardNode {
                    id: node(n),
                    x: 16,
                    y: 16,
                },
                None,
            )
            .unwrap(),
        );
    }
    let batch = EventBatch {
        seq: Seq(1),
        caused_by: None,
        events,
    };
    let ForPlayers::Batch(filtered) = filter_for_players(&state, &after, &batch) else {
        panic!("a move is not a visibility change");
    };
    assert_eq!(
        filtered.events,
        vec![
            Event::BoardNodeMoved {
                id: node(0),
                x: 16,
                y: 16
            },
            Event::Redacted
        ]
    );
}

#[test]
fn players_see_the_pictures_of_journal_entries_open_to_them() {
    let picture = |n: u128| AssetDto {
        id: AssetId(v7(0x7700 + n)),
        usage: AssetUsage::Token,
        sha256: format!("{n:064x}"),
        mime: "image/png".into(),
        size_bytes: 10,
        original_name: Some(format!("picture {n}.png")),
        width: 10,
        height: 10,
        display_sha256: Some(format!("{n:064x}")),
        thumbnail_sha256: format!("{n:064x}"),
        tiles: Vec::new(),
        folder_id: None,
        version: Version::INITIAL,
    };
    let mut state = table_state();
    for n in [0, 1] {
        let asset = picture(n);
        state.assets.insert(asset.id, asset);
    }
    for (n, visibility) in [(0, JournalVisibility::Players), (1, JournalVisibility::Gm)] {
        run(
            &mut state,
            Role::Gm,
            Intent::CreateJournalEntry {
                id: JournalEntryId(v7(0x7710 + n)),
                folder_id: None,
                title: format!("Entry {n}"),
                markdown: format!("See ![the hall](asset:{}) here.", AssetId(v7(0x7700 + n))),
                visibility,
                players_can_edit: false,
                table: None,
            },
            None,
        )
        .unwrap();
    }
    let seen = players_state(&state);
    assert_eq!(
        seen.assets.keys().copied().collect::<Vec<_>>(),
        [AssetId(v7(0x7700))]
    );
    assert_eq!(
        dnd_game_core::journal_pictures("![a](asset:nope) ![b](https://x) text").count(),
        0
    );
}

#[test]
fn trigger_blocks_stay_with_the_gm() {
    let mut state = boards();
    let trigger = TriggerId(v7(0x7800));
    run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger,
            definition: TriggerDef {
                name: "Collapse".into(),
                enabled: true,
                once: false,
                event: TriggerEvent::Manual,
                tokens: Vec::new(),
                condition: None,
                steps: vec![TriggerStep {
                    when: None,
                    action: TriggerAction::Chat {
                        text: "The ceiling falls.".into(),
                        gm_only: false,
                    },
                }],
            },
        },
        None,
    )
    .unwrap();
    // Asked to be shown to players: stored as the GM's anyway.
    run(
        &mut state,
        Role::Gm,
        Intent::CreateBoardNode {
            id: node(9),
            board_id: board(0),
            content: BoardNodeContent::Trigger { id: trigger },
            x: 0,
            y: 0,
            width: 3200,
            height: 1600,
            visibility: BoardVisibility::Players,
        },
        None,
    )
    .unwrap();
    assert_eq!(state.board_nodes[&node(9)].visibility, BoardVisibility::Gm);
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateBoardNode {
            id: node(9),
            content: None,
            width: None,
            height: None,
            visibility: Some(BoardVisibility::Players),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert_eq!(state.board_nodes[&node(9)].visibility, BoardVisibility::Gm);
    assert!(!players_state(&state).board_nodes.contains_key(&node(9)));
    // A missing trigger cannot become a block.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateBoardNode {
                id: node(10),
                board_id: board(0),
                content: BoardNodeContent::Trigger {
                    id: TriggerId(v7(0x7801))
                },
                x: 0,
                y: 0,
                width: 3200,
                height: 1600,
                visibility: BoardVisibility::Gm,
            },
            None,
        ),
        Err(RejectReason::invalid("content"))
    );
}
