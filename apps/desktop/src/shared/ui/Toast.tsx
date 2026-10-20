import { useEffect } from 'react';
import { create } from 'zustand';

/** How long a toast stays visible. */
export const TOAST_MS = 2000;

interface ToastState {
  message: string | null;
  tone: 'info' | 'error';
  /** Bumped on every show so the same message restarts the timer. */
  serial: number;
  show: (message: string, tone?: 'info' | 'error') => void;
  hide: () => void;
}

/** A single transient notification, shown by any feature through `@/shared`. */
export const useToast = create<ToastState>()((set) => ({
  message: null,
  tone: 'info',
  serial: 0,
  show: (message, tone = 'info') => {
    set((s) => ({ message, tone, serial: s.serial + 1 }));
  },
  hide: () => {
    set({ message: null });
  },
}));

/** Renders the current toast; mount once in the app shell. */
export function Toaster() {
  const { message, tone, serial, hide } = useToast();

  useEffect(() => {
    if (message === null) return;
    const timer = setTimeout(hide, TOAST_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [message, serial, hide]);

  return (
    <div className="toaster" role="status" aria-live="polite">
      {message !== null && (
        <div className="toast" data-tone={tone}>
          {message}
        </div>
      )}
    </div>
  );
}
