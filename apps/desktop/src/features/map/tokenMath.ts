// Token geometry and helpers. Pure: map units (and Units16 for stored positions), no Pixi.
import { toUnits16, type Point, type Size } from './viewport';
import type { Grid } from '@/shared/ipc/client';

/** Token diameter on maps without a grid, in map units. */
export const DEFAULT_TOKEN_SIZE = 70;

/** Diameter of a token in map units: one grid cell times its scale. */
export function tokenDiameter(grid: Grid, scale: number): number {
  const cell = grid.kind === 'none' ? DEFAULT_TOKEN_SIZE : grid.cell_size;
  return cell * scale;
}

/**
 * The centre of the square cell containing `p`. Other grids (hex snapping is later) and no grid
 * leave the point as is.
 */
export function snapToGrid(p: Point, grid: Grid): Point {
  if (grid.kind !== 'square') return p;
  const cell = grid.cell_size;
  const snap = (value: number, offset: number) =>
    Math.floor((value - offset) / cell) * cell + offset + cell / 2;
  return { x: snap(p.x, grid.offset_x), y: snap(p.y, grid.offset_y) };
}

/** A map point (map units) clamped onto the map, as a stored token position (Units16). */
export function tokenPosition(p: Point, map: Size): { x: number; y: number } {
  const clamp = (value: number, max: number) => Math.min(max, Math.max(0, value));
  return { x: toUnits16(clamp(p.x, map.width)), y: toUnits16(clamp(p.y, map.height)) };
}

/**
 * Sends at most one value per `intervalMs` while dragging (ADR-0003: token moves are
 * throttled to 10–15 Hz); `flush` sends the last value right away (on drop).
 */
export class Throttle<T> {
  private pending: T | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private last = -Infinity;

  constructor(
    private readonly intervalMs: number,
    private readonly send: (value: T) => void,
    private readonly now: () => number = () => performance.now(),
  ) {}

  push(value: T): void {
    this.pending = value;
    const wait = this.last + this.intervalMs - this.now();
    if (wait <= 0) {
      this.fire();
    } else {
      this.timer ??= setTimeout(() => {
        this.timer = null;
        this.fire();
      }, wait);
    }
  }

  /** Sends the pending value now (if any) and stops the timer. */
  flush(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.fire();
  }

  /** Drops the pending value. */
  cancel(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.pending = null;
  }

  private fire(): void {
    if (this.pending === null) return;
    const value = this.pending;
    this.pending = null;
    this.last = this.now();
    this.send(value);
  }
}
