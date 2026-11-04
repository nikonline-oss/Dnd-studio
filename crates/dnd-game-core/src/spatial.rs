//! Deterministic, two-dimensional physical occlusion. Publication visibility is separate.
use crate::GameState;
use dnd_protocol::{MapId, PluginAction, TokenDto, ZoneShape};

type Point = (i64, i64);

fn cross(a: Point, b: Point, c: Point) -> i128 {
    (i128::from(b.0) - i128::from(a.0)) * (i128::from(c.1) - i128::from(a.1))
        - (i128::from(b.1) - i128::from(a.1)) * (i128::from(c.0) - i128::from(a.0))
}
fn on_segment(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p) == 0
        && (a.0.min(b.0)..=a.0.max(b.0)).contains(&p.0)
        && (a.1.min(b.1)..=a.1.max(b.1)).contains(&p.1)
}
fn intersects(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (ac, ad, ca, cb) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (ac.signum() != ad.signum()
        && ca.signum() != cb.signum()
        && ac != 0
        && ad != 0
        && ca != 0
        && cb != 0)
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}
fn crosses(shape: &ZoneShape, a: Point, b: Point) -> bool {
    if shape.contains(a.0, a.1) || shape.contains(b.0, b.1) {
        return true;
    }
    match shape {
        ZoneShape::Rect {
            x,
            y,
            width,
            height,
        } => {
            let right = x.saturating_add(*width);
            let bottom = y.saturating_add(*height);
            let corners = [(*x, *y), (right, *y), (right, bottom), (*x, bottom)];
            edges_cross(&corners, a, b)
        }
        ZoneShape::Polygon { points } => {
            let corners: Vec<_> = points
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[x, y]| (x, y))
                .collect();
            edges_cross(&corners, a, b)
        }
        ZoneShape::Union { parts } => parts.iter().any(|part| crosses(part, a, b)),
    }
}
fn edges_cross(corners: &[Point], a: Point, b: Point) -> bool {
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .any(|(&c, &d)| intersects(a, b, c, d))
}
fn clear(state: &GameState, map: MapId, a: Point, b: Point, sight: bool) -> bool {
    !state.zones.values().any(|zone| {
        zone.map_id == map
            && if sight {
                zone.occlusion.sight
            } else {
                zone.occlusion.effect
            }
            && crosses(&zone.shape, a, b)
    })
}
pub(crate) fn permits(
    state: &GameState,
    action: &PluginAction,
    source: &TokenDto,
    target: Point,
) -> bool {
    let p = &action.spatial;
    if !crate::modifiers::feature_enabled(state, p.feature.as_deref()) {
        return true;
    }
    let origin = (source.x, source.y);
    (!p.sight || clear(state, source.map_id, origin, target, true))
        && (!p.line_of_effect || clear(state, source.map_id, origin, target, false))
}
pub(crate) fn area_permits(
    state: &GameState,
    action: &PluginAction,
    map: MapId,
    origin: Point,
    target: Point,
) -> bool {
    !action.spatial.area_line_of_effect
        || !crate::modifiers::feature_enabled(state, action.spatial.feature.as_deref())
        || clear(state, map, origin, target, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rays_use_exact_edges_including_tangency_and_endpoints() {
        let rect = ZoneShape::Rect {
            x: 10,
            y: 10,
            width: 10,
            height: 10,
        };
        assert!(crosses(&rect, (0, 15), (30, 15)));
        assert!(crosses(&rect, (0, 10), (30, 10)));
        assert!(crosses(&rect, (0, 0), (10, 10)));
        assert!(crosses(&rect, (15, 15), (15, 15)));
        assert!(!crosses(&rect, (0, 0), (30, 9)));
        assert!(!crosses(&rect, (0, 0), (0, 0)));
    }
    #[test]
    fn concave_polygons_and_unions_do_not_block_their_bounding_box() {
        let shape = ZoneShape::Polygon {
            points: vec![10, 10, 30, 10, 30, 20, 20, 20, 20, 30, 10, 30],
        };
        assert!(crosses(&shape, (0, 15), (40, 15)));
        assert!(!crosses(&shape, (25, 25), (40, 25)));
        let union = ZoneShape::Union { parts: vec![shape] };
        assert!(!crosses(&union, (25, 25), (40, 25)));
    }
    #[test]
    fn segment_symmetry_and_large_safe_coordinates() {
        let n = 9_007_199_254_740_000;
        assert!(intersects((0, 0), (n, n), (0, n), (n, 0)));
        for a in [(0, 0), (10, 10), (10, 30), (40, 40)] {
            for b in [(0, 40), (10, 10), (30, 10), (40, 0)] {
                assert_eq!(
                    intersects(a, b, (10, 10), (30, 10)),
                    intersects(b, a, (30, 10), (10, 10))
                );
            }
        }
    }
}
