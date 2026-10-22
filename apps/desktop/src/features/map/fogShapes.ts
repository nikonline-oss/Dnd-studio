// Turning pointer input into fog shapes (ADR-0019). Pure: map units in, Units16 shapes out.
import { toUnits16, type Point, type Size } from './viewport';
import type { FogShape } from '@/shared/ipc/client';

/** Mirrors `MAX_BRUSH_POINTS` / `MAX_BRUSH_RADIUS` of the protocol (checked again by the core). */
export const MAX_BRUSH_POINTS = 512;
export const MAX_BRUSH_RADIUS = 256;
export const MIN_BRUSH_RADIUS = 4;

const clamp = (value: number, max: number) => Math.min(max, Math.max(0, value));

/** Clamps a map point (map units) onto the map and converts it to Units16. */
export function toMap16(p: Point, map: Size): [number, number] {
  return [toUnits16(clamp(p.x, map.width)), toUnits16(clamp(p.y, map.height))];
}

/** The rectangle spanned by a drag from `a` to `b`, clipped to the map; `null` if empty. */
export function rectFromDrag(a: Point, b: Point, map: Size): FogShape | null {
  const [x1, y1] = toMap16(a, map);
  const [x2, y2] = toMap16(b, map);
  const x = Math.min(x1, x2);
  const y = Math.min(y1, y2);
  const width = Math.abs(x2 - x1);
  const height = Math.abs(y2 - y1);
  if (width === 0 || height === 0) return null;
  return { type: 'rect', x, y, width, height };
}

/**
 * Collects brush points in Units16. A point closer than half the radius to the previous one adds
 * nothing visible and is dropped, so a slow drag does not flood the stroke.
 */
export class BrushStroke {
  readonly points: number[] = [];

  constructor(
    readonly radius16: number,
    private readonly map: Size,
  ) {}

  /** Adds a point; returns whether it was kept. */
  add(p: Point): boolean {
    const [x, y] = toMap16(p, this.map);
    const n = this.points.length;
    if (n >= 2) {
      const dx = x - (this.points[n - 2] ?? x);
      const dy = y - (this.points[n - 1] ?? y);
      if (Math.hypot(dx, dy) < this.radius16 / 2) return false;
    }
    this.points.push(x, y);
    return true;
  }

  /**
   * The stroke as brush shapes of at most `MAX_BRUSH_POINTS` points. Consecutive shapes share
   * their joining point, so the stroke stays continuous.
   */
  shapes(): FogShape[] {
    const count = this.points.length / 2;
    if (count === 0) return [];
    const shapes: FogShape[] = [];
    for (let start = 0; start < count; start += MAX_BRUSH_POINTS - 1) {
      const end = Math.min(count, start + MAX_BRUSH_POINTS);
      shapes.push({
        type: 'brush',
        radius: this.radius16,
        points: this.points.slice(start * 2, end * 2),
      });
      if (end === count) break;
    }
    return shapes;
  }
}

/** Brush radius in map units → Units16, within the protocol bounds. */
export function brushRadius16(radius: number): number {
  return toUnits16(Math.min(MAX_BRUSH_RADIUS, Math.max(MIN_BRUSH_RADIUS, radius)));
}
