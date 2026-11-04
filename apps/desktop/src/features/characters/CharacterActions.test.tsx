import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { SystemSheet } from './SystemSheet';
import { useActionAim } from '@/shared/state/actionAim';
import { useSelection } from '@/shared/state/selection';
import type {
  CharacterDto,
  IntentEnvelope,
  PluginAction,
  PluginDto,
  Snapshot,
} from '@/shared/ipc/client';
import system from '../../../../../plugins/system-srd5/system.json';
import schema from '../../../../../plugins/system-srd5/sheet/schema.json';
import layout from '../../../../../plugins/system-srd5/sheet/layout.json';

const character: CharacterDto = {
  id: 'hero',
  name: 'Mira',
  kind: 'pc',
  owner: 'player',
  status: 'active',
  version: 3,
  sheet_json: JSON.stringify({
    attacks: [{ name: 'My sword', bonus: 5, damage: '1d8+3', range: 2, damage_type: 'slashing' }],
    spells: [
      { name: 'First spell', action: 'fireball' },
      { name: 'Second spell', action: 'shield_of_faith' },
    ],
  }),
};
function setup(enabled = true, owner = true) {
  const rules = {
    ...system,
    sheet_schema: schema,
    sheet_layout: layout,
    creature_sheet_layout: null,
  } as unknown as NonNullable<PluginDto['system']>;
  const attack = {
    ...rules.mechanics?.actions?.[0],
    id: 'sheet:attacks:0:3',
    label: 'My sword',
    damage_dice: '1d8+3',
  } as PluginAction;
  const snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [{ id: 'token', character_id: character.id, map_id: 'map', x: 0, y: 0 }],
    characters: [character],
    plugins: [{ id: 'srd5', system: rules }],
    system_plugin: 'srd5',
    rules_view: {
      tokens: [
        {
          token: 'token',
          actions: [
            {
              id: attack.id,
              enabled,
              reason: enabled ? null : 'resources',
              range_cells: 2,
              sheet_action: { collection: 'attacks', index: 0, action: attack },
            },
          ],
        },
      ],
    },
  } as unknown as Snapshot;
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current')
      return { name: 'C', path: '/c', user_id: owner ? 'player' : 'other', role: 'player' };
    if (cmd === 'sheet_view') return { computed: {}, resources: [], modifiers: [] };
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(cmd);
  });
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <SystemSheet character={character} rules={rules} canEdit={owner} />
    </QueryClientProvider>,
  );
  return { sent, attack };
}
afterEach(() => {
  useActionAim.getState().setAim(null);
  useSelection.getState().reset();
});
it('launches the actual attack row into the map picker, without sending an attack yet', async () => {
  const { sent, attack } = setup();
  await userEvent.click(await screen.findByRole('tab', { name: 'Attacks & notes' }));
  const row = screen.getByRole('group', { name: 'Attacks, row 1' });
  await userEvent.click(within(row).getByRole('button', { name: 'Use' }));
  expect(useActionAim.getState().aim).toMatchObject({
    source: 'token',
    action: attack,
    point: null,
  });
  expect(useSelection.getState().paneTabs).toContainEqual({ kind: 'map', id: 'map' });
  expect(sent).toHaveLength(0);
});
it.each([
  [false, true],
  [true, false],
])(
  'disables the row when unavailable or owned by another player (%s, %s)',
  async (enabled, owner) => {
    setup(enabled, owner);
    await userEvent.click(await screen.findByRole('tab', { name: 'Attacks & notes' }));
    expect(screen.getByRole('button', { name: 'Use' })).toBeDisabled();
  },
);
it('edits the selected spell rule on its own list row', async () => {
  const { sent } = setup();
  await userEvent.click(await screen.findByRole('tab', { name: 'Magic' }));
  const selectors = screen.getAllByRole('combobox', { name: 'Action rule' });
  await userEvent.selectOptions(selectors[1] as HTMLSelectElement, 'fireball');
  const intent = sent[0]?.intent;
  expect(intent?.kind).toBe('update_character');
  if (intent?.kind === 'update_character') {
    expect(JSON.parse(intent.sheet_json ?? '{}')).toMatchObject({
      spells: [
        { name: 'First spell', action: 'fireball' },
        { name: 'Second spell', action: 'fireball' },
      ],
    });
  }
});
