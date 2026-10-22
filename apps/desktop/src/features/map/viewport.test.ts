import { describe, expect, it } from 'vitest';
import { gridPattern } from './gridPattern';
import {
  clampCamera,
  fitCamera,
  fromUnits16,
  KEEP_VISIBLE,
  mapToScreen,
  MAX_ZOOM,
  panBy,
  screenToMap,
  toUnits16,
  wheelFactor,
  zoomAt,
  zoomLimits,
} from './viewport';

const screen = { width: 1000, height: 500 };
const map = { width: 4000, height: 3000 };

describe('viewport', () => {
  it('fits the whole map, centred', () => {
    const camera = fitCamera(screen, map);
    // Height is the limiting side: 500 / 3000 with a 5 % margin.
    expect(camera.zoom).toBeCloseTo((500 / 3000) * 0.95);
    const topLeft = mapToScreen(camera, { x: 0, y: 0 });
    const bottomRight = mapToScreen(camera, { x: map.width, y: map.height });
    expect(topLeft.x + bottomRight.x).toBeCloseTo(screen.width);
    expect(topLeft.y + bottomRight.y).toBeCloseTo(screen.height);
  });

  it('converts screen and map coordinates both ways', () => {
    const camera = { x: 120, y: -40, zoom: 0.5 };
    const p = { x: 1234.5, y: 77 };
    const back = screenToMap(camera, mapToScreen(camera, p));
    expect(back.x).toBeCloseTo(p.x);
    expect(back.y).toBeCloseTo(p.y);
  });

  it('keeps the map point under the cursor fixed while zooming', () => {
    const camera = fitCamera(screen, map);
    const cursor = { x: 700, y: 300 };
    const before = screenToMap(camera, cursor);
    const zoomed = zoomAt(camera, cursor, 1.5, screen, map);
    expect(zoomed.zoom).toBeCloseTo(camera.zoom * 1.5);
    const after = screenToMap(zoomed, cursor);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it('clamps the zoom between half the fit and MAX_ZOOM', () => {
    const camera = fitCamera(screen, map);
    const { min, max } = zoomLimits(screen, map);
    expect(max).toBe(MAX_ZOOM);
    expect(zoomAt(camera, { x: 0, y: 0 }, 1e6, screen, map).zoom).toBe(MAX_ZOOM);
    expect(zoomAt(camera, { x: 0, y: 0 }, 1e-6, screen, map).zoom).toBeCloseTo(min);
    expect(min).toBeCloseTo(camera.zoom / 2);
  });

  it('allows zooming in to fit a map smaller than MAX_ZOOM', () => {
    const tiny = { width: 50, height: 50 };
    expect(zoomLimits(screen, tiny).max).toBeCloseTo((500 / 50) * 0.95);
  });

  it('never pans the map completely off screen', () => {
    const camera = fitCamera(screen, map);
    const far = panBy(camera, 1e6, -1e6, screen, map);
    expect(far.x).toBe(screen.width - KEEP_VISIBLE);
    expect(far.y).toBe(KEEP_VISIBLE - map.height * camera.zoom);
    // Panning inside the bounds is exact.
    const near = panBy(camera, 10, -5, screen, map);
    expect(near.x).toBeCloseTo(camera.x + 10);
    expect(near.y).toBeCloseTo(camera.y - 5);
  });

  it('clamps by the smaller of the map and the viewport', () => {
    const small = { width: 10, height: 10 };
    const camera = clampCamera({ x: -1000, y: 2000, zoom: 1 }, screen, small);
    expect(camera.x).toBe(0);
    expect(camera.y).toBe(screen.height - 10);
  });

  it('round-trips Units16', () => {
    expect(toUnits16(70)).toBe(1120);
    expect(toUnits16(35.5)).toBe(568);
    expect(fromUnits16(toUnits16(12.25))).toBe(12.25);
    // Sub-1/16 positions are rounded to the stored integer.
    expect(toUnits16(0.03)).toBe(0);
  });

  it('zooms in on wheel up and out on wheel down, in any delta mode', () => {
    expect(wheelFactor(-100, 0)).toBeGreaterThan(1);
    expect(wheelFactor(100, 0)).toBeLessThan(1);
    expect(wheelFactor(100, 0) * wheelFactor(-100, 0)).toBeCloseTo(1);
    // Line mode (Firefox-style mice): 3 lines ≈ 48 px.
    expect(wheelFactor(3, 1)).toBeCloseTo(wheelFactor(48, 0));
  });
});

describe('gridPattern', () => {
  const base = { cell_size: 70, offset_x: 0, offset_y: 0 } as const;

  it('draws nothing for no grid', () => {
    expect(gridPattern({ ...base, kind: 'none' })).toBeNull();
  });

  it('repeats one square cell with its top and left edges', () => {
    const pattern = gridPattern({ ...base, kind: 'square' });
    expect(pattern).toMatchObject({ width: 70, height: 70, originX: 0, originY: 0 });
    expect(pattern?.segments).toEqual([
      [0, 0, 70, 0],
      [0, 0, 0, 70],
    ]);
  });

  it('wraps offsets into one period, including negative ones', () => {
    const pattern = gridPattern({ kind: 'square', cell_size: 70, offset_x: 145, offset_y: -10 });
    expect(pattern?.originX).toBeCloseTo(5);
    expect(pattern?.originY).toBeCloseTo(60);
  });

  it.each(['hex_flat', 'hex_pointy'] as const)(
    '%s: a period of 3r × cell with four hexes of equal edges',
    (kind) => {
      const pattern = gridPattern({ ...base, kind });
      const r = 70 / Math.sqrt(3);
      const [long, short] = kind === 'hex_flat' ? ['width', 'height'] : ['height', 'width'];
      expect(pattern?.[long as 'width']).toBeCloseTo(3 * r);
      expect(pattern?.[short as 'width']).toBeCloseTo(70);
      expect(pattern?.segments).toHaveLength(24);
      for (const [x1, y1, x2, y2] of pattern?.segments ?? []) {
        expect(Math.hypot(x2 - x1, y2 - y1)).toBeCloseTo(r);
      }
    },
  );

  it('scales the line width with the cell, within 1..4 units', () => {
    expect(gridPattern({ ...base, kind: 'square', cell_size: 8 })?.lineWidth).toBe(1);
    expect(gridPattern({ ...base, kind: 'square', cell_size: 70 })?.lineWidth).toBe(2);
    expect(gridPattern({ ...base, kind: 'square', cell_size: 2048 })?.lineWidth).toBe(4);
  });
});
