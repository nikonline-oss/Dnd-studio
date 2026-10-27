//! Zones (ADR-0024): named areas on a map that triggers watch. GM roles only; players never see
//! them.

use serde::{Deserialize, Serialize};

use crate::ids::{MapId, Version, ZoneId};

/// The area of a zone, in Units16 (ADR-0016).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ZoneShape {
    /// Axis-aligned rectangle with its top-left corner at (`x`, `y`).
    Rect {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        width: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        height: i64,
    },
    /// A closed polygon: flat `[x0, y0, x1, y1, …]`, 3..=[`MAX_ZONE_POINTS`] points.
    Polygon {
        #[cfg_attr(feature = "specta", specta(type = Vec<specta_typescript::Number>))]
        points: Vec<i64>,
    },
}

/// Most corners of a polygon zone.
pub const MAX_ZONE_POINTS: usize = 64;
/// Most zones on one map.
pub const MAX_ZONES_PER_MAP: usize = 200;

impl ZoneShape {
    /// Whether the point (`x`, `y`) lies inside (borders included for rectangles; even-odd rule for
    /// polygons). Integer coordinates; products fit in `i128`.
    #[must_use]
    pub fn contains(&self, x: i64, y: i64) -> bool {
        match self {
            Self::Rect {
                x: left,
                y: top,
                width,
                height,
            } => {
                (*left..=left.saturating_add(*width)).contains(&x)
                    && (*top..=top.saturating_add(*height)).contains(&y)
            }
            Self::Polygon { points } => {
                let corners: Vec<(i128, i128)> = points
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&[x, y]| (i128::from(x), i128::from(y)))
                    .collect();
                let (px, py) = (i128::from(x), i128::from(y));
                let mut inside = false;
                let mut j = corners.len().wrapping_sub(1);
                for (i, &(xi, yi)) in corners.iter().enumerate() {
                    let Some(&(xj, yj)) = corners.get(j) else {
                        break;
                    };
                    // The edge crosses the horizontal line through the point, to its right.
                    if (yi > py) != (yj > py) {
                        // px < xi + (py - yi) * (xj - xi) / (yj - yi), without division.
                        let lhs = (px - xi) * (yj - yi);
                        let rhs = (py - yi) * (xj - xi);
                        if (yj > yi && lhs < rhs) || (yj < yi && lhs > rhs) {
                            inside = !inside;
                        }
                    }
                    j = i;
                }
                inside
            }
        }
    }
}

/// A named zone on a map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ZoneDto {
    pub id: ZoneId,
    pub map_id: MapId,
    pub name: String,
    pub shape: ZoneShape,
    pub version: Version,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_include_their_border() {
        let rect = ZoneShape::Rect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(rect.contains(10, 20) && rect.contains(110, 70) && rect.contains(60, 40));
        assert!(!rect.contains(9, 40) && !rect.contains(60, 71));
    }

    #[test]
    fn polygons_use_the_even_odd_rule() {
        // An L shape: the notch at the top right is outside.
        let l = ZoneShape::Polygon {
            points: vec![0, 0, 50, 0, 50, 50, 100, 50, 100, 100, 0, 100],
        };
        assert!(l.contains(25, 25) && l.contains(75, 75));
        assert!(!l.contains(75, 25) && !l.contains(150, 75) && !l.contains(-1, 50));
    }
}
