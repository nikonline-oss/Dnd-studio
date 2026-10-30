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
    /// Several rectangles and polygons (not unions) taken together: a zone added to or merged.
    Union { parts: Vec<ZoneShape> },
}

/// Most corners of a polygon zone.
pub const MAX_ZONE_POINTS: usize = 64;
/// Most zones on one map.
pub const MAX_ZONES_PER_MAP: usize = 200;
/// Most parts of a union zone.
pub const MAX_ZONE_PARTS: usize = 32;

impl ZoneShape {
    /// Whether the point (`x`, `y`) lies inside, borders included (even-odd rule for polygons).
    /// Integer coordinates; products fit in `i128`.
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
                // On an edge: inside, as on a rectangle's border.
                let on_edge = corners.iter().zip(corners.iter().cycle().skip(1)).any(
                    |(&(ax, ay), &(bx, by))| {
                        (bx - ax) * (py - ay) == (by - ay) * (px - ax)
                            && (ax.min(bx)..=ax.max(bx)).contains(&px)
                            && (ay.min(by)..=ay.max(by)).contains(&py)
                    },
                );
                if on_edge {
                    return true;
                }
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
            Self::Union { parts } => parts.iter().any(|p| p.contains(x, y)),
        }
    }

    /// The rectangles and polygons the shape is made of (a union's parts; else itself).
    #[must_use]
    pub fn parts(&self) -> Vec<Self> {
        match self {
            Self::Union { parts } => parts.iter().flat_map(Self::parts).collect(),
            other => vec![other.clone()],
        }
    }

    /// One shape of all `parts`: the part itself when there is one, else their union.
    #[must_use]
    pub fn of_parts(mut parts: Vec<Self>) -> Self {
        if parts.len() == 1
            && let Some(only) = parts.pop()
        {
            return only;
        }
        Self::Union { parts }
    }
}

/// Where a zone's name is drawn: the top-left corner of the text, in Units16 (ADR-0016).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ZoneLabel {
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: i64,
}

/// A named zone on a map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ZoneDto {
    pub id: ZoneId,
    pub map_id: MapId,
    pub name: String,
    pub shape: ZoneShape,
    /// Where the GM moved the name to; `None` (and zones stored before): at the first part's corner.
    #[serde(default)]
    pub label: Option<ZoneLabel>,
    /// RGB, 0x000000..=0xffffff. Old campaigns use purple.
    #[serde(default = "default_zone_color")]
    pub color: u32,
    pub version: Version,
}

/// Default color of zones created before custom colors.
#[must_use]
pub const fn default_zone_color() -> u32 {
    0x9b7bff
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
    fn zones_stored_without_a_label_still_load() {
        let json = r#"{"id":"0190a000-0000-7000-8000-000000000001","map_id":"0190a000-0000-7000-8000-000000000002","name":"Altar","shape":{"type":"rect","x":0,"y":0,"width":16,"height":16},"version":3}"#;
        let zone: ZoneDto = serde_json::from_str(json).unwrap();
        assert_eq!(zone.label, None);
        assert_eq!(zone.color, default_zone_color());
    }

    #[test]
    fn polygons_use_the_even_odd_rule() {
        // An L shape: the notch at the top right is outside.
        let l = ZoneShape::Polygon {
            points: vec![0, 0, 50, 0, 50, 50, 100, 50, 100, 100, 0, 100],
        };
        assert!(l.contains(25, 25) && l.contains(75, 75));
        assert!(!l.contains(75, 25) && !l.contains(150, 75) && !l.contains(-1, 50));
        // Borders are inside, as with rectangles.
        assert!(
            l.contains(100, 75) && l.contains(75, 50) && l.contains(0, 0) && l.contains(50, 25)
        );
        assert!(!l.contains(101, 75));
    }
}
