import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { queryKeys, useGameEventsSync } from './useGame';
import { events, type Snapshot } from '@/shared/ipc/client';

const snapshot: Snapshot = {
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
  triggers: [],
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
};

describe('compendium synchronization', () => {
  it('refreshes compendium queries once on plugin events and on a full resnapshot', async () => {
    mockIPC(() => null, { shouldMockEvents: true });
    const client = new QueryClient();
    client.setQueryData(queryKeys.snapshot, snapshot);
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    renderHook(
      () => {
        useGameEventsSync();
      },
      {
        wrapper: ({ children }) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      },
    );
    const batch = {
      seq: 1,
      caused_by: null,
      events: [{ kind: 'plugin_uninstalled' as const, id: 'srd5' }],
    };
    await act(async () => {
      await events.gameEventsEmitted.emit(batch);
    });
    await waitFor(() => {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['compendia'] });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: ['compendium'] });
    });
    invalidate.mockClear();
    await act(async () => {
      await events.gameEventsEmitted.emit(batch);
    });
    expect(invalidate).not.toHaveBeenCalled();
    await act(async () => {
      await events.gameSnapshotReplaced.emit({ ...snapshot, seq: 1 });
    });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['compendia'] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['compendium'] });
  });
});
