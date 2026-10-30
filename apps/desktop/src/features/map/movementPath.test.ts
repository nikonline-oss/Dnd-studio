import { describe, expect, it } from 'vitest';
import { shortestMovementPath, movementDistance } from './movementPath';
import type { Grid } from '@/shared/ipc/client';

const square: Grid = { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 };

describe('shortestMovementPath', () => {
  it('counts diagonal distance geometrically without assuming a rule system', () => {
    expect(
      movementDistance(
        [
          { x: 35, y: 35 },
          { x: 105, y: 105 },
        ],
        square,
      ),
    ).toBeCloseTo(Math.SQRT2);
  });
  it('returns a shortest sequence of adjacent square cells', () => {
    expect(
      shortestMovementPath({ x: 35, y: 35 }, { x: 245, y: 175 }, square, {
        width: 400,
        height: 300,
      }),
    ).toEqual([
      { x: 35, y: 35 },
      { x: 105, y: 105 },
      { x: 175, y: 105 },
      { x: 245, y: 175 },
    ]);
  });

  it('uses six-neighbor hex steps for flat and pointy grids', () => {
    const flat: Grid = { ...square, kind: 'hex_flat' };
    const pointy: Grid = { ...square, kind: 'hex_pointy' };
    expect(
      shortestMovementPath({ x: 0, y: 0 }, { x: 60, y: 70 }, flat, { width: 400, height: 300 }),
    ).toHaveLength(2);
    expect(
      shortestMovementPath({ x: 0, y: 0 }, { x: 70, y: 60 }, pointy, { width: 400, height: 300 }),
    ).toHaveLength(2);
  });

  it('returns a direct route when the map has no grid', () => {
    const none: Grid = { ...square, kind: 'none' };
    expect(
      shortestMovementPath({ x: 20, y: 30 }, { x: 110, y: 80 }, none, { width: 200, height: 150 }),
    ).toEqual([
      { x: 20, y: 30 },
      { x: 110, y: 80 },
    ]);
  });
});
