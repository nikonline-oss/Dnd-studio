import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ItemStatesEditor } from './ItemStatesEditor';
import type { ItemStateDef, Snapshot } from '@/shared/ipc/client';

const state = vi.hoisted(() => ({
  mutate: vi.fn(),
}));

vi.mock('@/shared/game/useGame', () => ({
  useSnapshot: () => ({ data: snapshot }),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
vi.mock('@/shared/i18n/label', () => ({
  labelText: (label: unknown) =>
    typeof label === 'string' ? label : ((label as { ru?: string }).ru ?? ''),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, vars?: Record<string, string>) =>
      vars ? `${key}:${Object.values(vars).join(',')}` : key,
    i18n: { resolvedLanguage: 'ru', language: 'ru' },
  }),
}));

const custom: ItemStateDef[] = [
  { id: 'sealed', label: { ru: 'Запечатан', en: 'Sealed' }, active: true },
  { id: 'spent', label: { ru: 'Израсходован', en: 'Spent' }, active: false },
];

function snapshotWith(states: ItemStateDef[]): Snapshot {
  return {
    seq: 1,
    campaign: { name: 'C', description: '', version: 7, item_states: states },
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
    zones: [],
    triggers: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
}

let snapshot = snapshotWith(custom);

beforeEach(() => {
  snapshot = snapshotWith(custom);
  state.mutate.mockClear();
});

const sent = (index = 0) =>
  state.mutate.mock.calls[index]?.[0] as {
    intent: { kind: string; item_states: ItemStateDef[] };
    expectedVersion: number;
  };

describe('ItemStatesEditor', () => {
  it('lists the campaign states and whether they work', () => {
    render(<ItemStatesEditor />);
    expect(screen.getByText('sealed')).toBeInTheDocument();
    expect(screen.getByText('spent')).toBeInTheDocument();
    const boxes = screen.getAllByRole('checkbox');
    expect(boxes[0]).toBeChecked();
    expect(boxes[1]).not.toBeChecked();
  });

  it('falls back to the built-in set for a campaign that never edited it', () => {
    snapshot = snapshotWith([]);
    render(<ItemStatesEditor />);
    expect(screen.getByText('ok')).toBeInTheDocument();
    expect(screen.getByText('broken')).toBeInTheDocument();
    // "Restore" has nothing to do when the built-ins are already in use.
    expect(screen.getByRole('button', { name: 'items.resetStates' })).toBeDisabled();
  });

  it('toggles whether a state works with the campaign version', () => {
    render(<ItemStatesEditor />);
    fireEvent.click(screen.getAllByRole('checkbox')[1] as HTMLElement);
    expect(sent().intent).toMatchObject({
      kind: 'update_campaign_meta',
      item_states: [
        { id: 'sealed', active: true },
        { id: 'spent', active: true },
      ],
    });
    expect(sent().expectedVersion).toBe(7);
  });

  it('renames a state in the current language and keeps the other one', () => {
    render(<ItemStatesEditor />);
    const input = screen.getByLabelText<HTMLInputElement>('items.stateLabel:sealed');
    fireEvent.change(input, { target: { value: 'Запечатано' } });
    fireEvent.blur(input);
    const label = sent().intent.item_states[0]?.label as { ru?: string; en?: string };
    expect(label.ru).toBe('Запечатано');
    expect(label.en).toBe('Sealed');
  });

  it('adds a state with a plain id and a label', () => {
    render(<ItemStatesEditor />);
    fireEvent.change(screen.getByLabelText('items.newStateId'), { target: { value: 'rusted' } });
    fireEvent.change(screen.getByLabelText('items.newStateLabel'), {
      target: { value: 'Ржавый' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'items.addState' }));
    const states = sent().intent.item_states;
    expect(states).toHaveLength(3);
    expect(states[2]).toMatchObject({ id: 'rusted', active: true });
    expect(states[2]?.label).toMatchObject({ ru: 'Ржавый' });
  });

  it('refuses ids that are not plain or already taken', () => {
    render(<ItemStatesEditor />);
    const idInput = screen.getByLabelText('items.newStateId');
    const labelInput = screen.getByLabelText('items.newStateLabel');
    fireEvent.change(labelInput, { target: { value: 'X' } });
    fireEvent.change(idInput, { target: { value: 'two words' } });
    expect(screen.getByRole('button', { name: 'items.addState' })).toBeDisabled();
    fireEvent.change(idInput, { target: { value: 'sealed' } });
    expect(screen.getByRole('button', { name: 'items.addState' })).toBeDisabled();
    fireEvent.change(idInput, { target: { value: 'fine' } });
    expect(screen.getByRole('button', { name: 'items.addState' })).toBeEnabled();
  });

  it('removes a state but never the last one', () => {
    render(<ItemStatesEditor />);
    fireEvent.click(screen.getByRole('button', { name: 'items.removeState:spent' }));
    expect(sent().intent.item_states).toEqual([custom[0]]);
  });

  it('restores the built-in set with an empty list', () => {
    render(<ItemStatesEditor />);
    fireEvent.click(screen.getByRole('button', { name: 'items.resetStates' }));
    expect(sent().intent.item_states).toEqual([]);
  });

  it('warns when no state works', () => {
    snapshot = snapshotWith([{ id: 'dead', label: 'Dead', active: false }]);
    render(<ItemStatesEditor />);
    expect(screen.getByRole('alert')).toHaveTextContent('items.noActiveState');
  });
});
