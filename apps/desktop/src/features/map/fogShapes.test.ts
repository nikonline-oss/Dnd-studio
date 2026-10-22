import { describe, expect, it } from 'vitest';
import {
  BrushStroke,
  brushRadius16,
  MAX_BRUSH_POINTS,
  MAX_BRUSH_RADIUS,
  rectFromDrag,
  toMap16,
} from './fogShapes';

const map = { width: 4000, height: 3000 };

describe('rectFromDrag', () => {
  it('normalizes a drag in any direction to Units16', () => {
    expect(rectFromDrag({ x: 100, y: 50 }, { x: 10, y: 5.5 }, map)).toEqual({
      type: 'rect',
      x: 160,
      y: 88,
      width: 1440,
      height: 712,
    });
  });

  it('clips to the map and drops empty rectangles', () => {
    expect(rectFromDrag({ x: -50, y: -50 }, { x: 5000, y: 10 }, map)).toEqual({
      type: 'rect',
      x: 0,
      y: 0,
      width: 4000 * 16,
      height: 160,
    });
    expect(rectFromDrag({ x: 10, y: 10 }, { x: 10, y: 400 }, map)).toBeNull();
    // Entirely off the map: clipped to a line, so nothing.
    expect(rectFromDrag({ x: -10, y: 5 }, { x: -1, y: 50 }, map)).toBeNull();
  });
});

describe('BrushStroke', () => {
  it('drops points closer than half the radius', () => {
    const stroke = new BrushStroke(brushRadius16(20), map);
    expect(stroke.add({ x: 100, y: 100 })).toBe(true);
    expect(stroke.add({ x: 105, y: 100 })).toBe(false);
    expect(stroke.add({ x: 111, y: 100 })).toBe(true);
    expect(stroke.points).toEqual([1600, 1600, 1776, 1600]);
  });

  it('clamps points onto the map', () => {
    const stroke = new BrushStroke(brushRadius16(10), map);
    stroke.add({ x: -20, y: 3100 });
    expect(stroke.points).toEqual([0, 3000 * 16]);
    expect(toMap16({ x: 4001, y: -1 }, map)).toEqual([4000 * 16, 0]);
  });

  it('splits long strokes into connected shapes within the point limit', () => {
    const stroke = new BrushStroke(brushRadius16(4), map);
    const total = MAX_BRUSH_POINTS * 2 + 10;
    for (let i = 0; i < total; i++) stroke.add({ x: i * 3, y: 0 });
    const shapes = stroke.shapes();
    expect(shapes).toHaveLength(3);
    const lengths = shapes.map((s) => (s.type === 'brush' ? s.points.length / 2 : 0));
    expect(Math.max(...lengths)).toBeLessThanOrEqual(MAX_BRUSH_POINTS);
    // Each shape starts where the previous one ended.
    for (let i = 1; i < shapes.length; i++) {
      const prev = shapes[i - 1];
      const next = shapes[i];
      if (prev?.type !== 'brush' || next?.type !== 'brush') throw new Error('brush expected');
      expect(next.points.slice(0, 2)).toEqual(prev.points.slice(-2));
    }
    // Shared points counted once, all points are covered.
    expect(lengths.reduce((a, b) => a + b, 0) - (shapes.length - 1)).toBe(total);
  });

  it('a click without movement is a single dot', () => {
    const stroke = new BrushStroke(brushRadius16(30), map);
    stroke.add({ x: 10, y: 10 });
    expect(stroke.shapes()).toEqual([{ type: 'brush', radius: 480, points: [160, 160] }]);
    expect(new BrushStroke(480, map).shapes()).toEqual([]);
  });

  it('keeps the radius within the protocol bounds', () => {
    expect(brushRadius16(0)).toBe(4 * 16);
    expect(brushRadius16(10_000)).toBe(MAX_BRUSH_RADIUS * 16);
  });
});
