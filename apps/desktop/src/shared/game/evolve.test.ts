import { describe, expect, it } from 'vitest';
import { applyBatch, applyEvent } from './evolve';
import type {
  BoardDto,
  BoardEdgeDto,
  BoardNodeDto,
  CombatantDto,
  CombatDto,
  FogOp,
  MapDto,
  Snapshot,
  TokenDto,
} from '@/shared/ipc/client';

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
  journal_folders: [],
  plugins: [],
  system_plugin: null,
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
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

describe('applyEvent: fog (mirrors the core)', () => {
  const reveal: FogOp = {
    id: 'op1',
    kind: 'reveal',
    shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
  };
  const hide: FogOp = {
    id: 'op2',
    kind: 'hide',
    shape: { type: 'brush', radius: 32, points: [10, 10, 40, 40] },
  };
  const withMap = applyEvent(empty, { kind: 'map_created', map: map('m1') });

  it('appends operations in order; a map without a reset starts clear', () => {
    let s = applyEvent(withMap, { kind: 'fog_applied', map_id: 'm1', op: reveal });
    s = applyEvent(s, { kind: 'fog_applied', map_id: 'm1', op: hide });
    expect(s.fog).toEqual([{ map_id: 'm1', covered: false, ops: [reveal, hide] }]);
  });

  it('draws a replayed operation once', () => {
    let s = applyEvent(withMap, { kind: 'fog_applied', map_id: 'm1', op: reveal });
    s = applyEvent(s, { kind: 'fog_applied', map_id: 'm1', op: reveal });
    expect(s.fog[0]?.ops).toHaveLength(1);
  });

  it('reset drops operations; deleting the map drops its fog', () => {
    let s = applyEvent(withMap, { kind: 'fog_applied', map_id: 'm1', op: reveal });
    s = applyEvent(s, { kind: 'fog_reset', map_id: 'm1', covered: true });
    expect(s.fog).toEqual([{ map_id: 'm1', covered: true, ops: [] }]);
    s = applyEvent(s, { kind: 'map_deleted', id: 'm1' });
    expect(s.fog).toEqual([]);
  });

  it('ignores fog of an unknown map', () => {
    expect(applyEvent(empty, { kind: 'fog_applied', map_id: 'nope', op: reveal })).toBe(empty);
    expect(applyEvent(empty, { kind: 'fog_reset', map_id: 'nope', covered: true })).toBe(empty);
  });
});

