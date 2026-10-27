import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { applyEvent } from './evolve';
import { queryKeys, useSnapshotSelect } from './useGame';
import type { CharacterDto, Snapshot } from '@/shared/ipc/client';

const mira: CharacterDto = {
  id: 'c1',
  kind: 'pc',
  name: 'Mira',
  owner: null,
  status: 'active',
  sheet_json: '{}',
  version: 1,
};

const snapshot: Snapshot = {
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [],
  tokens: [
    {
      id: 't1',
      map_id: 'm1',
      character_id: null,
      asset_id: null,
      name: 'Goblin',
      x: 0,
      y: 0,
      rotation: 0,
      scale: 1,
      layer: 'tokens',
      visible: true,
      version: 1,
    },
  ],
  characters: [mira],
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

const tick = () =>
  new Promise((resolve) => {
    setTimeout(resolve, 10);
  });

describe('useSnapshotSelect', () => {
  it('re-renders only when the selected slice changes', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
    client.setQueryData(queryKeys.snapshot, snapshot);
    let renders = 0;
    function Names() {
      renders += 1;
      const characters = useSnapshotSelect((s) => s.characters);
      return <p>{characters?.map((c) => c.name).join(',')}</p>;
    }
    render(
      <QueryClientProvider client={client}>
        <Names />
      </QueryClientProvider>,
    );
    expect(screen.getByText('Mira')).toBeInTheDocument();
    const settled = renders;

    // A token drag: many moves, none of them about characters.
    await act(async () => {
      for (let x = 16; x <= 160; x += 16) {
        const current = client.getQueryData<Snapshot>(queryKeys.snapshot);
        if (!current) throw new Error('no snapshot');
        client.setQueryData(
          queryKeys.snapshot,
          applyEvent(current, { kind: 'token_moved', id: 't1', x, y: 0 }),
        );
      }
      // Query observers are notified on a timer, not synchronously.
      await tick();
    });
    expect(renders).toBe(settled);

    await act(async () => {
      const current = client.getQueryData<Snapshot>(queryKeys.snapshot);
      if (!current) throw new Error('no snapshot');
      client.setQueryData(
        queryKeys.snapshot,
        applyEvent(current, {
          kind: 'character_updated',
          character: { ...mira, name: 'Mira the Bold', version: 2 },
        }),
      );
      await tick();
    });
    expect(screen.getByText('Mira the Bold')).toBeInTheDocument();
    expect(renders).toBe(settled + 1);
  });
});
