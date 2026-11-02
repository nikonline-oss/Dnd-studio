import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { GmPanel } from './GmPanel';
import { useSelection } from '@/shared/state/selection';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';

function setup(role: Role = 'gm') {
  useSelection.getState().reset();
  const snapshot = {
    maps: [{ id: 'map', name: 'Crypt', grid: { cell_size: 70 } }],
    tokens: [
      { id: 'caster', map_id: 'map', character_id: 'c1', name: 'Mage', x: 0, y: 0 },
      { id: 'victim', map_id: 'map', character_id: 'c2', name: 'Goblin', x: 1120, y: 0 },
    ],
    characters: [
      { id: 'c1', name: 'Mage', version: 1 },
      { id: 'c2', name: 'Goblin', version: 1 },
    ],
    plugins: [
      {
        id: 'srd',
        name: 'Rules',
        feature_settings: {},
        system: {
          features: [{ id: 'fire', label: 'Enable fire', description: '', default_enabled: true }],
          mechanics: {
            gm_panel: [
              {
                id: 'r',
                kind: 'rules',
                label: 'Switches',
                description: 'Campaign wide',
                items: ['fire'],
              },
              { id: 'a', kind: 'actions', label: 'Tools', description: '', items: ['fireball'] },
              { id: 'c', kind: 'characters', label: 'Roster', description: '', items: [] },
            ],
            actions: [{ id: 'fireball', label: 'Fireball', feature: 'fire', radius_cells: 4 }],
          },
          conditions: [],
        },
      },
    ],
    system_plugin: 'srd',
    effects: [],
    sounds: [],
    combats: [],
    combatants: [],
  } as unknown as Snapshot;
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return { role, user_id: 'gm', path: '/c', name: 'C' };
    if (cmd === 'remote_current') return null;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 1 };
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <GmPanel />
    </QueryClientProvider>,
  );
  return { sent, client, snapshot };
}

it('renders plugin cards in declared order and changes rules through intents', async () => {
  const { sent } = setup('co_gm');
  await screen.findByText('Switches');
  expect(screen.getAllByText(/^(Switches|Tools|Roster)$/).map((e) => e.textContent)).toEqual([
    'Switches',
    'Tools',
    'Roster',
  ]);
  await userEvent.click(screen.getByRole('checkbox', { name: 'Enable fire' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({
      kind: 'configure_plugin_feature',
      id: 'srd',
      feature: 'fire',
      enabled: false,
    });
  });
});
it('requires review before executing and lists everyone in an area', async () => {
  const { sent } = setup();
  await userEvent.selectOptions(await screen.findByRole('combobox', { name: 'Actor' }), 'caster');
  await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Action' }), 'fireball');
  await userEvent.selectOptions(
    screen.getByRole('combobox', { name: 'Area center — token' }),
    'victim',
  );
  expect(sent).toEqual([]);
  await userEvent.click(screen.getByRole('button', { name: 'Review action' }));
  expect(screen.getByText('Characters in area: Mage, Goblin')).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: 'Confirm and apply' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({
      kind: 'execute_plugin_action',
      source: 'caster',
      action: 'fireball',
      target: null,
      x: 1120,
      y: 0,
    });
  });
});
it('hides all dashboard controls from a player', async () => {
  setup('player');
  expect(await screen.findByText('Available to the GM and co-GM.')).toBeInTheDocument();
  expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
  expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
});
it('opens a character sheet from the roster', async () => {
  setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Goblin' }));
  expect(useSelection.getState().active).toEqual({ kind: 'character', id: 'c2' });
});

it('invalidates area confirmation when a token moves', async () => {
  const { client, snapshot, sent } = setup();
  await userEvent.selectOptions(await screen.findByRole('combobox', { name: 'Actor' }), 'caster');
  await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Action' }), 'fireball');
  await userEvent.selectOptions(
    screen.getByRole('combobox', { name: 'Area center — token' }),
    'victim',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review action' }));
  expect(screen.getByRole('button', { name: 'Confirm and apply' })).toBeInTheDocument();
  act(() => {
    client.setQueryData(['snapshot'], {
      ...snapshot,
      tokens: snapshot.tokens.map((t) => (t.id === 'caster' ? { ...t, x: 100000 } : t)),
    });
  });
  await waitFor(() => {
    expect(screen.queryByRole('button', { name: 'Confirm and apply' })).not.toBeInTheDocument();
  });
  expect(sent).toEqual([]);
});
it('disables mutations while the remote session is disconnected', async () => {
  const { client, sent } = setup();
  await screen.findByRole('checkbox', { name: 'Enable fire' });
  act(() => {
    client.setQueryData(['remote'], { status: { state: 'reconnecting' } });
  });
  await waitFor(() => {
    expect(screen.getByRole('checkbox', { name: 'Enable fire' })).toBeDisabled();
  });
  expect(screen.getByRole('button', { name: 'Review action' })).toBeDisabled();
  expect(sent).toEqual([]);
});
