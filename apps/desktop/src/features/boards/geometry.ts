type Point = { x: number; y: number };
type Size = { w: number; h: number };

const center = (at: Point, size: Size): Point => ({
  x: at.x + size.w / 2,
  y: at.y + size.h / 2,
});

/** Where the line from `a` towards the centre `b` of a card of size `card` crosses its border. */
function clipToCard(a: Point, b: Point, card: Size): Point {
  const hw = card.w / 2;
  const hh = card.h / 2;
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  if (dx === 0 && dy === 0) return b;
  // A zero component never limits the line (and must not turn into hw / Infinity = 0).
  const scale = Math.min(
    dx === 0 ? Infinity : hw / Math.abs(dx),
    dy === 0 ? Infinity : hh / Math.abs(dy),
  );
  return { x: b.x + dx * Math.min(scale, 1), y: b.y + dy * Math.min(scale, 1) };
}

/**
 * The line between two cards, from the border of the first to the border of the second, so it
 * neither starts under the source card nor hides its arrow under the target.
 */
export function cardLine(
  from: { at: Point; size: Size },
  to: { at: Point; size: Size },
): { start: Point; end: Point } {
  const a = center(from.at, from.size);
  const b = center(to.at, to.size);
  return { start: clipToCard(b, a, from.size), end: clipToCard(a, b, to.size) };
}
