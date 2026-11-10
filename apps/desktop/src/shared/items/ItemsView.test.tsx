import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ItemsView } from './ItemsView';
import type { ItemDto, Snapshot } from '@/shared/ipc/client';

const state = vi.hoisted(() => ({
  role: 'gm',
  mutate: vi.fn(),
}));

vi.mock('@/shared/game/useGame', () => ({
  useCampaign: () => ({ data: { role: state.role, user_id: 'u1' } }),
  useSnapshot: () => ({ data: snapshot }),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
vi.mock('@/shared/i18n/label', () => ({
  labelText: (label: unknown) =>
    typeof label === 'string' ? label : ((label as { en?: string }).en ?? ''),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, vars?: Record<string, string>) =>
      vars ? `${key}:${Object.values(vars).join(',')}` : key,
    i18n: { resolvedLanguage: 'en', language: 'en' },
  }),
}));
vi.mock('@/shared/sheet/CharacterSheet', () => ({
  CharacterSheet: () => <p>plugin sheet</p>,
}));

const ring: ItemDto = {
  id: 'i1',
  kind: 'item',
  name: 'Ring of protection',
  type_id: null,
  is_template: false,
  template: null,
  owner: 'c1',
  visible_to_players: true,
  description: 'A worn band.',
  state: null,
  effects: [{ path: 'ac', add: 1, minimum: null, maximum: null }],
  grants: ['poisoned'],
  values: [{ key: 'armor_bonus', value: 2 }],
  sheet_json: '{}',
  version: 3,
};

const skill: ItemDto = {
  ...ring,
  id: 's1',
  kind: 'skill',
  name: 'Fire bolt',
  owner: null,
  effects: [],
  grants: [],
  values: [],
  description: 'A ranged spell attack.',
};

const wand: ItemDto = {
  ...ring,
  id: 'w1',
  name: 'Wand of sparks',
  type_id: 'equipment',
  sheet_json: '{"charges":3}',
  effects: [],
  grants: [],
  values: [],
};

