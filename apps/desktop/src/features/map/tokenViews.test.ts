import { describe, expect, it } from 'vitest';
import { tokenViews } from './tokenViews';
import type { CampaignInfo, Role, Snapshot, TokenDto, TokenLayer } from '@/shared/ipc/client';

const ME = 'u-me';
const token = (id: string, layer: TokenLayer, extra: Partial<TokenDto> = {}): TokenDto => ({
  id,
  map_id: 'm1',
  character_id: null,
  asset_id: null,
  name: id,
  x: 0,
  y: 0,
  rotation: 0,
  scale: 1,
  layer,
  visible: true,
  version: 1,
  ...extra,
});

const snapshot = (visibleToPlayers: boolean): Snapshot => ({
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [
    {
      id: 'm1',
      name: 'Crypt',
      background_asset_id: null,
      width: 100,
      height: 100,
      grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
      visible_to_players: visibleToPlayers,
      version: 1,
    },
  ],
  tokens: [
    token('gm-note', 'gm'),
    token('mine', 'tokens', { character_id: 'c-mine' }),
    token('hidden', 'tokens', { visible: false }),
    token('rug', 'background'),
    token('other', 'tokens', { character_id: 'c-other' }),
    token('elsewhere', 'tokens', { map_id: 'm2' }),
  ],
  characters: [
    {
      id: 'c-mine',
      kind: 'pc',
      name: 'Mira',
      owner: ME,
      status: 'active',
      sheet_json: '{}',
      version: 1,
    },
    {
      id: 'c-other',
      kind: 'pc',
      name: 'Bob',
      owner: 'u-other',
      status: 'active',
      sheet_json: '{}',
      version: 1,
    },
  ],
  users: [],
  journal: [],
  assets: [],
  fog: [],
  chat: [],
});

const viewer = (role: Role): CampaignInfo => ({ name: 'C', path: '/c', user_id: ME, role });

describe('tokenViews', () => {
  it('GM roles see every token of the map, layers bottom first, GM-only ones marked', () => {
    for (const role of ['gm', 'co_gm'] as const) {
      const views = tokenViews(snapshot(false), 'm1', viewer(role));
      expect(views.map((v) => v.token.id)).toEqual(['rug', 'mine', 'hidden', 'other', 'gm-note']);
      expect(views.every((v) => v.movable)).toBe(true);
      expect(views.filter((v) => v.gmOnly).map((v) => v.token.id)).toEqual(['hidden', 'gm-note']);
    }
  });

  it('players see no GM layer or hidden tokens and move only their own on a shared map', () => {
    const views = tokenViews(snapshot(true), 'm1', viewer('player'));
    expect(views.map((v) => v.token.id)).toEqual(['rug', 'mine', 'other']);
    expect(views.filter((v) => v.movable).map((v) => v.token.id)).toEqual(['mine']);
    // On a map not shared with players nothing is movable.
    expect(tokenViews(snapshot(false), 'm1', viewer('player')).some((v) => v.movable)).toBe(false);
  });

  it('spectators move nothing; colours follow the character', () => {
    const views = tokenViews(snapshot(true), 'm1', viewer('spectator'));
    expect(views.some((v) => v.movable)).toBe(false);
    expect(views.find((v) => v.token.id === 'mine')?.colorKey).toBe('c-mine');
    expect(views.find((v) => v.token.id === 'rug')?.colorKey).toBe('rug');
  });

  it('an unknown map has no tokens', () => {
    expect(tokenViews(snapshot(true), 'nope', viewer('gm'))).toEqual([]);
  });
});
