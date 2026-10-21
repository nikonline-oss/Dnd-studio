import type { Event, EventBatch, Snapshot } from '@/shared/ipc/client';

/**
 * Applies one event to a snapshot. Mirrors `dnd_game_core::evolve`: the client only applies
 * facts decided by the core, it never decides anything itself (ADR-0001).
 */
export function applyEvent(snapshot: Snapshot, event: Event): Snapshot {
  switch (event.kind) {
    case 'campaign_meta_updated':
      return { ...snapshot, campaign: event.campaign };
    case 'map_created':
    case 'map_updated':
      return { ...snapshot, maps: upsert(snapshot.maps, event.map) };
    case 'map_deleted':
      return {
        ...snapshot,
        maps: snapshot.maps.filter((m) => m.id !== event.id),
        tokens: snapshot.tokens.filter((t) => t.map_id !== event.id),
      };
    case 'token_created':
    case 'token_updated':
      return { ...snapshot, tokens: upsert(snapshot.tokens, event.token) };
    case 'token_moved':
      return {
        ...snapshot,
        tokens: snapshot.tokens.map((t) =>
          t.id === event.id ? { ...t, x: event.x, y: event.y } : t,
        ),
      };
    case 'token_deleted':
      return { ...snapshot, tokens: snapshot.tokens.filter((t) => t.id !== event.id) };
    case 'character_created':
    case 'character_updated':
      return { ...snapshot, characters: upsert(snapshot.characters, event.character) };
  }
}

/**
 * Applies a batch if it directly follows the snapshot. Returns the snapshot unchanged for an
 * already-applied batch and `null` on a gap (the caller must refetch the snapshot).
 */
export function applyBatch(snapshot: Snapshot, batch: EventBatch): Snapshot | null {
  const last = batch.seq + batch.events.length - 1;
  if (last <= snapshot.seq) return snapshot;
  if (batch.seq !== snapshot.seq + 1) return null;
  const next = batch.events.reduce(applyEvent, snapshot);
  return { ...next, seq: last };
}

function upsert<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex((i) => i.id === item.id);
  if (index === -1) return [...items, item].sort((a, b) => a.id.localeCompare(b.id));
  return items.map((i, n) => (n === index ? item : i));
}