const snapshot: Snapshot = {
  seq: 1,
  campaign: {
    name: 'C',
    description: '',
    version: 1,
    item_states: [
      { id: 'ok', label: { en: 'Intact' }, active: true },
      { id: 'broken', label: { en: 'Broken' }, active: false },
    ],
  },
  maps: [],
  tokens: [],
  characters: [
    {
      id: 'c1',
      kind: 'pc',
      name: 'Mira',
      owner: 'u1',
      status: 'active',
      sheet_json: '{}',
      version: 1,
    },
    {
      id: 'c2',
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
  plugins: [
    {
      id: 'sys',
      kind: 'system',
      name: { en: 'System' },
      license: 'MIT',
      version: '1',
      archive_sha256: '0'.repeat(64),
      system: {
        sheet_schema: {
          properties: { ac: { type: 'integer' }, level: { type: 'integer' } },
        },
        conditions: [
          { id: 'poisoned', label: { en: 'Poisoned' } },
          { id: 'prone', label: { en: 'Prone' } },
        ],
        mechanics: {
          item_types: [
            {
              id: 'equipment',
              label: { en: 'Equipment' },
              sheet_schema: { properties: { charges: { type: 'integer' } } },
              sheet_layout: { type: 'field', path: 'charges', label: 'Charges' },
            },
          ],
        },
      },
    } as unknown as Snapshot['plugins'][number],
  ],
  system_plugin: 'sys',
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
  items: [ring, skill, wand],
};

beforeEach(() => {
  state.role = 'gm';
  state.mutate.mockClear();
});

const sent = () => state.mutate.mock.calls[0]?.[0] as { intent: Record<string, unknown> };

describe('ItemsView', () => {
  it('lists only entries of its own kind', () => {
    render(<ItemsView />);
    const list = screen.getByRole('navigation', { name: 'items.library' });
    expect(within(list).getByText('Ring of protection')).toBeInTheDocument();
    expect(within(list).queryByText('Fire bolt')).not.toBeInTheDocument();
  });

  it('shows the card without form fields', () => {
    render(<ItemsView />);
    const card = screen.getByRole('region', { name: 'items.details' });
    expect(within(card).queryByRole('textbox')).not.toBeInTheDocument();
    expect(within(card).queryByRole('combobox')).not.toBeInTheDocument();
    expect(within(card).getByText('Ring of protection')).toBeInTheDocument();
    expect(within(card).getByText('ac +1')).toBeInTheDocument();
    expect(within(card).getByText('@item.armor_bonus = 2')).toBeInTheDocument();
    expect(within(card).getByText('Poisoned')).toBeInTheDocument();
  });

  it('creates an item from a dialog', () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: 'items.newItem' }));
    fireEvent.change(screen.getByPlaceholderText('items.name'), { target: { value: 'Chalk' } });
    fireEvent.click(screen.getByRole('button', { name: 'common.create' }));
    expect(sent().intent).toMatchObject({
      kind: 'create_item',
      item_kind: 'item',
      name: 'Chalk',
      type_id: null,
    });
  });

  it('creates a skill from the skills view', () => {
    render(<ItemsView kind="skill" />);
    fireEvent.click(screen.getByRole('button', { name: 'items.newSkill' }));
    fireEvent.change(screen.getByPlaceholderText('items.name'), { target: { value: 'Fire bolt' } });
    fireEvent.click(screen.getByRole('button', { name: 'common.create' }));
    expect(sent().intent).toMatchObject({ kind: 'create_item', item_kind: 'skill' });
  });

  it('renames and changes the state from the edit dialog', () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: 'common.edit' }));
    fireEvent.change(screen.getByDisplayValue('Ring of protection'), {
      target: { value: 'Ring of warding' },
    });
    fireEvent.change(screen.getByLabelText('items.state'), { target: { value: 'broken' } });
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));
    expect(sent().intent).toMatchObject({
      kind: 'update_item',
      id: 'i1',
      name: 'Ring of warding',
      state: 'broken',
    });
  });

  it('adds effects, custom values and granted conditions in the effects dialog', () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: 'items.effects' }));
    fireEvent.change(screen.getByLabelText('items.effectPath'), { target: { value: 'level' } });
    fireEvent.change(screen.getByLabelText('items.effectAdd'), { target: { value: '2' } });
    fireEvent.click(screen.getAllByRole('button', { name: 'common.add' })[0] as HTMLElement);
    fireEvent.change(screen.getByLabelText('items.valueKey'), { target: { value: 'range' } });
    fireEvent.change(screen.getByLabelText('items.valueAmount'), { target: { value: '9' } });
    fireEvent.click(screen.getAllByRole('button', { name: 'common.add' })[1] as HTMLElement);
    fireEvent.click(screen.getByRole('checkbox', { name: 'Prone' }));
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));
    const intent = sent().intent as {
      effects: { path: string; add: number }[];
      values: { key: string; value: number }[];
      grants: string[];
    };
    expect(intent.effects).toEqual([
      { path: 'ac', add: 1, minimum: null, maximum: null },
      { path: 'level', add: 2, minimum: null, maximum: null },
    ]);
    expect(intent.values).toEqual([
      { key: 'armor_bonus', value: 2 },
      { key: 'range', value: 9 },
    ]);
    expect(intent.grants).toEqual(['poisoned', 'prone']);
  });

  it('hands the entry over from the owner dialog', () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: 'items.give' }));
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'c2' } });
    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));
    expect(sent().intent).toEqual({ kind: 'transfer_item', id: 'i1', owner: 'c2' });
  });

  it('duplicates the entry under a fresh id', () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: 'items.duplicate' }));
    expect(sent().intent).toMatchObject({ kind: 'duplicate_item', id: 'i1' });
  });

  it('hands an entry to a character dropped on the owner chip', () => {
    render(<ItemsView />);
    const row = screen.getByRole('button', { name: /Ring of protection/ });
    const store = new Map<string, string>();
    const dataTransfer = {
      types: ['application/x-dndstudio-item'],
      setData: (type: string, value: string) => store.set(type, value),
      getData: (type: string) => store.get(type) ?? '',
      effectAllowed: 'move',
    };
    fireEvent.dragStart(row, { dataTransfer });
    fireEvent.drop(screen.getByRole('button', { name: 'Knight' }), { dataTransfer });
    expect(sent().intent).toEqual({ kind: 'transfer_item', id: 'i1', owner: 'c2' });
  });

  it('a player gets no GM actions but may edit their own entry', () => {
    state.role = 'player';
    render(<ItemsView />);
    expect(screen.queryByRole('button', { name: 'items.effects' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'items.duplicate' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'common.edit' })).toBeInTheDocument();
  });

  it("keeps a typed entry's plugin sheet behind a dialog, not in the card", () => {
    render(<ItemsView />);
    fireEvent.click(screen.getByRole('button', { name: /Wand of sparks/ }));
    const card = screen.getByRole('region', { name: 'items.details' });
    // The card is a card: no inputs even for an entry with a plugin schema.
    expect(within(card).queryByRole('textbox')).not.toBeInTheDocument();
    expect(within(card).queryByRole('combobox')).not.toBeInTheDocument();
    // The plugin sheet is not drawn in the card at all; it lives in the dialog.
    expect(within(card).queryByText('plugin sheet')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'items.data' }));
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('plugin sheet')).toBeInTheDocument();
  });
});
