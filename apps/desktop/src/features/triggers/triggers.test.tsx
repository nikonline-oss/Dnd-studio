import { useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { TemplateDialog, fillTemplate } from './TemplateDialog';
import { TriggerEditor } from './TriggerEditor';
// eslint-disable-next-line no-restricted-imports -- integration test: trigger creation starts on the board
import { BoardView } from '@/features/boards/BoardView';
import type {
  CampaignInfo,
  PluginDto,
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
  color: 0x9b7bff,
  label: null,
  shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
  version: 1,
};

const manual: TriggerDto = {
  id: id(40),
  definition: {
    name: 'Collapse',
    map_ids: [id(20)],
    enabled: true,
    once: false,
    event: { type: 'manual' },
    tokens: [],
    condition: null,
    steps: [{ when: null, action: { type: 'chat', text: 'The ceiling falls.', gm_only: false } }],
  },
  fired: false,
  fired_maps: [],
  version: 3,
};

function setup(
  triggers: TriggerDto[] = [manual],
  plugins: PluginDto[] = [],
  patch: Partial<Snapshot> = {},
) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [
      {
        id: id(20),
        name: 'Crypt',
        width: 100,
        height: 100,
        grid: { kind: 'square', cell_size: 10, offset_x: 0, offset_y: 0 },
        background_asset_id: null,
        visible_to_players: false,
        version: 1,
      },
    ],
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    journal_folders: [],
    plugins,
    system_plugin: null,
    boards: [{ id: id(10), name: 'Plot', sort_order: 0, visible_to_players: false, version: 1 }],
    board_nodes: triggers.map((tr) => ({
      id: id(11),
      board_id: id(10),
      content: { type: 'trigger', id: tr.id },
      x: 0,
      y: 0,
      width: 3200,
      height: 1600,
      visibility: 'gm',
      version: 1,
    })),
    board_edges: [],
    zones: [altar],
    triggers,
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    ...patch,
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role: 'gm' };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return null;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  return sent;
}

function Harness() {
  const [template, setTemplate] = useState(false);
  return (
    <>
      <BoardView
        boardId={id(10)}
        onTriggerTemplate={() => {
          setTemplate(true);
        }}
      />
      <TriggerEditor triggerId={manual.id} />
      {template && (
        <TemplateDialog
          boardId={id(10)}
          onClose={() => {
            setTemplate(false);
          }}
        />
      )}
    </>
  );
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('Triggers', () => {
  it('a new trigger is a disabled manual one, opened in a tab', async () => {
    const sent = setup([]);
    await userEvent.click(await screen.findByRole('button', { name: 'New trigger on the board' }));
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
    await userEvent.selectOptions(within(form).getByLabelText('Zone name'), altar.id);
    await userEvent.type(within(form).getByLabelText('Condition'), '@hp.current > 0');
    await userEvent.click(within(form).getByRole('button', { name: 'Add a step' }));
    const texts = within(form).getAllByLabelText('Text');
    const secondText = texts[1];
    if (!secondText) throw new Error('expected a text field per step');
    await userEvent.type(secondText, 'The passage opens.');
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
    const form = await screen.findByRole('form', { name: 'Collapse' });
    await userEvent.click(within(form).getByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toEqual({ kind: 'fire_trigger', id: manual.id, token: null });
    });
  });

  it('binds one map in a dropdown, several in checkboxes, with a preview', async () => {
    const harbor: Snapshot['maps'][number] = {
      id: id(21),
      name: 'Harbor',
      width: 100,
      height: 100,
      grid: { kind: 'square', cell_size: 10, offset_x: 0, offset_y: 0 },
      background_asset_id: null,
      visible_to_players: true,
      version: 1,
    };
    const cryptWithBackground: Snapshot['maps'][number] = {
      ...harbor,
      id: id(20),
      name: 'Crypt',
      visible_to_players: false,
      background_asset_id: 'bg',
    };
    const background: Snapshot['assets'][number] = {
      id: 'bg',
      usage: 'map',
      sha256: 'b'.repeat(64),
      mime: 'image/png',
      size_bytes: 10,
      original_name: 'bg.png',
      width: 10,
      height: 10,
      display_sha256: null,
      thumbnail_sha256: 't'.repeat(64),
      tiles: [],
      folder_id: null,
      version: 1,
    };
    const sent = setup([manual], [], {
      maps: [cryptWithBackground, harbor],
      assets: [background],
    });
    // assetUrl goes through Tauri's convertFileSrc.
    (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__[
      'convertFileSrc'
    ] = (hash: string) => `asset://${hash}`;
    const form = await screen.findByRole('form', { name: 'Collapse' });
    // One bound map: a dropdown (not checkboxes), its preview is the background thumbnail.
    const mapSelect = within(form).getByLabelText('Map');
    expect(mapSelect).toHaveValue(id(20));
    expect(within(form).queryByRole('checkbox', { name: 'Crypt' })).not.toBeInTheDocument();
    const preview = within(form).getByRole('img', { name: 'Crypt' });
    expect(preview.getAttribute('src')).toContain(background.thumbnail_sha256);
    // Several maps: checkboxes; ticking Harbor keeps Crypt and both are saved.
    await userEvent.click(within(form).getByRole('radio', { name: 'Several maps' }));
    const harborBox = within(form).getByRole('checkbox', { name: 'Harbor' });
    expect(harborBox).not.toBeChecked();
    await userEvent.click(harborBox);
    await userEvent.click(within(form).getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'update_trigger',
        definition: { map_ids: [id(20), id(21)] },
      });
    });
  });
});

