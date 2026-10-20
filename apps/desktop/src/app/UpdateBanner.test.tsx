import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { emit } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it } from 'vitest';
import { MenuBar } from './MenuBar';
import { UpdateBanner } from './UpdateBanner';
import type { IpcError } from '@/shared/ipc/client';

function withQuery(node: ReactNode) {
  return <QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>;
}

describe('UpdateBanner', () => {
  it('appears on update-available and installs only on click', async () => {
    const calls: string[] = [];
    mockIPC(
      (cmd) => {
        calls.push(cmd);
        if (cmd === 'update_install') return null;
        throw new Error(`unexpected command ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    render(withQuery(<UpdateBanner />));
    expect(screen.queryByRole('status')).not.toBeInTheDocument();

    await act(() =>
      emit('update-available', { version: '0.2.0', current_version: '0.1.0', notes: null }),
    );
    expect(await screen.findByRole('status')).toHaveTextContent('DndStudio 0.2.0 is available');
    expect(calls).toEqual([]);

    await userEvent.click(screen.getByRole('button', { name: 'Install and restart' }));
    await waitFor(() => {
      expect(calls).toEqual(['update_install']);
    });
  });
});

describe('Help → Check for updates', () => {
  it('explains when the build has no updater', async () => {
    mockIPC((cmd) => {
      if (cmd === 'update_check') {
        const error: IpcError = { kind: 'user', code: 'updater_disabled', message: 'dev build' };
        // eslint-disable-next-line @typescript-eslint/only-throw-error -- Tauri rejects with the plain error object
        throw error;
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    render(withQuery(<MenuBar breadcrumbs={['C']} />));
    await userEvent.click(screen.getByRole('button', { name: 'Help' }));
    await userEvent.click(screen.getByRole('menuitem', { name: /Check for updates/ }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Updates are not configured in this build',
    );
  });

  it('reports being up to date', async () => {
    mockIPC((cmd) => {
      if (cmd === 'update_check') return null;
      throw new Error(`unexpected command ${cmd}`);
    });
    render(withQuery(<MenuBar breadcrumbs={['C']} />));
    await userEvent.click(screen.getByRole('button', { name: 'Help' }));
    await userEvent.click(screen.getByRole('menuitem', { name: /Check for updates/ }));
    expect(await screen.findByRole('status')).toHaveTextContent('You have the latest version');
  });
});
