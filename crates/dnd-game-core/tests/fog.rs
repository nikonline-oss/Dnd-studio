//! Fog of war (ADR-0019): rights of all four roles, shape validation, limits, idempotency.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::evolve;
use dnd_protocol::arbitrary::{pooled_map_id, v7};
use dnd_protocol::*;

/// Map 0 is 4000×3000 logical units, i.e. 64000×48000 in Units16.
const W16: i64 = 4000 * 16;
const H16: i64 = 3000 * 16;

fn op(n: u128, kind: FogOpKind, shape: FogShape) -> FogOp {
    FogOp {
        id: FogOpId(v7(0x9000 + n)),
        kind,
        shape,
    }
}

fn rect(x: i64, y: i64, width: i64, height: i64) -> FogShape {
    FogShape::Rect {
        x,
        y,
        width,
        height,
    }
}

fn brush(radius: i64, points: &[i64]) -> FogShape {
    FogShape::Brush {
        radius,
        points: points.to_vec(),
    }
}

fn apply_fog(op: FogOp) -> Intent {
    Intent::ApplyFog {
        map_id: pooled_map_id(0),
        op,
    }
}

#[test]
fn only_gm_roles_change_fog() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = table_state();
        let reveal = apply_fog(op(1, FogOpKind::Reveal, rect(0, 0, 160, 160)));
        let reset = Intent::ResetFog {
            map_id: pooled_map_id(0),
            covered: true,
        };
        for intent in [reveal, reset] {
            let result = run(&mut state, role, intent, None);
            if allowed {
                assert!(result.is_ok(), "{role:?}: {result:?}");
            } else {
                assert_eq!(result, Err(RejectReason::Forbidden), "{role:?}");
            }
        }
    }
}

#[test]
fn operations_are_kept_in_order_over_the_initial_state() {
    let mut state = table_state();
    let map_id = pooled_map_id(0);
    run(
        &mut state,
        Role::Gm,
        Intent::ResetFog {
            map_id,
            covered: true,
        },
        None,
    )
    .unwrap();
    let reveal = op(1, FogOpKind::Reveal, rect(0, 0, 1600, 1600));
    let hide = op(2, FogOpKind::Hide, brush(160, &[800, 800, 1200, 900]));
    run(&mut state, Role::Gm, apply_fog(reveal.clone()), None).unwrap();
    run(&mut state, Role::Gm, apply_fog(hide.clone()), None).unwrap();

    let fog = &state.fog[&map_id];
    assert!(fog.covered);
    assert_eq!(fog.ops, vec![reveal, hide]);
    // Fog never bumps the map version (no conflicts with grid or name edits).
    assert_eq!(state.maps[&map_id].version, Version::INITIAL);
    // And it is part of the snapshot.
    assert_eq!(state.to_snapshot(Seq(0)).fog, vec![fog.clone()]);
}

#[test]
fn reset_drops_all_operations() {
    let mut state = table_state();
    let map_id = pooled_map_id(0);
    run(
        &mut state,
        Role::Gm,
        apply_fog(op(1, FogOpKind::Hide, rect(0, 0, 16, 16))),
        None,
    )
    .unwrap();
    // A map that never had a reset starts clear.
    assert!(!state.fog[&map_id].covered);
    let events = run(
        &mut state,
        Role::Gm,
        Intent::ResetFog {
            map_id,
            covered: false,
        },
        None,
    )
    .unwrap();
    assert_eq!(
        events,
        vec![Event::FogReset {
            map_id,
            covered: false
        }]
    );
    assert!(state.fog[&map_id].ops.is_empty());
}

#[test]
fn replaying_an_operation_draws_it_once() {
    let mut state = table_state();
    let events = run(
        &mut state,
        Role::Gm,
        apply_fog(op(1, FogOpKind::Reveal, rect(0, 0, 16, 16))),
        None,
    )
    .unwrap();
    for event in &events {
        evolve(&mut state, event);
    }
    assert_eq!(state.fog[&pooled_map_id(0)].ops.len(), 1);
    // The operation id is taken: sending it again is a duplicate.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            apply_fog(op(1, FogOpKind::Hide, rect(0, 0, 16, 16))),
            None
        ),
        Err(RejectReason::AlreadyExists)
    );
}

