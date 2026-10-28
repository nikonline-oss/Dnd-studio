//! Zone intents (ADR-0024): named areas on maps that triggers watch. GM roles only (checked in
//! `permissions::authorize`); players never see zones (`visibility`).

use dnd_protocol::{
    Event, Intent, MAX_ZONE_PARTS, MAX_ZONE_POINTS, MAX_ZONES_PER_MAP, MapDto, RejectReason,
    TriggerEvent, UNITS16_PER_UNIT, Version, ZoneDto, ZoneShape,
};

use crate::apply::{bump, valid_name};
use crate::state::GameState;
use crate::zone_union;

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
        Intent::MergeZones { into, from } => (|| {
            let mut zone = state
                .zones
                .get(into)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            let mut merged = Vec::with_capacity(from.len());
            for id in from {
                let other = state.zones.get(id).ok_or(RejectReason::NotFound)?;
                if *id == *into || other.map_id != zone.map_id || merged.contains(id) {
                    return Err(RejectReason::invalid("from"));
                }
                merged.push(*id);
            }
            if merged.is_empty() {
                return Err(RejectReason::invalid("from"));
            }
            let parts: Vec<ZoneShape> = std::iter::once(&zone)
                .chain(merged.iter().filter_map(|id| state.zones.get(id)))
                .flat_map(|z| z.shape.parts())
                .collect();
            let map = state.maps.get(&zone.map_id).ok_or(RejectReason::NotFound)?;
            zone.shape = valid_shape(map, &ZoneShape::of_parts(parts))?;
            zone.version = bump(zone.version)?;
            let mut events = vec![Event::ZoneUpdated { zone }];
            // Triggers watching a merged zone watch the result.
            for trigger in state.triggers.values() {
                let mut next = trigger.clone();
                let watched = match &mut next.definition.event {
                    TriggerEvent::RollInZone { zone, .. }
                    | TriggerEvent::TokenEnteredZone { zone } => Some(zone),
                    TriggerEvent::Manual | TriggerEvent::ItemReceived { .. } => None,
                };
                if let Some(zone) = watched
                    && merged.contains(zone)
                {
                    *zone = *into;
                    next.version = bump(next.version)?;
                    events.push(Event::TriggerUpdated { trigger: next });
                }
            }
            events.extend(merged.into_iter().map(|id| Event::ZoneDeleted { id }));
            Ok(events)
        })(),
        _ => return None,
    };
    Some(result)
}

/// A rectangle with a positive size or a polygon of 3..=64 corners, all on the map; or a union of
/// up to 32 of them (nested unions are flattened, touching or overlapping axis-aligned parts
/// joined into one outline).
fn valid_shape(map: &MapDto, shape: &ZoneShape) -> Result<ZoneShape, RejectReason> {
    if let ZoneShape::Union { .. } = shape {
        let parts = shape.parts();
        if parts.is_empty() || parts.len() > MAX_ZONE_PARTS {
            return Err(RejectReason::invalid("shape"));
        }
        let parts = parts
            .iter()
            .map(|p| valid_shape(map, p))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(ZoneShape::of_parts(zone_union::merge(parts)));
    }
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
        // Flattened above.
        ZoneShape::Union { .. } => return Err(invalid()),
    }
    Ok(shape.clone())
}
