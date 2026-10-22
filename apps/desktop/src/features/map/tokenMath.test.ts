import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_TOKEN_SIZE,
  snapToGrid,
  Throttle,
  tokenColor,
  tokenDiameter,
  tokenInitial,
  tokenPosition,
} from './tokenMath';
import type { Grid } from '@/shared/ipc/client';

const square: Grid = { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 };

describe('token geometry', () => {
  it('is one grid cell times the scale; 70 units without a grid', () => {
    expect(tokenDiameter(square, 1)).toBe(70);
    expect(tokenDiameter(square, 2)).toBe(140);
    expect(tokenDiameter({ ...square, kind: 'none' }, 1)).toBe(DEFAULT_TOKEN_SIZE);
  });

  it('snaps to the centre of a square cell, honouring the offset', () => {
    expect(snapToGrid({ x: 10, y: 139 }, square)).toEqual({ x: 35, y: 105 });
    expect(snapToGrid({ x: 10, y: 10 }, { ...square, offset_x: 20, offset_y: -5 })).toEqual({
      x: -15,
      y: 30,
    });
  });

  it('does not snap without a square grid', () => {
    const p = { x: 12.5, y: 99 };
    expect(snapToGrid(p, { ...square, kind: 'none' })).toEqual(p);
    expect(snapToGrid(p, { ...square, kind: 'hex_flat' })).toEqual(p);
  });

  it('stores positions in Units16, clamped onto the map', () => {
    const map = { width: 100, height: 50 };
    expect(tokenPosition({ x: 35, y: 10.5 }, map)).toEqual({ x: 560, y: 168 });
    expect(tokenPosition({ x: -5, y: 80 }, map)).toEqual({ x: 0, y: 800 });
  });

  it('gives stable colours and initials', () => {
    expect(tokenColor('abc')).toBe(tokenColor('abc'));
    expect(tokenInitial('  mira')).toBe('M');
    expect(tokenInitial('')).toBe('?');
  });
});

describe('Throttle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sends at most once per interval, always the latest value', () => {
    const sent: number[] = [];
    const throttle = new Throttle<number>(
      80,
      (v) => sent.push(v),
      () => Date.now(),
    );
    throttle.push(1); // first value goes out at once
    throttle.push(2);
    throttle.push(3);
    expect(sent).toEqual([1]);
    vi.advanceTimersByTime(80);
    expect(sent).toEqual([1, 3]);
    vi.advanceTimersByTime(500);
    expect(sent).toEqual([1, 3]);
  });

  it('flush sends the pending value immediately; cancel drops it', () => {
    const sent: number[] = [];
    const throttle = new Throttle<number>(
      80,
      (v) => sent.push(v),
      () => Date.now(),
    );
    throttle.push(1);
    throttle.push(2);
    throttle.flush();
    expect(sent).toEqual([1, 2]);
    vi.advanceTimersByTime(200);
    expect(sent).toEqual([1, 2]);
    throttle.push(3);
    throttle.push(4);
    throttle.cancel();
    vi.advanceTimersByTime(200);
    expect(sent).toEqual([1, 2, 3]);
  });
});