describe('Trigger templates', () => {
  it('fill every blank, inside strings too', () => {
    expect(
      fillTemplate(
        { zone: '{{zone}}', condition: '@total >= {{dc}}', steps: [{ text: '{{zone}}!' }], n: 1 },
        { zone: 'z1', dc: '13' },
      ),
    ).toEqual({ zone: 'z1', condition: '@total >= 13', steps: [{ text: 'z1!' }], n: 1 });
  });

  it('fill whole-string blanks of number parameters as numbers', () => {
    const params = [{ id: 'turns', label: 'Turns', kind: 'number' }];
    expect(
      fillTemplate(
        { duration: { type: 'turns', turns: '{{turns}}' }, label: '{{turns}} left' },
        { turns: '3' },
        params,
      ),
    ).toEqual({ duration: { type: 'turns', turns: 3 }, label: '3 left' });
    // Without the kinds (legacy callers) every blank stays a string.
    expect(fillTemplate({ turns: '{{turns}}' }, { turns: '3' })).toEqual({ turns: '3' });
    // A non-numeric value stays a string; the core rejects it.
    expect(fillTemplate({ turns: '{{turns}}' }, { turns: 'many' }, params)).toEqual({
      turns: 'many',
    });
  });

  it('create a trigger from a plugin template', async () => {
    const plugin: PluginDto = {
      id: 'srd5',
      version: '0.1.0',
      kind: 'content',
      name: 'SRD',
      license: 'MIT',
      attribution: null,
      archive_sha256: 'a'.repeat(64),
      system: null,
      trigger_templates: [
        {
          id: 'hidden_find',
          name: 'Hidden find',
          description: null,
          params: [
            { id: 'zone', label: 'Zone', kind: 'zone' },
            { id: 'dc', label: 'DC', kind: 'number' },
          ],
          definition: {
            name: 'Find',
            enabled: true,
            once: true,
            event: { type: 'roll_in_zone', zone: '{{zone}}', rolls: [] },
            tokens: [],
            condition: '@total >= {{dc}}',
            steps: [{ when: null, action: { type: 'chat', text: 'Found!', gm_only: false } }],
          },
        },
      ],
    };
    const sent = setup([], [plugin]);
    await userEvent.click(await screen.findByRole('button', { name: 'From a plugin template…' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.click(within(dialog).getByRole('option', { name: /Hidden find/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Pick: Zone' }));
    await userEvent.dblClick(screen.getByRole('option', { name: /Altar/ }));
    await userEvent.type(within(dialog).getByLabelText('DC'), '13');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create trigger' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_trigger',
        definition: {
          event: { type: 'roll_in_zone', zone: altar.id },
          condition: '@total >= 13',
        },
      });
    });
  });
});
