import { describe, expect, it } from 'vitest';
import { Landings } from './landings';

describe('Landings', () => {
  it('ignores echoes of the drag until the drop point arrives', () => {
    const landings = new Landings(3000, () => 0);
    landings.land('t', 160, 160);
    // Moves sent while dragging come back after the drop.
    expect(landings.holds('t', 32, 32)).toBe(true);
    expect(landings.holds('t', 96, 96)).toBe(true);
    expect(landings.holds('t', 160, 160)).toBe(false);
    // Settled: later moves (someone else's) show at once.
    expect(landings.holds('t', 48, 48)).toBe(false);
  });

  it('gives up after the timeout when the core put the token elsewhere', () => {
    let now = 0;
    const landings = new Landings(3000, () => now);
    landings.land('t', 160, 160);
    expect(landings.holds('t', 32, 32)).toBe(true);
    now = 3000;
    expect(landings.holds('t', 32, 32)).toBe(false);
  });

  it('leaves other tokens alone', () => {
    const landings = new Landings(3000, () => 0);
    landings.land('t', 160, 160);
    expect(landings.holds('u', 32, 32)).toBe(false);
    landings.forget('t');
    expect(landings.holds('t', 32, 32)).toBe(false);
  });
});
