import { describe, expect, it } from 'vitest';
import { cardLine } from './geometry';

describe('cardLine', () => {
  it('runs from the border of the source card to the border of the target', () => {
    // Two 100×50 cards side by side, 100 px apart.
    const line = cardLine(
      { at: { x: 0, y: 0 }, size: { w: 100, h: 50 } },
      { at: { x: 200, y: 0 }, size: { w: 100, h: 50 } },
    );
    expect(line.start).toEqual({ x: 100, y: 25 });
    expect(line.end).toEqual({ x: 200, y: 25 });
  });

  it('leaves the source through its top or bottom when the target is below', () => {
    const line = cardLine(
      { at: { x: 0, y: 0 }, size: { w: 100, h: 50 } },
      { at: { x: 0, y: 200 }, size: { w: 100, h: 50 } },
    );
    expect(line.start).toEqual({ x: 50, y: 50 });
    expect(line.end).toEqual({ x: 50, y: 200 });
  });

  it('collapses to the centre when the cards share it', () => {
    const card = { at: { x: 10, y: 10 }, size: { w: 20, h: 20 } };
    expect(cardLine(card, card)).toEqual({ start: { x: 20, y: 20 }, end: { x: 20, y: 20 } });
  });
});
