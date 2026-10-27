import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { AppShell } from './AppShell';
import type { CampaignInfo, IpcError, Snapshot } from '@/shared/ipc/client';
import { Toaster, useToast } from '@/shared/ui/Toast';

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
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
};

const campaign: CampaignInfo = {
  name: 'C',
  path: '/c',
  user_id: '0190a000-0000-7000-8000-000000000001',
  role: 'gm',
};

function renderShell(checkpoint: () => unknown) {
  const calls: string[] = [];
  mockIPC(
    (cmd) => {
      calls.push(cmd);
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'app_info') return { version: '0.0.1' };
      if (cmd === 'campaign_checkpoint') return checkpoint();
      throw new Error(`unexpected command ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AppShell campaign={campaign} />
      <Toaster />
    </QueryClientProvider>,
  );
  return calls;
}

afterEach(() => {
  useToast.getState().hide();
});

describe('Ctrl+S', () => {
  it('forces a WAL checkpoint and shows "Saved"', async () => {
    const calls = renderShell(() => null);
    await userEvent.keyboard('{Control>}s{/Control}');
    await waitFor(() => {
      expect(calls).toContain('campaign_checkpoint');
    });
    expect(await screen.findByText('Saved')).toBeInTheDocument();
  });

  it('shows the error as a toast', async () => {
    renderShell(() => {
      const error: IpcError = { kind: 'internal', code: 'storage_error', message: 'locked' };
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- Tauri rejects with the plain error object
      throw error;
    });
    await userEvent.keyboard('{Control>}s{/Control}');
    const toast = await screen.findByText('Could not save data');
    expect(toast).toHaveAttribute('data-tone', 'error');
  });
});
