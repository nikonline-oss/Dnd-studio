import type { Grid } from '@/shared/ipc/client';
import type { Point, Size } from './viewport';

const SQRT3 = Math.sqrt(3);

/** Shortest route preview in map units. Grid routes include every traversed cell centre. */
export function shortestMovementPath(from: Point, to: Point, grid: Grid, map: Size): Point[] {
  if (grid.kind === 'none') return [clamp(from, map), clamp(to, map)];
  if (grid.kind === 'square') return squarePath(from, to, grid, map);
  return hexPath(from, to, grid, map);
}

function squarePath(from: Point, to: Point, grid: Grid, map: Size): Point[] {
  const cell = grid.cell_size;
  const countX = Math.max(1, Math.ceil(map.width / cell));
  const countY = Math.max(1, Math.ceil(map.height / cell));
  const index = (value: number, offset: number, count: number) =>
    Math.max(0, Math.min(count - 1, Math.floor((value - offset) / cell)));
  let x = index(from.x, grid.offset_x, countX);
  let y = index(from.y, grid.offset_y, countY);
  const endX = index(to.x, grid.offset_x, countX);
  const endY = index(to.y, grid.offset_y, countY);
  const path: Point[] = [squareCenter(x, y, grid, map)];
  while (x !== endX) {
    x += Math.sign(endX - x);
    path.push(squareCenter(x, y, grid, map));
  }
  while (y !== endY) {
    y += Math.sign(endY - y);
    path.push(squareCenter(x, y, grid, map));
  }
  return path;
}

function squareCenter(x: number, y: number, grid: Grid, map: Size): Point {
  const cell = grid.cell_size;
  return clamp({ x: grid.offset_x + (x + 0.5) * cell, y: grid.offset_y + (y + 0.5) * cell }, map);
}

type Axial = { q: number; r: number };

function hexPoint(cell: Axial, grid: Grid): Point {
  const size = grid.cell_size;
  if (grid.kind === 'hex_flat') {
    return {
      x: grid.offset_x + (SQRT3 / 2) * size * cell.q,
      y: grid.offset_y + size * (cell.r + cell.q / 2),
    };
  }
  return {
    x: grid.offset_x + size * (cell.q + cell.r / 2),
    y: grid.offset_y + (SQRT3 / 2) * size * cell.r,
  };
}

function nearestHex(point: Point, grid: Grid, map: Size): Axial {
  const size = grid.cell_size;
  const x = point.x - grid.offset_x;
  const y = point.y - grid.offset_y;
  const r = grid.kind === 'hex_flat' ? y / size - x / (SQRT3 * size) : (2 * y) / (SQRT3 * size);
  const q = grid.kind === 'hex_flat' ? (2 * x) / (SQRT3 * size) : x / size - r / 2;
  const baseQ = Math.round(q);
  const baseR = Math.round(r);
  let best: Axial = { q: baseQ, r: baseR };
  let distance = Infinity;
  for (let dq = -2; dq <= 2; dq++) {
    for (let dr = -2; dr <= 2; dr++) {
      const candidate = { q: baseQ + dq, r: baseR + dr };
      const center = hexPoint(candidate, grid);
      if (center.x < 0 || center.y < 0 || center.x > map.width || center.y > map.height) continue;
      const next = (center.x - point.x) ** 2 + (center.y - point.y) ** 2;
      if (next < distance) {
        best = candidate;
        distance = next;
      }
    }
  }
  return best;
}

function hexPath(from: Point, to: Point, grid: Grid, map: Size): Point[] {
  const start = nearestHex(clamp(from, map), grid, map);
  const end = nearestHex(clamp(to, map), grid, map);
  const dq = end.q - start.q;
  const dr = end.r - start.r;
  const distance = Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr));
  if (distance === 0) return [hexPoint(start, grid)];
  return Array.from({ length: distance + 1 }, (_, step) => {
    const progress = step / distance;
    const q = start.q + dq * progress;
    const r = start.r + dr * progress;
    const rounded = cubeRound(q, r);
    return clamp(hexPoint(rounded, grid), map);
  });
}

function cubeRound(q: number, r: number): Axial {
  let rq = Math.round(q);
  let rr = Math.round(r);
  const rs = Math.round(-q - r);
  const qError = Math.abs(rq - q);
  const rError = Math.abs(rr - r);
  const sError = Math.abs(rs + q + r);
  if (qError > rError && qError > sError) rq = -rr - rs;
  else if (rError > sError) rr = -rq - rs;
  return { q: rq, r: rr };
}

function clamp(point: Point, map: Size): Point {
  return {
    x: Math.max(0, Math.min(map.width, point.x)),
    y: Math.max(0, Math.min(map.height, point.y)),
  };
}
