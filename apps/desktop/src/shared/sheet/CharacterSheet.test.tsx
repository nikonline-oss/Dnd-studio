import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CharacterSheet, type CharacterSheetProps } from './CharacterSheet';
import { d20Sheet, d6PoolSheet, type SheetFixture } from './fixtures';
import type { SheetData, SheetNode } from './model';
import i18n from '@/shared/i18n';

function renderSheet(fixture: SheetFixture, patch: Partial<CharacterSheetProps> = {}) {
  const onChange = vi.fn<(next: SheetData) => void>();
  const onRoll = vi.fn<(id: string) => void>();
  const props: CharacterSheetProps = { ...fixture, canEdit: true, onChange, onRoll, ...patch };
  const view = render(<CharacterSheet {...props} />);
  return { ...view, onChange, onRoll, props };
}

const lastChange = (fn: ReturnType<typeof vi.fn<(next: SheetData) => void>>) => {
  const call = fn.mock.lastCall;
  if (!call) throw new Error('onChange was not called');
  return call[0];
};

afterEach(async () => {
  await act(() => i18n.changeLanguage('en'));
});

describe('CharacterSheet', () => {
  it('steps numbers with −/+ within the schema and shows rolls with the character’s numbers', async () => {
    const user = userEvent.setup();
    const { onChange } = renderSheet(d20Sheet);
    await user.click(screen.getByRole('button', { name: 'Increase Strength' }));
    expect(lastChange(onChange)).toMatchObject({ abilities: { str: 9 } });
    // Initiative: 1d20 + @dex_mod with DEX 14.
    const initiative = screen.getByRole('button', { name: 'Roll: Initiative' });
    expect(initiative).toHaveTextContent('1d20+2');
  });

  it('renders every node type of the d20 sheet', async () => {
    const user = userEvent.setup();
    renderSheet(d20Sheet);

    // tabs
    const tablist = screen.getByRole('tablist');
    expect(
      within(tablist)
        .getAllByRole('tab')
        .map((t) => t.textContent),
    ).toEqual(['Main', 'Skills', 'Notes']);
    expect(screen.getByRole('tab', { name: 'Main' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel')).toHaveAccessibleName('Main');
    // section + grid + field widgets derived from the schema
    expect(screen.getByRole('region', { name: 'Abilities' })).toBeInTheDocument();
    const abilities = screen.getByRole('region', { name: 'Abilities' });
    expect(within(abilities).getByLabelText('Strength')).toHaveValue('8');
    expect(screen.getByLabelText('Name')).toHaveValue('Aria');
    expect(screen.getByLabelText('Alignment')).toHaveDisplayValue('neutral');
    expect(screen.getByLabelText('Inspiration')).not.toBeChecked();
    expect(
      within(screen.getByRole('region', { name: 'Saving throws' })).getByLabelText('Dexterity'),
    ).toBeChecked();
    // computed
    expect(screen.getAllByRole('status', { name: 'Mod' }).map((o) => o.textContent)).toEqual([
      '-1',
      '2',
      '1',
      '0',
      '1',
      '2',
    ]);
    // roll
    expect(screen.getByRole('button', { name: 'Roll: Strength check' })).toHaveAttribute(
      'title',
      '1d20 + @str_mod',
    );
    // resource
    expect(screen.getByRole('meter', { name: 'Hit points' })).toHaveProperty('value', 17);
    expect(screen.getByText('17 / 24')).toBeInTheDocument();

    // list
    await user.click(screen.getByRole('tab', { name: 'Skills' }));
    expect(screen.getByRole('tab', { name: 'Skills' })).toHaveAttribute('aria-selected', 'true');
    const first = screen.getByRole('group', { name: 'Skills, row 1' });
    expect(within(first).getByLabelText('Skill')).toHaveValue('Stealth');
    expect(within(first).getByLabelText('Proficient')).toBeChecked();
    expect(screen.getByRole('group', { name: 'Skills, row 2' })).toBeInTheDocument();

    // text + textarea
    await user.click(screen.getByRole('tab', { name: 'Notes' }));
    expect(screen.getByText('Background and the rest.')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Notes' }).tagName).toBe('TEXTAREA');
  });

  it('moves between tabs with the arrow keys', async () => {
    const user = userEvent.setup();
    renderSheet(d20Sheet);
    await user.click(screen.getByRole('tab', { name: 'Main' }));
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Skills' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Skills' })).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(screen.getByRole('tab', { name: 'Notes' })).toHaveAttribute('aria-selected', 'true');
  });

  it('renders the d6-pool sheet with the same components', () => {
    renderSheet(d6PoolSheet);
    expect(screen.getByLabelText('Agility')).toHaveValue('4');
    expect(screen.getByRole('button', { name: 'Roll: Agility' })).toBeEnabled();
    expect(screen.getByLabelText('Bruised')).toBeChecked();
    expect(screen.getByRole('status', { name: 'Penalty' })).toHaveTextContent('-1');
    expect(screen.getByRole('meter', { name: 'Edge' })).toBeInTheDocument();
  });

  it('emits a number on blur, clamped to the schema, as an immutable patch', async () => {
    const user = userEvent.setup();
    const { onChange } = renderSheet(d20Sheet);
    const str = within(screen.getByRole('region', { name: 'Abilities' })).getByLabelText(
      'Strength',
    );

    await user.clear(str);
    await user.type(str, '99');
    expect(onChange).not.toHaveBeenCalled(); // no emit per keystroke
    await user.tab();

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = lastChange(onChange);
    expect(next.abilities).toEqual({ ...(d20Sheet.data.abilities as object), str: 30 });
    expect(next.saves).toBe(d20Sheet.data.saves);
    expect((d20Sheet.data.abilities as { str: number }).str).toBe(8);
  });

  it('emits text on Enter, cut to maxLength; Escape and non-integers revert', async () => {
    const user = userEvent.setup();
    const { onChange } = renderSheet(d20Sheet);

    const name = screen.getByLabelText('Name');
    await user.clear(name);
    await user.paste('x'.repeat(70));
    await user.keyboard('{Enter}');
    expect(lastChange(onChange).name).toBe('x'.repeat(60));

    onChange.mockClear();
    const level = screen.getByLabelText('Level');
    await user.clear(level);
    await user.type(level, '2.5');
    expect(level).toHaveAttribute('aria-invalid', 'true');
    await user.tab();
    expect(onChange).not.toHaveBeenCalled();
    expect(level).toHaveValue('3');

    await user.clear(level);
    await user.type(level, '7{Escape}');
    expect(level).toHaveValue('3');
    await user.tab();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('emits checkboxes and selects immediately', async () => {
    const user = userEvent.setup();
    const { onChange } = renderSheet(d20Sheet);

    await user.click(screen.getByLabelText('Inspiration'));
    expect(lastChange(onChange).inspiration).toBe(true);

    await user.selectOptions(screen.getByLabelText('Alignment'), 'chaotic');
    expect(lastChange(onChange).alignment).toBe('chaotic');
    // Two edits before the parent echoed the first one back still compose.
    expect(lastChange(onChange).inspiration).toBe(true);
  });

  it('adds and removes list rows, respecting maxItems', async () => {
    const user = userEvent.setup();
    const { onChange, rerender, props } = renderSheet(d6PoolSheet);

    await user.click(screen.getByRole('button', { name: 'Add to “Talents”' }));
    const added = lastChange(onChange);
    expect(added.talents).toEqual([
      { name: 'Pilot', dice: 2 },
      { name: '', dice: 1 },
    ]);

    rerender(<CharacterSheet {...props} data={added} />);
    const row2 = screen.getByRole('group', { name: 'Talents, row 2' });
    await user.type(within(row2).getByLabelText('Talent'), 'Brawl{Enter}');
    const named = lastChange(onChange);
    expect(named.talents).toEqual([
      { name: 'Pilot', dice: 2 },
      { name: 'Brawl', dice: 1 },
    ]);

    rerender(<CharacterSheet {...props} data={{ ...named, talents: [{}, {}, {}] }} />);
    expect(screen.getByRole('button', { name: 'Add to “Talents”' })).toBeDisabled();

    rerender(<CharacterSheet {...props} data={named} />);
    await user.click(screen.getByRole('button', { name: 'Remove row 1 from “Talents”' }));
    expect(lastChange(onChange).talents).toEqual([{ name: 'Brawl', dice: 1 }]);
  });

  it('shows values without inputs in read-only mode, but rolls still work', async () => {
    const user = userEvent.setup();
    const { onRoll, onChange } = renderSheet(d20Sheet, { canEdit: false });

    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    expect(screen.getByText('Aria')).toBeInTheDocument();
    expect(screen.getAllByText('Yes')).toHaveLength(2); // dex and cha saves

    await user.click(screen.getByRole('button', { name: 'Roll: Initiative' }));
    expect(onRoll).toHaveBeenCalledWith('initiative');

    await user.click(screen.getByRole('tab', { name: 'Skills' }));
    expect(screen.queryByRole('button', { name: /Add to/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Remove row/ })).not.toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
  });

  it('passes the roll id on click', async () => {
    const user = userEvent.setup();
    const { onRoll } = renderSheet(d20Sheet);
    await user.click(screen.getByRole('button', { name: 'Roll: Dexterity check' }));
    expect(onRoll).toHaveBeenCalledWith('check_dex');
  });

  it('switches labels with the language', async () => {
    renderSheet(d20Sheet);
    expect(screen.getByRole('tab', { name: 'Main' })).toBeInTheDocument();
    await act(() => i18n.changeLanguage('ru'));
    expect(screen.getByRole('tab', { name: 'Основное' })).toBeInTheDocument();
    expect(
      within(screen.getByRole('region', { name: 'Характеристики' })).getByLabelText('Сила'),
    ).toHaveValue('8');
    expect(screen.getByRole('button', { name: 'Бросок: Проверка: Сила' })).toBeInTheDocument();
  });

  it('shows a placeholder for unknown nodes and missing references', () => {
    const layout = [
      { type: 'hologram', children: [] },
      { type: 'roll', roll: 'nope' },
      { type: 'resource', resource: 'mana' },
      { type: 'computed', name: 'nothing', label: 'Nothing' },
      { type: 'field', path: 'free', label: 'Free' },
    ] as unknown as SheetNode[];
    renderSheet(d20Sheet, { layout, schema: undefined, data: { free: 5 } });
    expect(screen.getByRole('note')).toHaveTextContent('Unsupported sheet element: hologram');
    expect(screen.getByRole('button', { name: 'Roll: nope' })).toBeDisabled();
    expect(screen.getByText('Not found: mana')).toBeInTheDocument();
    expect(screen.getByRole('status', { name: 'Nothing' })).toHaveTextContent('—');
    // Without a schema the widget comes from the value.
    expect(screen.getByLabelText('Free')).toHaveValue('5');
  });

  it('handles a 200-field sheet, keeping the other inputs in place', async () => {
    const user = userEvent.setup();
    const fields = Array.from({ length: 200 }, (_, i) => `f${i}`);
    const layout: SheetNode = {
      type: 'grid',
      columns: 4,
      children: fields.map((f) => ({ type: 'field', path: f, label: f })),
    };
    const data = Object.fromEntries(fields.map((f) => [f, 0]));
    const { onChange, rerender, props } = renderSheet(d20Sheet, {
      layout,
      schema: undefined,
      data,
    });
    const before = screen.getByLabelText('f199');
    await user.clear(screen.getByLabelText('f0'));
    await user.type(screen.getByLabelText('f0'), '5');
    await user.tab();
    const next = lastChange(onChange);
    rerender(<CharacterSheet {...props} data={next} />);
    expect(screen.getByLabelText('f0')).toHaveValue('5');
    expect(screen.getByLabelText('f199')).toBe(before);
  });
});