#[test]
fn shapes_must_lie_on_the_map() {
    let cases = [
        (rect(-1, 0, 16, 16), "op.shape.x"),
        (rect(W16 - 15, 0, 16, 16), "op.shape.x"),
        (rect(0, 0, 0, 16), "op.shape.x"),
        (rect(0, H16, 16, 16), "op.shape.y"),
        (rect(0, 0, 16, -16), "op.shape.y"),
        // Outside the JS-safe range: caught by envelope validation first.
        (rect(0, 0, 16, i64::MAX), "op.shape.height"),
        (brush(0, &[10, 10]), "op.shape.radius"),
        (brush(MAX_BRUSH_RADIUS + 1, &[10, 10]), "op.shape.radius"),
        (brush(16, &[]), "op.shape.points"),
        (brush(16, &[10, 10, 20]), "op.shape.points"),
        (brush(16, &[10, H16 + 1]), "op.shape.points"),
        (
            brush(16, &vec![10; (MAX_BRUSH_POINTS + 1) * 2]),
            "op.shape.points",
        ),
    ];
    for (shape, field) in cases {
        let mut state = table_state();
        assert_eq!(
            run(
                &mut state,
                Role::Gm,
                apply_fog(op(1, FogOpKind::Reveal, shape.clone())),
                None
            ),
            Err(RejectReason::invalid(field)),
            "{shape:?}"
        );
    }
    // Edges are included.
    let mut state = table_state();
    for (n, shape) in [
        rect(0, 0, W16, H16),
        brush(
            MAX_BRUSH_RADIUS,
            &vec![W16; MAX_BRUSH_POINTS * 2]
                .iter()
                .enumerate()
                .map(|(i, v)| if i % 2 == 0 { *v } else { H16 })
                .collect::<Vec<_>>(),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        run(
            &mut state,
            Role::Gm,
            apply_fog(op(n as u128, FogOpKind::Hide, shape)),
            None,
        )
        .unwrap();
    }
}

#[test]
fn unknown_maps_are_not_found() {
    let mut state = table_state();
    let missing = MapId(v7(0x7777));
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::ApplyFog {
                map_id: missing,
                op: op(1, FogOpKind::Reveal, rect(0, 0, 16, 16)),
            },
            None
        ),
        Err(RejectReason::NotFound)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::ResetFog {
                map_id: missing,
                covered: true
            },
            None
        ),
        Err(RejectReason::NotFound)
    );
}

#[test]
fn operation_and_point_limits_ask_for_a_reset() {
    let map_id = pooled_map_id(0);
    let mut state = table_state();
    // Fill the map up to the operation limit directly (thousands of runs would be slow).
    state.fog.insert(
        map_id,
        FogDto {
            map_id,
            covered: true,
            ops: (0..MAX_FOG_OPS)
                .map(|n| op(0x10_0000 + n as u128, FogOpKind::Reveal, rect(0, 0, 16, 16)))
                .collect(),
        },
    );
    let next = apply_fog(op(1, FogOpKind::Reveal, rect(0, 0, 16, 16)));
    assert_eq!(
        run(&mut state, Role::Gm, next.clone(), None),
        Err(RejectReason::LimitExceeded {
            limit: "fog_ops".into()
        })
    );
    // A reset makes room again.
    run(
        &mut state,
        Role::Gm,
        Intent::ResetFog {
            map_id,
            covered: true,
        },
        None,
    )
    .unwrap();
    run(&mut state, Role::Gm, next, None).unwrap();

    // Points: one stroke short of the budget fits, one point more does not.
    let stroke = |n: u128, count: usize| {
        apply_fog(op(n, FogOpKind::Reveal, brush(16, &vec![100; count * 2])))
    };
    let mut state = table_state();
    let full_strokes = MAX_FOG_POINTS / MAX_BRUSH_POINTS;
    for n in 0..full_strokes {
        run(
            &mut state,
            Role::Gm,
            stroke(0x100 + n as u128, MAX_BRUSH_POINTS),
            None,
        )
        .unwrap();
    }
    assert_eq!(
        run(&mut state, Role::Gm, stroke(0x50, 1), None),
        Err(RejectReason::LimitExceeded {
            limit: "fog_points".into()
        })
    );
    // Rectangles carry no points and still fit.
    run(
        &mut state,
        Role::Gm,
        apply_fog(op(0x51, FogOpKind::Hide, rect(0, 0, 16, 16))),
        None,
    )
    .unwrap();
}

#[test]
fn deleting_a_map_drops_its_fog() {
    let mut state = table_state();
    let map_id = pooled_map_id(0);
    run(
        &mut state,
        Role::Gm,
        apply_fog(op(1, FogOpKind::Reveal, rect(0, 0, 16, 16))),
        None,
    )
    .unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteMap { id: map_id },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert!(!state.fog.contains_key(&map_id));
}
