import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { MapList } from './MapList';
import type { IntentEnvelope, IntentOutcome, Snapshot } from '@/shared/ipc/client';

const snapshot: Snapshot = {
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [
    {
      id: '0190a000-0000-7000-8000-000000000001',
      name: 'Crypt',
      background_asset_id: null,
      width: 4000,
      height: 3000,
      grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
      visible_to_players: false,
      version: 3,
    },
  ],
  tokens: [],
  characters: [],
  users: [],
};

function renderWithIpc(outcome: IntentOutcome) {
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return outcome;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MapList />
    </QueryClientProvider>,
  );
  return sent;
}

describe('MapList', () => {
  it('shows maps from the snapshot', async () => {
    renderWithIpc({ status: 'accepted', seq: 2 });
    expect(await screen.findByText('Crypt')).toBeInTheDocument();
  });

  it('sends create_map with a client-generated v7 id', async () => {
    const sent = renderWithIpc({ status: 'accepted', seq: 2 });
    await userEvent.click(await screen.findByRole('button', { name: 'Add map' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    const envelope = sent[0];
    expect(envelope?.expected_version).toBeNull();
    expect(envelope?.intent).toMatchObject({ kind: 'create_map', name: 'New map 2' });
    expect(envelope?.intent_id).toMatch(/^[0-9a-f-]{14}7/);
  });

  it('passes the entity version on updates (ADR-0003)', async () => {
    const sent = renderWithIpc({ status: 'accepted', seq: 2 });
    await userEvent.click(await screen.findByRole('checkbox'));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'update_map', visible_to_players: true },
    });
  });

  it('shows a localized message when the core rejects an intent', async () => {
    renderWithIpc({ status: 'rejected', reason: { code: 'stale_version', current: 4 } });
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Someone changed this before you');
  });
});
