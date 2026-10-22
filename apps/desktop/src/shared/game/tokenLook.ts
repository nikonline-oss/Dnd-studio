// How a token looks, shared by the map (Pixi) and lists such as the initiative tracker, so the same
// creature has the same colour and letter everywhere.

/** Muted, distinct colours that read on light and dark maps. */
const TOKEN_COLORS = [
  0xc0574a, 0x4f86c6, 0x5aa36b, 0xc99a3b, 0x8c63b8, 0x3fa3a3, 0xc46a9b, 0x7d8a4a,
] as const;

/** A stable colour for a token, from its character (or its own) id. */
export function tokenColor(key: string): number {
  let hash = 0;
  for (let i = 0; i < key.length; i++) hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  return TOKEN_COLORS[hash % TOKEN_COLORS.length] ?? 0x8a7f6a;
}

/** The same colour as a CSS value. */
export function tokenCssColor(key: string): string {
  return `#${tokenColor(key).toString(16).padStart(6, '0')}`;
}

/** First letter of a name for the token face; `?` for an unnamed token. */
export function tokenInitial(name: string): string {
  return name.trim().charAt(0).toUpperCase() || '?';
}
