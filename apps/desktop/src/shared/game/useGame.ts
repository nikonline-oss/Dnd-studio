import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { applyBatch } from './evolve';
import { playSound, stopSound, syncLoopingSounds } from './soundPlayback';
import {
  events,
  IntentRejected,
  ipc,
  commands,
  unwrap,
  type Intent,
  type Snapshot,
} from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';

export const queryKeys = {
  campaign: ['campaign'] as const,
  snapshot: ['snapshot'] as const,
  journalEntry: (id: string) => ['journal', id] as const,
};

/** The open campaign, or `null`. */
export function useCampaign() {
  return useQuery({ queryKey: queryKeys.campaign, queryFn: ipc.campaignCurrent });
}

/** Game state of the open campaign. Kept current by {@link useGameEventsSync}. */
export function useSnapshot(enabled = true) {
  return useQuery({ queryKey: queryKeys.snapshot, queryFn: ipc.getSnapshot, enabled });
}

/**
 * A slice of the game state. The component re-renders only when the slice changes: TanStack Query
 * shares structure between results, and `applyEvent` keeps untouched arrays as they were, so a
 * token move does not wake the character list or the chat. Prefer this over {@link useSnapshot}.
 */
export function useSnapshotSelect<T>(select: (snapshot: Snapshot) => T): T | undefined {
  return useQuery({ queryKey: queryKeys.snapshot, queryFn: ipc.getSnapshot, select }).data;
}

/** Content-addressed plugin membership; unchanged by token movement and other game events. */
export function usePluginRevision(): string | undefined {
  return useSnapshotSelect((s) =>
    s.plugins
      .map((p) => `${p.id}:${p.archive_sha256}`)
      .sort()
      .join('|'),
  );
}

/** Subscribes once (in the app shell) to game events and patches the snapshot cache. */
export function useGameEventsSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    // A session server replaces the whole state (connect, reconnect, visibility change).
    const unlistenSnapshot = events.gameSnapshotReplaced.listen(({ payload }) => {
      syncLoopingSounds(payload.sounds ?? []);
      queryClient.setQueryData(queryKeys.snapshot, payload);
      void queryClient.invalidateQueries({ queryKey: ['journal'] });
      void queryClient.invalidateQueries({ queryKey: ['compendia'] });
      void queryClient.invalidateQueries({ queryKey: ['compendium'] });
    });
    const unlisten = events.gameEventsEmitted.listen(({ payload }) => {
      const current = queryClient.getQueryData<Snapshot>(queryKeys.snapshot);
      if (!current) return;
      const next = applyBatch(current, payload);
      if (!next) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
        return;
      }
      // Do side effects only for a newly applied batch: duplicate delivery must not replay sound.
      if (next === current) return;
      if (
        payload.events.some(
          (event) => event.kind === 'plugin_installed' || event.kind === 'plugin_uninstalled',
        )
      ) {
        void queryClient.invalidateQueries({ queryKey: ['compendia'] });
        void queryClient.invalidateQueries({ queryKey: ['compendium'] });
      }
      // Journal texts live in their own cache entries (the snapshot has only the index).
      for (const event of payload.events) {
        if (event.kind === 'sound_played') playSound(event.sound);
        else if (event.kind === 'sound_stopped') stopSound(event.asset_id);
        if (event.kind === 'journal_entry_created' || event.kind === 'journal_entry_updated') {
          queryClient.setQueryData(queryKeys.journalEntry(event.entry.id), event.entry);
        } else if (event.kind === 'journal_entry_deleted') {
          queryClient.removeQueries({ queryKey: queryKeys.journalEntry(event.id) });
        }
      }
      queryClient.setQueryData(queryKeys.snapshot, next);
    });
    return () => {
      for (const listener of [unlisten, unlistenSnapshot]) {
        void listener.then((stop) => {
          stop();
        });
      }
    };
  }, [queryClient]);
}

export interface IntentRequest {
  intent: Intent;
  /** The version the edit is based on (ADR-0003); omit for creations and moves. */
  expectedVersion?: number;
}

/**
 * Sends intents. A rejection becomes an {@link IntentRejected} error; on a stale version the
 * snapshot is refetched so the user sees the current data.
 */
export function useSubmitIntent() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ intent, expectedVersion }: IntentRequest) => {
      const outcome = await unwrap(
        commands.submitIntent({
          intent_id: uuidv7(),
          expected_version: expectedVersion ?? null,
          intent,
        }),
      );
      if (outcome.status === 'rejected') {
        throw new IntentRejected(outcome.reason);
      }
      return outcome.seq;
    },
    onError: (error) => {
      if (error instanceof IntentRejected && error.reason.code === 'stale_version') {
        void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
        void queryClient.invalidateQueries({ queryKey: ['journal'] });
      }
    },
  });
}

/** Full text of a journal entry; kept current by {@link useGameEventsSync}. */
export function useJournalEntry(id: string | null) {
  return useQuery({
    queryKey: queryKeys.journalEntry(id ?? ''),
    queryFn: () => unwrap(commands.journalEntry(id ?? '')),
    enabled: id !== null,
  });
}
