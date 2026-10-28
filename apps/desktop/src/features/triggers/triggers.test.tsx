import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { TriggerEditor } from './TriggerEditor';
import { TriggerList } from './TriggerList';
import type {
  CampaignInfo,
  IntentEnvelope,
  Snapshot,
  TriggerDto,
  ZoneDto,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const altar: ZoneDto = {
  id: id(30),
  map_id: id(20),
  name: 'Altar',
  shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
  version: 1,
};

const manual: TriggerDto = {
  id: id(40),
  definition: {
    name: 'Collapse',
    enabled: true,
    once: false,
    event: { type: 'manual' },
    tokens: [],
    condition: null,
    steps: [{ when: null, action: { type: 'chat', text: 'The ceiling falls.', gm_only: false } }],
  },
  fired: false,
  version: 3,
};

function setup(triggers: TriggerDto[] = [manual]) {
  const snapshot: Snapshot = {
    seq: 1,
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
    zones: [altar],
    triggers,
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role: 'gm' };
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
  render(
    <QueryClientProvider client={new QueryClient()}>
      <TriggerList />
      <TriggerEditor triggerId={manual.id} />
    </QueryClientProvider>,
  );
  return sent;
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('Triggers', () => {
  it('a new trigger is a disabled manual one, opened in a tab', async () => {
    const sent = setup([]);
    await userEvent.click(await screen.findByRole('button', { name: 'New trigger' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_trigger',
        definition: { enabled: false, event: { type: 'manual' } },
      });
    });
    expect(useSelection.getState().active?.kind).toBe('trigger');
  });

  it('edits the event with the selection form and saves the whole definition', async () => {
    const sent = setup();
    const form = await screen.findByRole('form', { name: 'Collapse' });
    await userEvent.selectOptions(within(form).getByLabelText('Event'), 'token_entered_zone');
    await userEvent.click(within(form).getByRole('button', { name: 'Pick: Zones' }));
    await userEvent.dblClick(screen.getByRole('option', { name: /Altar/ }));
    await userEvent.type(within(form).getByLabelText('Condition'), '@hp.current > 0');
    await userEvent.click(within(form).getByRole('button', { name: 'Add a step' }));
    await userEvent.click(within(form).getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: {
          kind: 'update_trigger',
          id: manual.id,
          definition: {
            event: { type: 'token_entered_zone', zone: altar.id },
            condition: '@hp.current > 0',
          },
        },
      });
    });
    const intent = sent[0]?.intent as { definition: TriggerDto['definition'] };
    expect(intent.definition.steps).toHaveLength(2);
  });

  it('runs a manual trigger with the GM button', async () => {
    const sent = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toEqual({ kind: 'fire_trigger', id: manual.id, token: null });
    });
  });
});
