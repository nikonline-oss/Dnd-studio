import { describe, expect, it } from 'vitest';
import {
  clampLabel,
  dragHandle,
  handleAt,
  labelPosition,
  moveShape,
  shapeContains,
  zoneAt,
} from './zoneEdit';
import type { ZoneDto, ZoneShape } from '@/shared/ipc/client';

const map = { width: 1600, height: 1000 };
const rect = (x: number, y: number, width: number, height: number): ZoneShape => ({
  type: 'rect',
  x,
  y,
  width,
  height,
});
const zone = (id: string, shape: ZoneShape, label: ZoneDto['label'] = null): ZoneDto => ({
  id,
  map_id: 'm',
  name: id,
  shape,
  label,
  version: 1,
});

describe('zone edit geometry', () => {
  it('hit-tests like the core: borders inside, the topmost zone wins', () => {
    const l: ZoneShape = {
      type: 'polygon',
      points: [0, 0, 50, 0, 50, 50, 100, 50, 100, 100, 0, 100],
    };
    expect(shapeContains(l, 25, 25) && shapeContains(l, 100, 75) && shapeContains(l, 75, 50)).toBe(
      true,
    );
    expect(shapeContains(l, 75, 25) || shapeContains(l, 101, 75)).toBe(false);
    const zones = [zone('a', rect(0, 0, 100, 100)), zone('b', rect(50, 50, 100, 100))];
    expect(zoneAt(zones, { x: 60, y: 60 })?.id).toBe('b');
    expect(zoneAt(zones, { x: 10, y: 10 })?.id).toBe('a');
    expect(zoneAt(zones, { x: 500, y: 500 })).toBeNull();
  });

  it('moves every part together and keeps the zone on the map', () => {
    const shape: ZoneShape = {
      type: 'union',
      parts: [rect(0, 0, 100, 100), rect(200, 0, 100, 100)],
    };
    expect(moveShape(shape, 10.4, 20, map)).toEqual({
      shape: { type: 'union', parts: [rect(10, 20, 100, 100), rect(210, 20, 100, 100)] },
      dx: 10,
      dy: 20,
    });
    // Stopped at the map's edges.
    expect(moveShape(shape, -50, 5000, map)).toMatchObject({ dx: 0, dy: 900 });
  });

  it('resizes a rectangle by a corner, keeping the opposite one', () => {
    const shape = rect(100, 100, 200, 100);
    const corner = handleAt(shape, { x: 302, y: 198 }, 8);
    expect(corner).toMatchObject({ part: 0, index: 2 });
    if (!corner) return;
    expect(dragHandle(shape, corner, { x: 400, y: 300 }, map)).toEqual(rect(100, 100, 300, 200));
    // Dragged past the opposite corner: the rectangle flips.
    expect(dragHandle(shape, corner, { x: 50, y: 50 }, map)).toEqual(rect(50, 50, 50, 50));
    // Less than a unit wide: refused.
    expect(dragHandle(shape, corner, { x: 105, y: 300 }, map)).toBe(shape);
    // Clamped to the map.
    expect(dragHandle(shape, corner, { x: 9000, y: 300 }, map)).toEqual(rect(100, 100, 1500, 200));
  });

  it('moves one vertex of a polygon part of a union', () => {
    const triangle: ZoneShape = { type: 'polygon', points: [0, 0, 100, 0, 0, 100] };
    const shape: ZoneShape = { type: 'union', parts: [rect(500, 500, 10, 10), triangle] };
    const vertex = handleAt(shape, { x: 99, y: 1 }, 4);
    expect(vertex).toMatchObject({ part: 1, index: 1 });
    if (!vertex) return;
    expect(dragHandle(shape, vertex, { x: 150, y: 20 }, map)).toEqual({
      type: 'union',
      parts: [rect(500, 500, 10, 10), { type: 'polygon', points: [0, 0, 150, 20, 0, 100] }],
    });
  });

  it('puts the name at the first corner until the GM moves it', () => {
    expect(labelPosition(zone('a', rect(100, 200, 50, 50)))).toEqual({ x: 164, y: 248 });
    expect(labelPosition(zone('a', rect(100, 200, 50, 50), { x: 5, y: 6 }))).toEqual({
      x: 5,
      y: 6,
    });
    expect(clampLabel({ x: -3, y: 2000.6 }, map)).toEqual({ x: 0, y: 1000 });
  });
});
