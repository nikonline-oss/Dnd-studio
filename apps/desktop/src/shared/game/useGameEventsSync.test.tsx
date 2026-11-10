import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { queryKeys, useGameEventsSync, useSnapshot } from './useGame';
import * as playback from './soundPlayback';
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
  it('reconciles sound controls after refetching a sequence gap', async () => {
    const paused = {
      asset_id: '0190a000-0000-7000-8000-000000000050',
      sha256: 'a'.repeat(64),
      mime: 'audio/ogg',
      volume: 60,
      looped: true,
      paused: true,
      position_ms: 15000,
    };
    let current = snapshot;
    mockIPC((cmd) => (cmd === 'get_snapshot' ? current : null), { shouldMockEvents: true });
    const sync = vi.spyOn(playback, 'syncLoopingSounds').mockImplementation(() => undefined);
    const client = new QueryClient();
    renderHook(
      () => {
        useGameEventsSync();
        return useSnapshot();
      },
      {
        wrapper: ({ children }) => (
          <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
      },
    );
    await waitFor(() => {
      expect(client.getQueryData(queryKeys.snapshot)).toEqual(snapshot);
    });
    sync.mockClear();
    current = { ...snapshot, seq: 10, sounds: [paused] };
    await act(async () => {
      await events.gameEventsEmitted.emit({
        seq: 10,
        caused_by: null,
        events: [{ kind: 'redacted' }],
      });
    });
    await waitFor(() => {
      expect(sync).toHaveBeenCalledWith([paused]);
    });
    sync.mockRestore();
  });
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

  it('drops sheet and journal caches that a new snapshot no longer allows', async () => {
    mockIPC(() => null, { shouldMockEvents: true });
    const client = new QueryClient();
    client.setQueryData(queryKeys.snapshot, snapshot);
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
    const kept = {
      id: 'kept',
      folder_id: null,
      title: 'Shared',
      sort_order: 0,
      visibility: 'players' as const,
      players_can_edit: false,
      author: null,
      version: 1,
    };
    client.setQueryData([...queryKeys.sheetView, 'gone'], { computed: { hp: 3 } });
    client.setQueryData([...queryKeys.sheetView, 'kept'], { computed: { hp: 3 } });
    client.setQueryData(['journal', 'gone'], { markdown: 'GM only' });
    client.setQueryData(['journal', 'kept'], { markdown: 'shared text' });

    await act(async () => {
      await events.gameSnapshotReplaced.emit({ ...snapshot, seq: 1, journal: [kept] });
    });

    // Sheet views are recomputed from the snapshot, so every cached one is dropped.
    expect(client.getQueryData([...queryKeys.sheetView, 'gone'])).toBeUndefined();
    expect(client.getQueryData([...queryKeys.sheetView, 'kept'])).toBeUndefined();
    // A journal text that left the snapshot index is removed, not just invalidated.
    expect(client.getQueryData(['journal', 'gone'])).toBeUndefined();
    expect(client.getQueryData(['journal', 'kept'])).toEqual({ markdown: 'shared text' });
  });
});
