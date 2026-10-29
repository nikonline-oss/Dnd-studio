import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useRemote } from '@/shared/game/useRemote';
import { commands, events, unwrap } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

const pendingKey = ['offline-edits'] as const;

/** Durable GM edits and explicit conflict resolution (ADR-0004). */
export function OfflineEdits({ isGm, campaignPath }: { isGm: boolean; campaignPath: string }) {
  const { t } = useTranslation();
  const remote = useRemote();
  const client = useQueryClient();
  const enabled = isGm && (remote === null || remote.hosted);
  const online = enabled && remote?.status.state === 'online';
  const pending = useQuery({
    queryKey: [...pendingKey, campaignPath, remote?.server, remote?.session_id],
    queryFn: () => unwrap(commands.remotePending()),
    enabled,
  });
  const { mutate: sync, error: syncError } = useMutation({
    mutationFn: () => unwrap(commands.remoteSync()),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: pendingKey });
    },
  });
  useEffect(() => {
    if (online) sync();
  }, [online, sync]);
  useEffect(() => {
    if (!enabled) return;
    const unlisten = events.gameSnapshotReplaced.listen(() => {
      if (online) sync();
      else void client.invalidateQueries({ queryKey: pendingKey });
    });
    const unlistenEdits = events.gameEventsEmitted.listen(() => {
      if (!online) void client.invalidateQueries({ queryKey: pendingKey });
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
      void unlistenEdits.then((stop) => {
        stop();
      });
    };
  }, [enabled, online, sync, client]);
  const resolve = useMutation({
    mutationFn: ({ id, apply }: { id: string; apply: boolean }) =>
      unwrap(commands.remoteResolve(id, apply)),
    onSuccess: () => {
      sync();
    },
  });
  if (
    !enabled ||
    (!pending.data?.edits.length &&
      (!pending.data?.binding || online) &&
      !syncError &&
      !pending.error)
  )
    return null;
  return (
    <section className="offline-edits" aria-label={t('offline.title')}>
      <h2>
        {t('offline.title')} ({pending.data?.edits.length ?? 0})
      </h2>
      {!online && pending.data?.binding && <p role="status">{t('offline.copy')}</p>}
      {(pending.data?.edits ?? []).map((edit, index) => (
        <details key={edit.id}>
          <summary>
            {t('offline.edit', { id: index + 1 })} —{' '}
            {t(edit.conflict ? 'offline.conflict' : 'offline.pending')}
          </summary>
          <div className="offline-edit-comparison">
            <div>
              <h3>{t('offline.mine')}</h3>
              <pre>{JSON.stringify(edit.envelope.intent, null, 2)}</pre>
            </div>
            <div>
              <h3>{t('offline.server')}</h3>
              <pre>{edit.server_data ?? t(online ? 'offline.missing' : 'offline.unavailable')}</pre>
            </div>
          </div>
          {edit.conflict && (
            <>
              <p>{t('offline.reason', { code: edit.conflict.code })}</p>
              <button
                type="button"
                disabled={!online || resolve.isPending || edit.conflict.code !== 'stale_version'}
                onClick={() => {
                  resolve.mutate({ id: edit.id, apply: true });
                }}
              >
                {t('offline.apply')}
              </button>
              <button
                type="button"
                disabled={!online || resolve.isPending}
                onClick={() => {
                  resolve.mutate({ id: edit.id, apply: false });
                }}
              >
                {t('offline.discard')}
              </button>
            </>
          )}
        </details>
      ))}
      <ErrorText error={pending.error ?? resolve.error ?? syncError} />
    </section>
  );
}
