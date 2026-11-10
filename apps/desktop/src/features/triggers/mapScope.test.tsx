import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { TriggerEditor } from './TriggerEditor';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';

function setup(role: Role = 'gm', rejected = false, spent = false) {
  const trigger = {
    id: 'trigger',
    version: 3,
    fired: spent,
    fired_maps: spent ? ['map-a'] : [],
    definition: {
      name: 'Alarm',
      enabled: true,
      once: true,
      map_ids: ['map-a', 'map-b'],
      event: { type: 'manual' as const },
      tokens: [],
      condition: null,
      steps: [{ when: null, action: { type: 'chat' as const, text: 'Alarm!', gm_only: true } }],
    },
  };
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: ['map-a', 'map-b'].map((id) => ({
      id,
      name: id === 'map-a' ? 'Crypt' : 'Tower',
      background_asset_id: null,
      width: 100,
      height: 100,
      visible_to_players: false,
      version: 1,
      grid: { kind: 'square', cell_size: 10, offset_x: 0, offset_y: 0 },
    })),
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    journal_folders: [],
    assets: [],
    plugins: [],
    system_plugin: null,
    boards: [{ id: 'board', name: 'Plot', sort_order: 0, visible_to_players: false, version: 1 }],
    board_nodes: [
      {
        id: 'card',
        board_id: 'board',
        content: { type: 'trigger', id: 'trigger' },
        x: 0,
        y: 0,
        width: 3200,
        height: 1600,
        visibility: 'gm',
        version: 1,
      },
    ],
    board_edges: [],
    zones: [],
    triggers: [trigger],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const sent: IntentEnvelope[] = [];
  mockIPC((command, args) => {
    if (command === 'campaign_current') return { name: 'C', path: '/c', user_id: 'gm', role };
    if (command === 'get_snapshot') return snapshot;
    if (command === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return rejected
        ? { status: 'rejected', reason: { code: 'invalid', field: 'condition' } }
        : { status: 'accepted', seq: 2 };
    }
    throw new Error(command);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <TriggerEditor triggerId="trigger" />
    </QueryClientProvider>,
  );
  return { sent, client };
}

describe('Map-scoped trigger editor', () => {
  it('explains unsaved changes instead of calling the trigger spent', async () => {
    setup();
    await userEvent.type(await screen.findByRole('textbox', { name: 'Trigger name' }), ' edited');
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    expect(screen.getByText('Save changes before running.')).toBeInTheDocument();
    expect(screen.queryByText('Already fired')).not.toBeInTheDocument();
  });

  it('explains a disabled trigger and does not send an intent', async () => {
    const { client, sent } = setup();
    await screen.findByRole('form', { name: 'Alarm' });
    act(() => client.setQueryData<Snapshot>(['snapshot'], (s) => s && ({
      ...s, triggers: s.triggers.map(t => ({...t, definition:{...t.definition, enabled:false}})),
    })));
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    expect(screen.getByText('Enable the trigger before running.')).toBeInTheDocument();
    expect(sent).toEqual([]);
  });
  it('saves selected maps using the trigger version and explains automatic board links', async () => {
    const { sent } = setup('co_gm');
    const crypt = await screen.findByRole('checkbox', { name: 'Crypt' });
    expect(crypt).toBeChecked();
    await userEvent.click(crypt);
    expect(screen.getByText(/Map cards and connections are created/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: { kind: 'update_trigger', definition: { map_ids: ['map-b'] } },
      });
    });
  });

  it('explains invalid names and empty map selection and prevents submission', async () => {
    const { sent } = setup();
    await userEvent.clear(await screen.findByRole('textbox', { name: 'Trigger name' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'Crypt' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'Tower' }));
    expect(screen.getByText('Enter a trigger name.')).toBeInTheDocument();
    expect(screen.getByText('Select at least one map.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(sent).toEqual([]);
  });

  it('explains incomplete step inputs before sending an intent', async () => {
    const { sent } = setup();
    await screen.findByRole('form', { name: 'Alarm' });
    await userEvent.click(screen.getByRole('button', { name: 'Add a step' }));
    expect(screen.getByText(/Step 2: complete/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(sent).toEqual([]);
  });

  it('runs remaining maps even when the old global fired flag is true', async () => {
    const { sent } = setup('gm', false, true);
    const run = await screen.findByRole('button', { name: 'Run' });
    expect(run).toBeEnabled();
    await userEvent.click(run);
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({ kind: 'fire_trigger', token: null });
    });
  });

  it.each(['player', 'spectator'] as const)(
    '%s cannot edit or run a leaked trigger',
    async (role) => {
      const { client, sent } = setup(role);
      await waitFor(() => {
        expect(client.getQueryData(['snapshot'])).toBeDefined();
      });
      expect(screen.queryByRole('form')).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Run' })).not.toBeInTheDocument();
      expect(sent).toEqual([]);
    },
  );

  it('requires confirmation to delete the trigger from its board editor', async () => {
    const { sent } = setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Delete trigger' }));
    expect(sent).toEqual([]);
    const dialog = screen.getByRole('alertdialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete?' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: { kind: 'delete_trigger', id: 'trigger' },
      });
    });
  });
});
