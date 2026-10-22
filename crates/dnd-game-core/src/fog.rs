//! Fog of war (ADR-0019): an ordered list of reveal/hide shapes over an initial covered/clear
//! state. GM roles only (`authorize`). The core checks shapes and limits; drawing is the client's.

use dnd_protocol::{
    Event, FogOp, FogShape, Intent, MAX_BRUSH_POINTS, MAX_BRUSH_RADIUS, MAX_FOG_OPS,
    MAX_FOG_POINTS, MapDto, MapId, RejectReason, UNITS16_PER_UNIT,
};

use crate::state::GameState;

pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    match intent {
        Intent::ApplyFog { map_id, op } => Some(apply_fog(state, *map_id, op)),
        Intent::ResetFog { map_id, covered } => Some(if state.maps.contains_key(map_id) {
            Ok(vec![Event::FogReset {
                map_id: *map_id,
                covered: *covered,
            }])
        } else {
            Err(RejectReason::NotFound)
        }),
        _ => None,
    }
}

fn apply_fog(state: &GameState, map_id: MapId, op: &FogOp) -> Result<Vec<Event>, RejectReason> {
    let map = state.maps.get(&map_id).ok_or(RejectReason::NotFound)?;
    valid_shape(map, &op.shape)?;
    if let Some(fog) = state.fog.get(&map_id) {
        if fog.ops.len() >= MAX_FOG_OPS {
            return Err(limit("fog_ops"));
        }
        if points(&fog.ops) + shape_points(&op.shape) > MAX_FOG_POINTS {
            return Err(limit("fog_points"));
        }
    }
    Ok(vec![Event::FogApplied {
        map_id,
        op: op.clone(),
    }])
}

fn limit(name: &str) -> RejectReason {
    RejectReason::LimitExceeded {
        limit: name.to_owned(),
    }
}

/// Brush points stored for these operations.
pub(crate) fn points(ops: &[FogOp]) -> usize {
    ops.iter().map(|op| shape_points(&op.shape)).sum()
}

fn shape_points(shape: &FogShape) -> usize {
    match shape {
        FogShape::Rect { .. } => 0,
        FogShape::Brush { points, .. } => points.len() / 2,
    }
}

/// The shape lies on the map (its bounds in Units16, edges included) and is not degenerate.
fn valid_shape(map: &MapDto, shape: &FogShape) -> Result<(), RejectReason> {
    let max_x = map.width.saturating_mul(UNITS16_PER_UNIT);
    let max_y = map.height.saturating_mul(UNITS16_PER_UNIT);
    match shape {
        FogShape::Rect {
            x,
            y,
            width,
            height,
        } => {
            let inside = |pos: i64, len: i64, max: i64| {
                pos >= 0 && len > 0 && pos.checked_add(len).is_some_and(|end| end <= max)
            };
            if !inside(*x, *width, max_x) {
                return Err(RejectReason::invalid("op.shape.x"));
            }
            if !inside(*y, *height, max_y) {
                return Err(RejectReason::invalid("op.shape.y"));
            }
        }
        FogShape::Brush { radius, points } => {
            if !(1..=MAX_BRUSH_RADIUS).contains(radius) {
                return Err(RejectReason::invalid("op.shape.radius"));
            }
            if points.is_empty() || points.len() % 2 != 0 || points.len() / 2 > MAX_BRUSH_POINTS {
                return Err(RejectReason::invalid("op.shape.points"));
            }
            let on_map = points
                .as_chunks::<2>()
                .0
                .iter()
                .all(|[x, y]| (0..=max_x).contains(x) && (0..=max_y).contains(y));
            if !on_map {
                return Err(RejectReason::invalid("op.shape.points"));
            }
        }
    }
    Ok(())
}
