import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { commands, events, unwrap, type RemoteInfo } from '@/shared/ipc/client';

export const remoteKey = ['remote'] as const;

/** The connected session server, or `null` in local play (ADR-0022). */
export function useRemote(): RemoteInfo | null {
  const { data } = useQuery({
    queryKey: remoteKey,
    queryFn: () => unwrap(commands.remoteCurrent()),
  });
  return data ?? null;
}

/** Subscribes once (in the app) to connection changes and keeps {@link useRemote} current. */
export function useRemoteSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlisten = events.remoteStatusChanged.listen(({ payload }) => {
      queryClient.setQueryData<RemoteInfo | null>(remoteKey, (info) =>
        info ? { ...info, status: payload } : info,
      );
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, [queryClient]);
}
