//! Zone intents (ADR-0024): named areas on maps that triggers watch. GM roles only (checked in
//! `permissions::authorize`); players never see zones (`visibility`).

use dnd_protocol::{
    Event, Intent, MAX_ZONE_POINTS, MAX_ZONES_PER_MAP, MapDto, RejectReason, UNITS16_PER_UNIT,
    Version, ZoneDto, ZoneShape,
};

use crate::apply::{bump, valid_name};
use crate::state::GameState;

/// Events for a zone intent; `None` if `intent` is not about zones.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateZone {
            id,
            map_id,
            name,
            shape,
        } => (|| {
            let map = state.maps.get(map_id).ok_or(RejectReason::NotFound)?;
            let zones = state.zones.values().filter(|z| z.map_id == *map_id).count();
            if zones >= MAX_ZONES_PER_MAP {
                return Err(RejectReason::invalid("map_id"));
            }
            Ok(vec![Event::ZoneCreated {
                zone: ZoneDto {
                    id: *id,
                    map_id: *map_id,
                    name: valid_name(name, "name")?,
                    shape: valid_shape(map, shape)?,
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateZone { id, name, shape } => (|| {
            if name.is_none() && shape.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            let mut zone = state.zones.get(id).cloned().ok_or(RejectReason::NotFound)?;
            if let Some(name) = name {
                zone.name = valid_name(name, "name")?;
            }
            if let Some(shape) = shape {
                let map = state.maps.get(&zone.map_id).ok_or(RejectReason::NotFound)?;
                zone.shape = valid_shape(map, shape)?;
            }
            zone.version = bump(zone.version)?;
            Ok(vec![Event::ZoneUpdated { zone }])
        })(),
        Intent::DeleteZone { id } => (|| {
            if !state.zones.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(vec![Event::ZoneDeleted { id: *id }])
        })(),
        _ => return None,
    };
    Some(result)
}

/// A rectangle with a positive size or a polygon of 3..=64 corners, all on the map.
fn valid_shape(map: &MapDto, shape: &ZoneShape) -> Result<ZoneShape, RejectReason> {
    let invalid = || RejectReason::invalid("shape");
    let width = map.width.saturating_mul(UNITS16_PER_UNIT);
    let height = map.height.saturating_mul(UNITS16_PER_UNIT);
    let on_map = |x: i64, y: i64| (0..=width).contains(&x) && (0..=height).contains(&y);
    match shape {
        ZoneShape::Rect {
            x,
            y,
            width: w,
            height: h,
        } => {
            if *w <= 0
                || *h <= 0
                || !on_map(*x, *y)
                || !on_map(x.saturating_add(*w), y.saturating_add(*h))
            {
                return Err(invalid());
            }
        }
        ZoneShape::Polygon { points } => {
            let corners = points.len() / 2;
            if points.len() % 2 != 0 || !(3..=MAX_ZONE_POINTS).contains(&corners) {
                return Err(invalid());
            }
            if !points.as_chunks::<2>().0.iter().all(|&[x, y]| on_map(x, y)) {
                return Err(invalid());
            }
        }
    }
    Ok(shape.clone())
}
