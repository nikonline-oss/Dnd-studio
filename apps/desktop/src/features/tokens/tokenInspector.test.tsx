import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { TokenInspector } from './TokenInspector';
import type { CampaignInfo, IntentEnvelope, Role, Snapshot, TokenDto } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const token: TokenDto = {
  id: 't1',
  map_id: 'm1',
  character_id: 'c1',
  asset_id: null,
  name: 'Restless Knight',
  x: 0,
  y: 0,
  rotation: 0,
  scale: 1,
  layer: 'tokens',
  visible: true,
  version: 4,
};

function setup(role: Role) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [token],
    characters: [
      {
        id: 'c1',
        kind: 'npc',
        name: 'Knight',
        owner: null,
        status: 'active',
        sheet_json: '{}',
        version: 1,
      },
    ],
    users: [],
    journal: [],
    journal_folders: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  useSelection.getState().selectToken('t1');
  render(
    <QueryClientProvider client={new QueryClient()}>
      <TokenInspector />
    </QueryClientProvider>,
  );
  return sent;
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('TokenInspector', () => {
  it('shows the token and its character', async () => {
    setup('gm');
    expect(await screen.findByText('Restless Knight')).toBeInTheDocument();
    expect(screen.getByText('Token of Knight')).toBeInTheDocument();
  });

  it('GM changes the layer, size and visibility with the token version', async () => {
    const sent = setup('gm');
    await userEvent.selectOptions(await screen.findByLabelText('Layer'), 'gm');
    await userEvent.selectOptions(screen.getByLabelText('Size'), '2');
    await userEvent.click(screen.getByRole('checkbox', { name: /Visible to players/ }));
    await waitFor(() => {
      expect(sent).toHaveLength(3);
    });
    expect(sent.map((e) => e.expected_version)).toEqual([4, 4, 4]);
    expect(sent.map((e) => e.intent)).toMatchObject([
      { kind: 'update_token', id: 't1', layer: 'gm', name: null, scale: null },
      { kind: 'update_token', id: 't1', scale: 2, layer: null },
      { kind: 'update_token', id: 't1', visible: false, layer: null },
    ]);
  });

  it('GM renames on blur; an unchanged name sends nothing', async () => {
    const sent = setup('gm');
    const name = await screen.findByLabelText('Name');
    await userEvent.click(name);
    await userEvent.tab();
    expect(sent).toHaveLength(0);
    await userEvent.clear(name);
    await userEvent.type(name, 'Fallen Knight{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({ kind: 'update_token', name: 'Fallen Knight' });
  });

  it('deleting needs a second click and clears the selection', async () => {
    const sent = setup('gm');
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    expect(sent).toHaveLength(0);
    await userEvent.click(screen.getByRole('button', { name: 'Delete?' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 4,
      intent: { kind: 'delete_token', id: 't1' },
    });
    expect(useSelection.getState()).toMatchObject({ tokenId: null, inspect: 'map' });
  });

  it('players see the token read-only', async () => {
    setup('player');
    expect(await screen.findByText('Restless Knight')).toBeInTheDocument();
    expect(screen.queryByLabelText('Layer')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Delete' })).not.toBeInTheDocument();
  });
});
