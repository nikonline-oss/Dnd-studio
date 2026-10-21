import { describe, expect, it } from 'vitest';
import { applyBatch, applyEvent } from './evolve';
import type { MapDto, Snapshot, TokenDto } from '@/shared/ipc/client';

const map = (id: string, version = 1): MapDto => ({
  id,
  name: `Map ${id}`,
  background_asset_id: null,
  width: 4000,
  height: 3000,
  grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
  visible_to_players: false,
  version,
});

const token = (id: string, mapId: string): TokenDto => ({
  id,
  map_id: mapId,
  character_id: null,
  asset_id: null,
  name: `Token ${id}`,
  x: 0,
  y: 0,
  rotation: 0,
  scale: 1,
  layer: 'tokens',
  visible: true,
  version: 1,
});

const empty: Snapshot = {
  seq: 0,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [],
  tokens: [],
  characters: [],
  users: [],
  journal: [],
  chat: [],
};

describe('applyEvent', () => {
  it('upserts maps and keeps id order', () => {
    let s = applyEvent(empty, { kind: 'map_created', map: map('b') });
    s = applyEvent(s, { kind: 'map_created', map: map('a') });
    s = applyEvent(s, { kind: 'map_updated', map: { ...map('b', 2), name: 'Renamed' } });
    expect(s.maps.map((m) => [m.id, m.name, m.version])).toEqual([
      ['a', 'Map a', 1],
      ['b', 'Renamed', 2],
    ]);
  });

  it('deleting a map removes its tokens', () => {
    let s = applyEvent(empty, { kind: 'map_created', map: map('m1') });
    s = applyEvent(s, { kind: 'map_created', map: map('m2') });
    s = applyEvent(s, { kind: 'token_created', token: token('t1', 'm1') });
    s = applyEvent(s, { kind: 'token_created', token: token('t2', 'm2') });
    s = applyEvent(s, { kind: 'map_deleted', id: 'm1' });
    expect(s.tokens.map((t) => t.id)).toEqual(['t2']);
  });

  it('a stray move after delete does not resurrect the token', () => {
    let s = applyEvent(empty, { kind: 'token_created', token: token('t1', 'm1') });
    s = applyEvent(s, { kind: 'token_deleted', id: 't1' });
    s = applyEvent(s, { kind: 'token_moved', id: 't1', x: 1, y: 1 });
    expect(s.tokens).toEqual([]);
  });

  it('moves tokens without touching their version', () => {
    let s = applyEvent(empty, { kind: 'token_created', token: token('t1', 'm1') });
    s = applyEvent(s, { kind: 'token_moved', id: 't1', x: 70, y: 35 });
    expect(s.tokens[0]).toMatchObject({ x: 70, y: 35, version: 1 });
  });
});

describe('applyBatch', () => {
  const batch = (seq: number, n = 1) => ({
    seq,
    caused_by: null,
    events: Array.from({ length: n }, (_, i) => ({
      kind: 'map_created' as const,
      map: map(`m${seq + i}`),
    })),
  });

  it('applies the next batch and advances seq', () => {
    const s = applyBatch(empty, batch(1, 2));
    expect(s?.seq).toBe(2);
    expect(s?.maps).toHaveLength(2);
  });

  it('ignores an already applied batch', () => {
    const s = applyBatch({ ...empty, seq: 5 }, batch(3, 2));
    expect(s).toEqual({ ...empty, seq: 5 });
  });

  it('reports a gap', () => {
    expect(applyBatch(empty, batch(3))).toBeNull();
  });
});
