import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { OfflineEdits } from './OfflineEdits';
import { remoteKey } from '@/shared/game/useRemote';
import type { PendingEdit, RemoteInfo } from '@/shared/ipc/client';

describe('Offline edits', () => {
  it('marks a local campaign that is still hosted even when no edits are queued', async () => {
    mockIPC(
      (cmd) => {
        if (cmd === 'remote_current') return null;
        if (cmd === 'remote_pending') return { binding: 'https://server/session', edits: [] };
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <OfflineEdits isGm campaignPath="/offline" />
      </QueryClientProvider>,
    );
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Offline copy of a hosted campaign',
    );
  });
  it('syncs on connection and lets the GM rebase or discard a conflict', async () => {
    const remote: RemoteInfo = {
      server: 'http://localhost',
      session_id: 's',
      session_name: 'C',
      hosted: true,
      role: 'gm',
      status: { state: 'online' },
    };
    const edits: PendingEdit[] = [
      {
        id: '1',
        envelope: {
          intent_id: 'intent',
          expected_version: 1,
          intent: { kind: 'update_campaign_meta', name: 'My name', description: null },
        },
        conflict: { code: 'stale_version', current: 2 },
        server_data: '{"name":"Server name"}',
      },
    ];
    let syncs = 0;
    const resolved: unknown[] = [];
    mockIPC(
      (cmd, args) => {
        if (cmd === 'remote_current') return remote;
        if (cmd === 'remote_pending') return { binding: 'session', edits };
        if (cmd === 'remote_sync') {
          syncs++;
          return null;
        }
        if (cmd === 'remote_resolve') {
          resolved.push(args);
          return null;
        }
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(remoteKey, remote);
    render(
      <QueryClientProvider client={client}>
        <OfflineEdits isGm campaignPath="/c" />
      </QueryClientProvider>,
    );
    await userEvent.click(await screen.findByText(/Edit 1/));
    expect(screen.getByText(/Server name/)).toBeInTheDocument();
    await waitFor(() => {
      expect(syncs).toBeGreaterThan(0);
    });
    await userEvent.click(screen.getByRole('button', { name: 'Apply mine over server' }));
    await waitFor(() => {
      expect(resolved).toContainEqual({ id: '1', applyMine: true });
    });
    await userEvent.click(screen.getByRole('button', { name: 'Discard' }));
    await waitFor(() => {
      expect(resolved).toContainEqual({ id: '1', applyMine: false });
    });
  });
});
