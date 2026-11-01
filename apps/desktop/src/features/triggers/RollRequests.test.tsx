import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { RollRequests } from './RollRequests';
import { useAttention } from '@/shared/state/attention';
import type { CampaignInfo, IntentEnvelope, RollRequestDto, Snapshot } from '@/shared/ipc/client';

const character = (id: string, owner: string | null) => ({
  id,
  kind: 'pc' as const,
  name: id === 'c1' ? 'Aria' : 'Borin',
  owner,
  status: 'active' as const,
  sheet_json: '{}',
  version: 1,
});

const request = (
  id: string,
  characterId: string,
  status: RollRequestDto['status'],
): RollRequestDto => ({
  id,
  label: 'Poisoned dart',
  trigger: 't1',
  character: characterId,
  token: null,
  dice: '1d20 + 5',
  hit: null,
  resource: 'hp',
  damage_dice: '2d6',
  heal: false,
  status,
  total: status === 'rolled' ? 12 : null,
  delta: status === 'rolled' ? -7 : null,
  version: 3,
});

function baseSnapshot(): Snapshot {
  return {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [character('c1', 'u1'), character('c2', 'u2')],
    users: [],
    journal: [],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    roll_requests: [
      request('r1', 'c1', 'pending'),
      request('r2', 'c2', 'pending'),
      request('r3', 'c1', 'rolled'),
      request('r4', 'c2', 'cancelled'),
    ],
  };
}

function setup(role: CampaignInfo['role'], snapshot = baseSnapshot()) {
  const sent: IntentEnvelope[] = [];
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role };
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RollRequests />
    </QueryClientProvider>,
  );
  return sent;
}

afterEach(() => {
  useAttention.setState({ requestsOpen: false, soundEnabled: true });
});

describe('RollRequests', () => {
  it('shows a player only their own characters and lets them roll', async () => {
    const sent = setup('player');
    await userEvent.click(await screen.findByRole('button', { name: 'Rolls to make' }));
    expect(screen.getAllByText('Aria · 1d20 + 5').length).toBeGreaterThan(0);
    expect(screen.queryByText('Borin · 1d20 + 5')).not.toBeInTheDocument();
    await userEvent.click(screen.getAllByRole('button', { name: 'Roll' })[0] as HTMLElement);
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'fulfill_roll_request', id: 'r1' },
    });
    expect(screen.queryByRole('button', { name: 'Dismiss' })).not.toBeInTheDocument();
  });

  it('gives the GM every request with dismiss and undo', async () => {
    const sent = setup('gm');
    await userEvent.click(await screen.findByRole('button', { name: 'Rolls to make' }));
    expect(screen.getAllByText('Aria · 1d20 + 5').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Borin · 1d20 + 5').length).toBeGreaterThan(0);
    expect(screen.getByText('rolled 12')).toBeInTheDocument();
    expect(screen.getByText('cancelled')).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole('button', { name: 'Dismiss' })[0] as HTMLElement);
    await userEvent.click(screen.getByRole('button', { name: 'Undo damage' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent.map((e) => e.intent)).toMatchObject([
      { kind: 'cancel_roll_request', id: 'r1' },
      { kind: 'cancel_roll_request', id: 'r3' },
    ]);
  });

  it('stays hidden for spectators', () => {
    setup('spectator');
    expect(screen.queryByRole('button', { name: 'Rolls to make' })).not.toBeInTheDocument();
  });
  it('does not offer undo on the deleted character of a historical roll', async () => {
    const snapshot = baseSnapshot();
    snapshot.characters = [];
    snapshot.roll_requests = [request('old', 'gone', 'rolled')];
    setup('gm', snapshot);
    await userEvent.click(await screen.findByRole('button', { name: 'Rolls to make' }));
    expect(screen.queryByRole('button', { name: 'Undo damage' })).not.toBeInTheDocument();
  });
});
