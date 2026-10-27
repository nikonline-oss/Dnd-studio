//! Zones (ADR-0024): rights of all four roles, shapes on the map, cascade with the map, and that
//! players never see them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{ForPlayers, GameState, filter_for_players, players_state};
use dnd_protocol::arbitrary::{pooled_map_id, v7};
use dnd_protocol::*;

fn zone(n: u128) -> ZoneId {
    ZoneId(v7(0x7300 + n))
}

fn rect(x: i64, y: i64, width: i64, height: i64) -> ZoneShape {
    ZoneShape::Rect {
        x,
        y,
        width,
        height,
    }
}

/// Map 0 (4000×3000 units) has zone 0, the chapel.
fn zoned() -> GameState {
    let mut state = table_state();
    run(
        &mut state,
        Role::Gm,
        Intent::CreateZone {
            id: zone(0),
            map_id: pooled_map_id(0),
            name: "Chapel".into(),
            shape: rect(1600, 1600, 3200, 3200),
        },
        None,
    )
    .unwrap();
    state
}

#[test]
fn only_gm_roles_manage_zones() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let state = zoned();
        let intents = [
            (
                Intent::CreateZone {
                    id: zone(9),
                    map_id: pooled_map_id(0),
                    name: "Crypt".into(),
                    shape: rect(0, 0, 160, 160),
                },
                None,
            ),
            (
                Intent::UpdateZone {
                    id: zone(0),
                    name: Some("Old chapel".into()),
                    shape: None,
                },
                Some(Version::INITIAL),
            ),
            (Intent::DeleteZone { id: zone(0) }, Some(Version::INITIAL)),
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
fn shapes_lie_on_the_map() {
    let state = zoned();
    let create = |shape: ZoneShape| {
        run(
            &mut state.clone(),
            Role::Gm,
            Intent::CreateZone {
                id: zone(9),
                map_id: pooled_map_id(0),
                name: "Z".into(),
                shape,
            },
            None,
        )
    };
    let invalid = Err(RejectReason::invalid("shape"));
    // The map is 4000×3000 units = 64 000×48 000 Units16.
    assert!(create(rect(0, 0, 64_000, 48_000)).is_ok());
    assert_eq!(create(rect(0, 0, 64_001, 16)), invalid);
    assert_eq!(create(rect(-1, 0, 16, 16)), invalid);
    assert_eq!(create(rect(0, 0, 0, 16)), invalid);
    assert!(
        create(ZoneShape::Polygon {
            points: vec![0, 0, 160, 0, 80, 160]
        })
        .is_ok()
    );
    assert_eq!(
        create(ZoneShape::Polygon {
            points: vec![0, 0, 160, 0]
        }),
        invalid
    );
    assert_eq!(
        create(ZoneShape::Polygon {
            points: vec![0, 0, 160, 0, 80]
        }),
        invalid
    );
    assert_eq!(
        create(ZoneShape::Polygon {
            points: (0..=MAX_ZONE_POINTS as i64).flat_map(|i| [i, i]).collect()
        }),
        invalid
    );
}

#[test]
fn zones_go_with_their_map() {
    let mut state = zoned();
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteMap {
            id: pooled_map_id(0),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert!(state.zones.is_empty());
}

#[test]
fn players_never_see_zones() {
    let before = table_state();
    let mut after = before.clone();
    let events = run(
        &mut after,
        Role::Gm,
        Intent::CreateZone {
            id: zone(0),
            map_id: pooled_map_id(0),
            name: "Chapel".into(),
            shape: rect(0, 0, 160, 160),
        },
        None,
    )
    .unwrap();
    assert!(players_state(&after).zones.is_empty());
    let batch = EventBatch {
        seq: Seq(1),
        caused_by: None,
        events,
    };
    assert_eq!(
        filter_for_players(&before, &after, &batch),
        ForPlayers::Batch(EventBatch {
            events: vec![Event::Redacted],
            ..batch
        })
    );
}
