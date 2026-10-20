import { useEffect, useState } from 'react';
import { events, type SaveStatusChanged } from '@/shared/ipc/client';

/** Live autosave status of the open campaign (ADR-0011). */
export function useSaveStatus(): SaveStatusChanged {
  const [status, setStatus] = useState<SaveStatusChanged>({ dirty: false, error: null });
  useEffect(() => {
    const unlisten = events.saveStatusChanged.listen(({ payload }) => {
      setStatus(payload);
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);
  return status;
}
