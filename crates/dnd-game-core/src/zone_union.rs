//! Union of zone parts (ADR-0024): rectangles and axis-aligned polygons that touch along an edge
//! or overlap become one outline; parts that do not, and slanted polygons, stay separate.
//!
//! Integer-only and deterministic: the part corners cut the plane into a grid of cells, a cell is
//! filled when any part covers it, edge-connected filled cells form one piece, and the border of a
//! piece is traced into a rectangle or a polygon. A piece whose border is not one loop (a hole,
//! or cells meeting only at a corner) or has more than [`MAX_ZONE_POINTS`] corners keeps its
//! original parts.

use std::collections::BTreeMap;

use dnd_protocol::{MAX_ZONE_POINTS, ZoneShape};

/// Most grid cells the union looks at; beyond that the parts are kept as they are.
const MAX_CELLS: usize = 4_000_000;

/// The parts with touching or overlapping axis-aligned ones joined, in the order of the first part
/// of each piece.
pub(crate) fn merge(parts: Vec<ZoneShape>) -> Vec<ZoneShape> {
    // One bit per part marks who covers a cell.
    if parts.len() > 64 {
        return parts;
    }
    let loops: Vec<Option<Vec<(i64, i64)>>> = parts.iter().map(rectilinear).collect();
    let axis = |pick: fn(&(i64, i64)) -> i64| {
        let mut values: Vec<i64> = loops.iter().flatten().flatten().map(pick).collect();
        values.sort_unstable();
        values.dedup();
        values
    };
    let xs = axis(|p| p.0);
    let ys = axis(|p| p.1);
    let cols = xs.len().saturating_sub(1);
    let rows = ys.len().saturating_sub(1);
    if cols.saturating_mul(rows) > MAX_CELLS {
        return parts;
    }
    let grid = Grid { cols, rows };

    // Fill: scan each row of cells, pairing the vertical edges that span it (even-odd rule, as in
    // `ZoneShape::contains`).
    let index = |values: &[i64], v: i64| values.binary_search(&v).unwrap_or_else(|i| i);
    let mut owners = vec![0_u64; cols * rows];
    for (k, corners) in loops.iter().enumerate() {
        let Some(corners) = corners else { continue };
        let verticals: Vec<(i64, i64, i64)> = corners
            .iter()
            .zip(corners.iter().cycle().skip(1))
            .filter(|(a, b)| a.0 == b.0 && a.1 != b.1)
            .map(|(a, b)| (a.0, a.1.min(b.1), a.1.max(b.1)))
            .collect();
        for (r, band) in ys.windows(2).enumerate() {
            let &[top, bottom] = band else { continue };
            let mut crossings: Vec<i64> = verticals
                .iter()
                .filter(|&&(_, y0, y1)| y0 <= top && y1 >= bottom)
                .map(|&(x, _, _)| x)
                .collect();
            crossings.sort_unstable();
            for &[left, right] in crossings.as_chunks::<2>().0 {
                for c in index(&xs, left)..index(&xs, right) {
                    if let Some(cell) = owners.get_mut(r * cols + c) {
                        *cell |= 1 << k;
                    }
                }
            }
        }
    }

    // Pieces: edge-connected filled cells, numbered in row-major order of their first cell.
    let mut piece_of = vec![usize::MAX; cols * rows];
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    for start in 0..owners.len() {
        if owners[start] == 0 || piece_of[start] != usize::MAX {
            continue;
        }
        let id = pieces.len();
        let mut cells = vec![start];
        piece_of[start] = id;
        let mut next = 0;
        while let Some(&cell) = cells.get(next) {
            next += 1;
            for neighbour in grid.neighbours(cell) {
                if owners[neighbour] != 0 && piece_of[neighbour] == usize::MAX {
                    piece_of[neighbour] = id;
                    cells.push(neighbour);
                }
            }
        }
        pieces.push(cells);
    }
    let owners_of: Vec<u64> = pieces
        .iter()
        .map(|cells| cells.iter().fold(0, |mask, &c| mask | owners[c]))
        .collect();
    let outlines: Vec<Option<ZoneShape>> = pieces
        .iter()
        .enumerate()
        .map(|(id, cells)| outline(&grid, &piece_of, id, cells, &xs, &ys))
        .collect();

    let mut out = Vec::new();
    let mut piece_done = vec![false; pieces.len()];
    let mut part_done = vec![false; parts.len()];
    for (k, part) in parts.iter().enumerate() {
        let covers: Vec<usize> = (0..pieces.len())
            .filter(|&id| owners_of[id] & (1 << k) != 0)
            .collect();
        if loops[k].is_none() || covers.is_empty() {
            // Slanted, or covering no cell (no area): kept as drawn.
            out.push(part.clone());
            part_done[k] = true;
            continue;
        }
        for id in covers {
            if std::mem::replace(&mut piece_done[id], true) {
                continue;
            }
            match &outlines[id] {
                Some(shape) => out.push(shape.clone()),
                None => {
                    for (j, original) in parts.iter().enumerate() {
                        if owners_of[id] & (1 << j) != 0
                            && !std::mem::replace(&mut part_done[j], true)
                        {
                            out.push(original.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

/// The corners of a rectangle or of a polygon whose edges are all horizontal or vertical.
fn rectilinear(shape: &ZoneShape) -> Option<Vec<(i64, i64)>> {
    match shape {
        ZoneShape::Rect {
            x,
            y,
            width,
            height,
        } => {
            let (right, bottom) = (x.saturating_add(*width), y.saturating_add(*height));
            Some(vec![(*x, *y), (right, *y), (right, bottom), (*x, bottom)])
        }
        ZoneShape::Polygon { points } => {
            let corners: Vec<(i64, i64)> = points
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[x, y]| (x, y))
                .collect();
            corners
                .iter()
                .zip(corners.iter().cycle().skip(1))
                .all(|(a, b)| a.0 == b.0 || a.1 == b.1)
                .then_some(corners)
        }
        ZoneShape::Union { .. } => None,
    }
}

/// Cells of the compressed grid, row by row.
struct Grid {
    cols: usize,
    rows: usize,
}

impl Grid {
    fn neighbours(&self, cell: usize) -> impl Iterator<Item = usize> {
        let (c, r) = (cell % self.cols, cell / self.cols);
        let left = c.checked_sub(1).map(|c| (c, r));
        let up = r.checked_sub(1).map(|r| (c, r));
        let right = (c + 1 < self.cols).then_some((c + 1, r));
        let down = (r + 1 < self.rows).then_some((c, r + 1));
        let cols = self.cols;
        [left, up, right, down]
            .into_iter()
            .flatten()
            .map(move |(c, r)| r * cols + c)
    }
}

/// The border of one piece as a rectangle or a polygon (clockwise on screen, y down); `None` when
/// it is not a single loop or has too many corners.
fn outline(
    grid: &Grid,
    piece_of: &[usize],
    id: usize,
    cells: &[usize],
    xs: &[i64],
    ys: &[i64],
) -> Option<ZoneShape> {
    let inside = |c: Option<usize>, r: Option<usize>| match (c, r) {
        (Some(c), Some(r)) if c < grid.cols && r < grid.rows => {
            piece_of.get(r * grid.cols + c) == Some(&id)
        }
        _ => false,
    };
    // Directed border edges between grid corners (column, row), the piece on their right.
    let mut outgoing: BTreeMap<(usize, usize), Vec<(usize, usize)>> = BTreeMap::new();
    let mut edges = 0_usize;
    for &cell in cells {
        let (c, r) = (cell % grid.cols, cell / grid.cols);
        let sides = [
            (inside(Some(c), r.checked_sub(1)), (c, r), (c + 1, r)),
            (inside(Some(c + 1), Some(r)), (c + 1, r), (c + 1, r + 1)),
            (inside(Some(c), Some(r + 1)), (c + 1, r + 1), (c, r + 1)),
            (inside(c.checked_sub(1), Some(r)), (c, r + 1), (c, r)),
        ];
        for (neighbour_inside, from, to) in sides {
            if !neighbour_inside {
                outgoing.entry(from).or_default().push(to);
                edges += 1;
            }
        }
    }

    // Walk the border, turning right (into the piece) where two loops meet at a corner.
    let step = |from: (usize, usize), to: (usize, usize)| {
        (
            to.0.cast_signed() - from.0.cast_signed(),
            to.1.cast_signed() - from.1.cast_signed(),
        )
    };
    // From the topmost, then leftmost corner.
    let (&start, targets) = outgoing.iter().min_by_key(|&(&(c, r), _)| (r, c))?;
    let first = *targets.first()?;
    let mut path = vec![start];
    let (mut at, mut heading) = (first, step(start, first));
    while path.len() <= edges {
        let options = outgoing.get(&at)?;
        let (dx, dy) = heading;
        let next = [(-dy, dx), (dx, dy), (dy, -dx)]
            .into_iter()
            .find_map(|turn| options.iter().find(|&&to| step(at, to) == turn))?;
        path.push(at);
        if at == start && *next == first {
            break;
        }
        heading = step(at, *next);
        at = *next;
    }
    // The loop closed back at the start: drop the repeated corner.
    path.pop();
    if path.len() != edges {
        return None;
    }

    // Corners only: drop points in the middle of straight runs.
    let n = path.len();
    let corners: Vec<(i64, i64)> = (0..n)
        .filter(|&i| {
            let prev = path[(i + n - 1) % n];
            let next = path[(i + 1) % n];
            step(prev, path[i]) != step(path[i], next)
        })
        .map(|i| (xs[path[i].0], ys[path[i].1]))
        .collect();
    if corners.len() == 4 {
        let (x0, y0) = corners.iter().copied().min()?;
        let (x1, y1) = corners.iter().copied().max()?;
        return Some(ZoneShape::Rect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        });
    }
    (corners.len() <= MAX_ZONE_POINTS).then(|| ZoneShape::Polygon {
        points: corners.into_iter().flat_map(|(x, y)| [x, y]).collect(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use super::*;
    use proptest::prelude::*;

    fn rect(x: i64, y: i64, width: i64, height: i64) -> ZoneShape {
        ZoneShape::Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn covers(parts: &[ZoneShape], x: i64, y: i64) -> bool {
        parts.iter().any(|p| p.contains(x, y))
    }

    #[test]
    fn rectangles_sharing_an_edge_become_one() {
        assert_eq!(
            merge(vec![rect(0, 0, 10, 10), rect(10, 0, 5, 10)]),
            [rect(0, 0, 15, 10)]
        );
    }

    #[test]
    fn overlapping_rectangles_become_one_outline() {
        // A plus sign: 12 corners.
        let merged = merge(vec![rect(10, 0, 10, 30), rect(0, 10, 30, 10)]);
        assert_eq!(
            merged,
            [ZoneShape::Polygon {
                points: vec![
                    10, 0, 20, 0, 20, 10, 30, 10, 30, 20, 20, 20, 20, 30, 10, 30, 10, 20, 0, 20, 0,
                    10, 10, 10
                ]
            }]
        );
        assert!(merged[0].contains(15, 15) && merged[0].contains(25, 15));
        assert!(!merged[0].contains(5, 5) && !merged[0].contains(25, 25));
    }

    #[test]
    fn apart_or_at_a_corner_they_stay_separate() {
        let apart = vec![rect(0, 0, 10, 10), rect(20, 0, 10, 10)];
        assert_eq!(merge(apart.clone()), apart);
        let corner = vec![rect(0, 0, 10, 10), rect(10, 10, 10, 10)];
        assert_eq!(merge(corner.clone()), corner);
    }

    #[test]
    fn a_ring_with_a_hole_keeps_its_parts() {
        let ring = vec![
            rect(0, 0, 30, 10),
            rect(0, 20, 30, 10),
            rect(0, 10, 10, 10),
            rect(20, 10, 10, 10),
        ];
        assert_eq!(merge(ring.clone()), ring);
    }

    #[test]
    fn merged_outlines_merge_again_and_slanted_polygons_stay() {
        let l = merge(vec![rect(0, 0, 20, 10), rect(0, 10, 10, 10)]);
        assert_eq!(l.len(), 1);
        let triangle = ZoneShape::Polygon {
            points: vec![100, 100, 120, 100, 100, 120],
        };
        let mut parts = l;
        parts.push(rect(10, 10, 10, 10));
        parts.push(triangle.clone());
        assert_eq!(merge(parts), [rect(0, 0, 20, 20), triangle]);
    }

    proptest! {
        /// The union covers exactly the points the parts cover, borders included.
        #[test]
        fn covers_what_the_parts_cover(
            raw in prop::collection::vec((0_i64..12, 0_i64..12, 1_i64..8, 1_i64..8), 1..7)
        ) {
            let parts: Vec<ZoneShape> =
                raw.iter().map(|&(x, y, w, h)| rect(x * 2, y * 2, w * 2, h * 2)).collect();
            let merged = merge(parts.clone());
            prop_assert!(merged.len() <= parts.len());
            for x in -1..=42 {
                for y in -1..=42 {
                    prop_assert_eq!(covers(&merged, x, y), covers(&parts, x, y), "({}, {})", x, y);
                }
            }
            // Deterministic.
            prop_assert_eq!(merge(parts), merged);
        }
    }
}
