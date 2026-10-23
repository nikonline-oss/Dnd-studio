import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { CampaignScreen } from './CampaignScreen';
import type { CampaignInfo, IpcError } from '@/shared/ipc/client';

const restored: CampaignInfo = {
  name: 'Crypt',
  path: '/games/Crypt',
  user_id: '0190a000-0000-7000-8000-000000000001',
  role: 'gm',
};

describe('CampaignScreen', () => {
  it('offers restoring from a backup when the campaign is corrupted', async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === 'campaign_open') {
        const error: IpcError = {
          kind: 'recoverable',
          code: 'campaign_corrupt',
          message: 'malformed',
        };
        // Tauri rejects failed commands with the plain serialized error object, not an Error.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw error;
      }
      if (cmd === 'campaign_restore_backup') return restored;
      throw new Error(`unexpected command ${cmd}`);
    });
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <CampaignScreen />
      </QueryClientProvider>,
    );

    await userEvent.click(screen.getByRole('button', { name: 'Open…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The campaign file is damaged');

    await userEvent.click(screen.getByRole('button', { name: 'Restore from latest backup' }));
    await screen.findByRole('button', { name: 'Open…' });
    expect(calls).toEqual(['campaign_open', 'campaign_restore_backup']);
    expect(client.getQueryData(['campaign'])).toEqual(restored);
  });

  it('a cancelled dialog changes nothing', async () => {
    mockIPC((cmd) => {
      if (cmd === 'campaign_open') return null;
      throw new Error(`unexpected command ${cmd}`);
    });
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <CampaignScreen />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Open…' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(client.getQueryData(['campaign'])).toBeUndefined();
  });

  it('imports an archive and opens it; a broken archive is explained', async () => {
    let broken = true;
    mockIPC((cmd) => {
      if (cmd !== 'campaign_import') throw new Error(`unexpected command ${cmd}`);
      if (broken) {
        const error: IpcError = { kind: 'user', code: 'invalid_archive', message: 'bad' };
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw error;
      }
      return restored;
    });
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <CampaignScreen />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Import campaign…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Not a DndStudio campaign archive');

    broken = false;
    await userEvent.click(screen.getByRole('button', { name: 'Import campaign…' }));
    await vi.waitFor(() => {
      expect(client.getQueryData(['campaign'])).toEqual(restored);
    });
  });
});
