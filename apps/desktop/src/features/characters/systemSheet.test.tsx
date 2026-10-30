import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { CharacterEditor } from './CharacterEditor';
import type {
  CampaignInfo,
  CharacterDto,
  IntentEnvelope,
  PluginDto,
  Role,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const SRD: PluginDto = {
  id: 'srd5',
  version: '0.1.0',
  kind: 'system',
  name: 'SRD',
  license: 'CC-BY-4.0',
  attribution: null,
  archive_sha256: 'a'.repeat(64),
  system: {
    sheet_schema: {
      type: 'object',
      properties: {
        abilities: {
          type: 'object',
          properties: { str: { type: 'integer', minimum: 1, maximum: 30, default: 10 } },
        },
      },
    },
    sheet_layout: {
      type: 'section',
      children: [
        { type: 'field', path: 'abilities.str', label: { ru: 'Сила', en: 'Strength' } },
        { type: 'computed', name: 'str_mod', label: 'Modifier' },
        { type: 'roll', roll: 'str_check' },
      ],
    },
    computed: { str_mod: 'floor((@abilities.str - 10) / 2)' },
    initiative: '1d20',
    resources: [],
    health_thresholds: [75, 25, 0],
    rolls: [{ id: 'str_check', label: 'Strength check', dice: '1d20 + @str_mod' }],
    conditions: [
      { id: 'poisoned', label: { ru: 'Отравлен', en: 'Poisoned' } },
      { id: 'prone', label: { ru: 'Сбит с ног', en: 'Prone' } },
    ],
    inventory: null,
  },
};

const hero: CharacterDto = {
  id: '0190a000-0000-7000-8000-000000000301',
  kind: 'npc',
  name: 'Aria',
  owner: null,
  status: 'active',
  sheet_json: '{"abilities":{"str":16}}',
  conditions: ['prone'],
  version: 4,
};

function setup(role: Role, system: string | null = 'srd5', creature = false, kind = hero.kind) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [{ ...hero, kind }],
    users: [],
    journal: [],
    journal_folders: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    plugins: [
      creature && SRD.system
        ? {
            ...SRD,
            system: {
              ...SRD.system,
              creature_sheet_layout: {
                type: 'section',
                children: [{ type: 'field', path: 'abilities.str', label: 'Creature strength' }],
              },
            },
          }
        : SRD,
    ],
    system_plugin: system,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    switch (cmd) {
      case 'get_snapshot':
        return snapshot;
      case 'campaign_current':
        return info;
      case 'sheet_view':
        return { computed: { str_mod: 3 }, resources: [] };
      case 'submit_intent':
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        return { status: 'accepted', seq: 2 };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
  useSelection.setState({ characterId: hero.id, inspect: 'character' });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CharacterEditor />
    </QueryClientProvider>,
  );
  return sent;
}

it('uses the creature form for NPCs while PCs retain their own form', async () => {
  setup('gm', 'srd5', true, 'npc');
  expect(await screen.findByLabelText('Creature strength')).toHaveValue('16');
  expect(screen.queryByLabelText('Strength')).not.toBeInTheDocument();
});

it('keeps the PC form when the system also defines a creature form', async () => {
  setup('gm', 'srd5', true, 'pc');
  expect(await screen.findByLabelText('Strength')).toHaveValue('16');
  expect(screen.queryByLabelText('Creature strength')).not.toBeInTheDocument();
});

afterEach(() => {
  useSelection.getState().reset();
});

describe('CharacterEditor with a system plugin', () => {
  it('shows the plugin sheet with computed values; rolls and edits become intents', async () => {
    const sent = setup('gm');
    const strength = await screen.findByLabelText('Strength');
    expect(strength).toHaveValue('16');
    expect(await screen.findByText('3')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /Strength check/ }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'roll_dice',
      expression: '1d20 + @str_mod',
      character_id: hero.id,
      hidden: false,
      roll: 'str_check',
    });

    fireEvent.change(strength, { target: { value: '18' } });
    fireEvent.blur(strength);
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]).toMatchObject({
      expected_version: 4,
      intent: { kind: 'update_character', id: hero.id, name: null },
    });
    const intent = sent[1]?.intent as { sheet_json: string };
    expect(JSON.parse(intent.sheet_json)).toEqual({ abilities: { str: 18 } });

    // GM roles keep the raw JSON at hand.
    expect(screen.getByText('Sheet as JSON')).toBeInTheDocument();
  });

  it('shows conditions; GM roles take them off and add the system ones', async () => {
    const sent = setup('gm');
    const conditions = await screen.findByRole('region', { name: 'Conditions' });
    expect(conditions).toHaveTextContent('Prone');
    await userEvent.click(await screen.findByRole('button', { name: 'Remove Prone' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 4,
        intent: { kind: 'set_conditions', id: hero.id, conditions: [] },
      });
    });
    await userEvent.selectOptions(screen.getByLabelText('Add a condition…'), 'poisoned');
    await waitFor(() => {
      expect(sent[1]?.intent).toMatchObject({ conditions: ['prone', 'poisoned'] });
    });
  });

  it('without a system the raw JSON editor stays', async () => {
    setup('gm', null);
    expect(await screen.findByText('Save sheet')).toBeInTheDocument();
    expect(screen.queryByLabelText('Strength')).not.toBeInTheDocument();
  });
});
