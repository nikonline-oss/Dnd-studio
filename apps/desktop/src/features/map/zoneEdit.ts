// Geometry of the GM's zone edit mode (ADR-0024): hit tests, moving a zone, dragging the corner
// of a rectangle part or the vertex of a polygon part, the name label. All in Units16 (integers).
import { zoneParts } from './zoneTool';
import type { ZoneDto, ZoneLabel, ZoneShape } from '@/shared/ipc/client';

type Part = Exclude<ZoneShape, { type: 'union' }>;
export interface Point16 {
  x: number;
  y: number;
}
/** The map size in Units16. */
export interface Size16 {
  width: number;
  height: number;
}

/** Where a label goes when the GM has not moved it: near the first part's corner. */
const LABEL_OFFSET = { x: 64, y: 48 };
/** A resized rectangle is at least one map unit wide and high. */
const MIN_SIDE = 16;

/** A grabbable point of a zone: a rectangle's corner (0..3 clockwise) or a polygon's vertex. */
export interface Handle {
  part: number;
  index: number;
  x: number;
  y: number;
}

const clamp = (v: number, min: number, max: number) => Math.min(Math.max(v, min), max);

/** Whether a part covers the point, borders included (mirrors `ZoneShape::contains`). */
function partContains(part: Part, x: number, y: number): boolean {
  if (part.type === 'rect') {
    return x >= part.x && x <= part.x + part.width && y >= part.y && y <= part.y + part.height;
  }
  const p = part.points;
  const n = Math.floor(p.length / 2);
  let inside = false;
  for (let i = 0, j = n - 1; i < n; j = i++) {
    const xi = p[2 * i] ?? 0;
    const yi = p[2 * i + 1] ?? 0;
    const xj = p[2 * j] ?? 0;
    const yj = p[2 * j + 1] ?? 0;
    const onEdge =
      (xj - xi) * (y - yi) === (yj - yi) * (x - xi) &&
      x >= Math.min(xi, xj) &&
      x <= Math.max(xi, xj) &&
      y >= Math.min(yi, yj) &&
      y <= Math.max(yi, yj);
    if (onEdge) return true;
    if (yi > y !== yj > y && x < xi + ((y - yi) * (xj - xi)) / (yj - yi)) inside = !inside;
  }
  return inside;
}

export function shapeContains(shape: ZoneShape, x: number, y: number): boolean {
  return zoneParts(shape).some((part) => partContains(part, x, y));
}

/** The topmost (last drawn) zone under the point. */
export function zoneAt(zones: ZoneDto[], at: Point16): ZoneDto | null {
  for (let i = zones.length - 1; i >= 0; i--) {
    const zone = zones[i];
    if (zone && shapeContains(zone.shape, at.x, at.y)) return zone;
  }
  return null;
}

/** Where the zone's name is drawn (top-left of the text). */
export function labelPosition(zone: ZoneDto): Point16 {
  if (zone.label) return { x: zone.label.x, y: zone.label.y };
  const [first] = zoneParts(zone.shape);
  if (!first) return { x: 0, y: 0 };
  const corner =
    first.type === 'rect'
      ? { x: first.x, y: first.y }
      : { x: first.points[0] ?? 0, y: first.points[1] ?? 0 };
  return { x: corner.x + LABEL_OFFSET.x, y: corner.y + LABEL_OFFSET.y };
}

/** The corners of rectangle parts and the vertices of polygon parts. */
export function handlesOf(shape: ZoneShape): Handle[] {
  return zoneParts(shape).flatMap((part, partIndex) => {
    if (part.type === 'rect') {
      const { x, y, width: w, height: h } = part;
      return [
        [x, y],
        [x + w, y],
        [x + w, y + h],
        [x, y + h],
      ].map(([hx = 0, hy = 0], index) => ({ part: partIndex, index, x: hx, y: hy }));
    }
    const handles: Handle[] = [];
    for (let i = 0; i + 1 < part.points.length; i += 2) {
      handles.push({
        part: partIndex,
        index: i / 2,
        x: part.points[i] ?? 0,
        y: part.points[i + 1] ?? 0,
      });
    }
    return handles;
  });
}

/** The handle within `tolerance` of the point, nearest first. */
export function handleAt(shape: ZoneShape, at: Point16, tolerance: number): Handle | null {
  let best: Handle | null = null;
  let bestDistance = Infinity;
  for (const handle of handlesOf(shape)) {
    const distance = Math.max(Math.abs(handle.x - at.x), Math.abs(handle.y - at.y));
    if (distance <= tolerance && distance < bestDistance) {
      best = handle;
      bestDistance = distance;
    }
  }
  return best;
}

function withParts(shape: ZoneShape, parts: Part[]): ZoneShape {
  return shape.type === 'union' || parts.length !== 1 || !parts[0]
    ? { type: 'union', parts }
    : parts[0];
}

/** The shape moved by (`dx`, `dy`), kept on the map as a whole; and the delta actually used. */
export function moveShape(
  shape: ZoneShape,
  dx: number,
  dy: number,
  map: Size16,
): { shape: ZoneShape; dx: number; dy: number } {
  const parts = zoneParts(shape);
  const xs = parts.flatMap((p) => (p.type === 'rect' ? [p.x, p.x + p.width] : evens(p.points)));
  const ys = parts.flatMap((p) => (p.type === 'rect' ? [p.y, p.y + p.height] : odds(p.points)));
  const mx = clamp(Math.round(dx), 0 - Math.min(...xs), map.width - Math.max(...xs));
  const my = clamp(Math.round(dy), 0 - Math.min(...ys), map.height - Math.max(...ys));
  const moved = parts.map((p): Part =>
    p.type === 'rect'
      ? { ...p, x: p.x + mx, y: p.y + my }
      : { type: 'polygon', points: p.points.map((v, i) => v + (i % 2 === 0 ? mx : my)) },
  );
  return { shape: withParts(shape, moved), dx: mx, dy: my };
}

const evens = (points: number[]) => points.filter((_, i) => i % 2 === 0);
const odds = (points: number[]) => points.filter((_, i) => i % 2 === 1);

/**
 * The shape with `handle` dragged to `to` (on the map): a rectangle keeps the opposite corner, a
 * polygon moves that vertex. A rectangle smaller than one unit is refused (the shape is returned
 * unchanged).
 */
export function dragHandle(shape: ZoneShape, handle: Handle, to: Point16, map: Size16): ZoneShape {
  const x = clamp(Math.round(to.x), 0, map.width);
  const y = clamp(Math.round(to.y), 0, map.height);
  const parts = zoneParts(shape);
  const part = parts[handle.part];
  if (!part) return shape;
  let next: Part;
  if (part.type === 'rect') {
    const opposite = handlesOf(part)[(handle.index + 2) % 4];
    if (!opposite) return shape;
    const left = Math.min(x, opposite.x);
    const top = Math.min(y, opposite.y);
    const width = Math.abs(x - opposite.x);
    const height = Math.abs(y - opposite.y);
    if (width < MIN_SIDE || height < MIN_SIDE) return shape;
    next = { type: 'rect', x: left, y: top, width, height };
  } else {
    const points = [...part.points];
    points[handle.index * 2] = x;
    points[handle.index * 2 + 1] = y;
    next = { type: 'polygon', points };
  }
  return withParts(
    shape,
    parts.map((p, i) => (i === handle.part ? next : p)),
  );
}

/** A label position kept on the map. */
export function clampLabel(at: Point16, map: Size16): ZoneLabel {
  return {
    x: clamp(Math.round(at.x), 0, map.width),
    y: clamp(Math.round(at.y), 0, map.height),
  };
}
