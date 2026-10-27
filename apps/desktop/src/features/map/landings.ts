/**
 * How long a dropped token waits for the events to bring its final position. Echoes of the moves
 * sent while it was dragged arrive first (a round trip later when online); shown one by one, they
 * make the token jump back along its path before it settles.
 */
export const LANDING_TIMEOUT_MS = 3000;

/** Where dropped tokens landed (Units16), until the events confirm it. */
export class Landings {
  private readonly pending = new Map<string, { x: number; y: number; until: number }>();

  constructor(
    private readonly timeoutMs = LANDING_TIMEOUT_MS,
    private readonly now: () => number = () => Date.now(),
  ) {}

  /** The token was dropped at `x, y`. */
  land(id: string, x: number, y: number): void {
    this.pending.set(id, { x, y, until: this.now() + this.timeoutMs });
  }

  /**
   * The events place the token at `x, y`: `true` while it should stay where it was dropped
   * instead. Ends once the events reach the drop point, or after the timeout (the core put the
   * token elsewhere, or someone moved it since).
   */
  holds(id: string, x: number, y: number): boolean {
    const landing = this.pending.get(id);
    if (!landing) return false;
    if ((landing.x === x && landing.y === y) || this.now() >= landing.until) {
      this.pending.delete(id);
      return false;
    }
    return true;
  }

  forget(id: string): void {
    this.pending.delete(id);
  }
}
