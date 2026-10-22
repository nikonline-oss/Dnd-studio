// Grid as one repeating pattern tile for a `TilingSprite` (ADR-0017): the cost does not depend on
// the number of cells. Pure geometry in logical map units; the scene turns it into a texture.
import type { Grid } from '@/shared/ipc/client';

/** A line segment `[x1, y1, x2, y2]`. */
export type Segment = [number, number, number, number];

export interface GridPattern {
  /** Size of one period of the pattern. */
  width: number;
  height: number;
  segments: Segment[];
  /** Where the pattern starts (the grid offset, wrapped into one period). */
  originX: number;
  originY: number;
  lineWidth: number;
}

/** Mirrors `MIN_CELL_SIZE` / `MAX_CELL_SIZE` of the protocol (checked again by the core). */
export const MIN_CELL_SIZE = 8;
export const MAX_CELL_SIZE = 2048;

const SQRT3 = Math.sqrt(3);

/** Edges of a hexagon with circumradius `r` around (cx, cy). */
function hexagon(cx: number, cy: number, r: number, flat: boolean): Segment[] {
  const corners: [number, number][] = [];
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 3) * i + (flat ? 0 : Math.PI / 6);
    corners.push([cx + r * Math.cos(angle), cy + r * Math.sin(angle)]);
  }
  return corners.map(([x, y], i) => {
    const [nx, ny] = corners[(i + 1) % 6] ?? [x, y];
    return [x, y, nx, ny];
  });
}

const wrap = (value: number, period: number) => ((value % period) + period) % period;

/**
 * The repeating tile of a grid, or `null` for no grid. `cell_size` is the side of a square cell
 * or the distance between the centres of neighbouring hexes (flat-to-flat width).
 */
export function gridPattern(grid: Grid): GridPattern | null {
  if (grid.kind === 'none') return null;
  const cell = grid.cell_size;
  const lineWidth = Math.min(4, Math.max(1, cell / 35));
  if (grid.kind === 'square') {
    return {
      width: cell,
      height: cell,
      segments: [
        [0, 0, cell, 0],
        [0, 0, 0, cell],
      ],
      originX: wrap(grid.offset_x, cell),
      originY: wrap(grid.offset_y, cell),
      lineWidth,
    };
  }
  // One period of the hex lattice holds parts of four hexes; the texture clips them.
  const r = cell / SQRT3;
  const flat = grid.kind === 'hex_flat';
  const long = 3 * r;
  const width = flat ? long : cell;
  const height = flat ? cell : long;
  const centres: [number, number][] = flat
    ? [
        [0, cell / 2],
        [long, cell / 2],
        [long / 2, 0],
        [long / 2, cell],
      ]
    : [
        [cell / 2, 0],
        [cell / 2, long],
        [0, long / 2],
        [cell, long / 2],
      ];
  return {
    width,
    height,
    segments: centres.flatMap(([x, y]) => hexagon(x, y, r, flat)),
    originX: wrap(grid.offset_x, width),
    originY: wrap(grid.offset_y, height),
    lineWidth,
  };
}