describe('applyEvent: combat (mirrors the core)', () => {
  const combat: CombatDto = { id: 'c1', map_id: 'm1', round: 1, turn: null, version: 1 };
  const combatant = (id: string, tokenId: string | null): CombatantDto => ({
    id,
    combat_id: 'c1',
    token_id: tokenId,
    character_id: null,
    name: id,
    initiative: null,
    sort_order: 0,
    version: 1,
  });
  let s = applyEvent(empty, { kind: 'map_created', map: map('m1') });
  s = applyEvent(s, { kind: 'token_created', token: token('t1', 'm1') });
  s = applyEvent(s, { kind: 'combat_started', combat });
  s = applyEvent(s, { kind: 'combatant_added', combatant: combatant('a', 't1') });
  s = applyEvent(s, { kind: 'combatant_added', combatant: combatant('b', null) });
  const fight = s;

  it('tracks turns and initiative', () => {
    let next = applyEvent(fight, {
      kind: 'combat_updated',
      combat: { ...combat, turn: 'a', round: 2, version: 2 },
    });
    next = applyEvent(next, {
      kind: 'combatant_updated',
      combatant: { ...combatant('a', 't1'), initiative: 15, version: 2 },
    });
    expect(next.combats).toEqual([{ ...combat, turn: 'a', round: 2, version: 2 }]);
    expect(next.combatants.find((c) => c.id === 'a')?.initiative).toBe(15);
    next = applyEvent(next, { kind: 'combatant_removed', id: 'b' });
    expect(next.combatants.map((c) => c.id)).toEqual(['a']);
  });

  it('a deleted token leaves its combatant without a token', () => {
    const next = applyEvent(fight, { kind: 'token_deleted', id: 't1' });
    expect(next.combatants.find((c) => c.id === 'a')?.token_id).toBeNull();
  });

  it('ending the combat or deleting its map removes everything', () => {
    for (const event of [
      { kind: 'combat_ended', id: 'c1' } as const,
      { kind: 'map_deleted', id: 'm1' } as const,
    ]) {
      const next = applyEvent(fight, event);
      expect(next.combats).toEqual([]);
      expect(next.combatants).toEqual([]);
    }
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

describe('applyEvent: session events (ADR-0022)', () => {
  it('adds a joined user and updates a rejoining one', () => {
    const mira = { id: 'u2', name: 'Mira', role: 'player' as const };
    const joined = applyEvent(empty, { kind: 'user_joined', user: mira });
    expect(joined.users).toEqual([mira]);
    const renamed = applyEvent(joined, {
      kind: 'user_joined',
      user: { ...mira, name: 'Mira the Bold' },
    });
    expect(renamed.users.map((u) => u.name)).toEqual(['Mira the Bold']);
  });

  it('a redacted event only advances the sequence', () => {
    const s = applyBatch(empty, {
      seq: 1,
      caused_by: null,
      events: [{ kind: 'redacted' }, { kind: 'map_created', map: map('m1') }, { kind: 'redacted' }],
    });
    expect(s?.seq).toBe(3);
    expect(s?.maps).toHaveLength(1);
    expect(applyEvent(empty, { kind: 'redacted' })).toBe(empty);
  });
});

describe('applyEvent: plugins (ADR-0023)', () => {
  const plugin = (id: string, version = '0.1.0') => ({
    id,
    version,
    kind: 'content' as const,
    name: { ru: 'Плагин', en: 'Plugin' },
    license: 'MIT',
    attribution: null,
    archive_sha256: 'a'.repeat(64),
    system: null,
  });

  it('installs, replaces, chooses the system and uninstalls', () => {
    let s = applyEvent(empty, { kind: 'plugin_installed', plugin: plugin('srd5') });
    s = applyEvent(s, { kind: 'plugin_installed', plugin: plugin('dark') });
    s = applyEvent(s, { kind: 'plugin_installed', plugin: plugin('srd5', '0.2.0') });
    expect(s.plugins.map((p) => `${p.id}@${p.version}`)).toEqual(['dark@0.1.0', 'srd5@0.2.0']);
    s = applyEvent(s, { kind: 'system_plugin_set', id: 'srd5' });
    expect(s.system_plugin).toBe('srd5');
    s = applyEvent(s, { kind: 'plugin_uninstalled', id: 'dark' });
    expect(s.plugins.map((p) => p.id)).toEqual(['srd5']);
  });
});

describe('applyEvent: boards (mirrors the core, ADR-0024)', () => {
  const board: BoardDto = {
    id: 'b1',
    name: 'Plot',
    sort_order: 0,
    visible_to_players: false,
    version: 1,
  };
  const node = (id: string): BoardNodeDto => ({
    id,
    board_id: 'b1',
    content: { type: 'note', markdown: id },
    x: 0,
    y: 0,
    width: 3200,
    height: 1600,
    visibility: 'gm',
    version: 1,
  });
  const edge: BoardEdgeDto = {
    id: 'e1',
    board_id: 'b1',
    from: 'n1',
    to: 'n2',
    label: '',
    directed: false,
    visibility: 'players',
    version: 1,
  };
  let s = applyEvent(empty, { kind: 'board_created', board });
  s = applyEvent(s, { kind: 'board_node_created', node: node('n1') });
  s = applyEvent(s, { kind: 'board_node_created', node: node('n2') });
  s = applyEvent(s, { kind: 'board_edge_created', edge });
  const full = s;

  it('moves nodes without touching their version', () => {
    const moved = applyEvent(full, { kind: 'board_node_moved', id: 'n1', x: 32, y: -48 });
    expect(moved.board_nodes[0]).toMatchObject({ x: 32, y: -48, version: 1 });
  });

  it('a deleted node takes its edges; a deleted board takes everything', () => {
    expect(applyEvent(full, { kind: 'board_node_deleted', id: 'n2' }).board_edges).toEqual([]);
    const gone = applyEvent(full, { kind: 'board_deleted', id: 'b1' });
    expect([gone.boards, gone.board_nodes, gone.board_edges]).toEqual([[], [], []]);
  });
});
