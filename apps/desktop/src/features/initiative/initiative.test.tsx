import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { InitiativePanel } from './InitiativePanel';
import type {
  CampaignInfo,
  CombatantDto,
  CombatDto,
  IntentEnvelope,
  Role,
  Snapshot,
  TokenDto,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const ME = 'u-me';
const id = (n: number) => `0190a000-0000-7000-8000-${n.toString().padStart(12, '0')}`;

const token = (n: number, layer: TokenDto['layer'], character: string | null = null): TokenDto => ({
  id: id(n),
  map_id: id(1),
  character_id: character,
  asset_id: null,
  name: `Token ${n}`,
  x: 0,
  y: 0,
  rotation: 0,
  scale: 1,
  layer,
  visible: true,
  version: 1,
});

const combatant = (n: number, initiative: number | null, character: string | null = null) =>
  ({
    id: id(100 + n),
    combat_id: id(50),
    token_id: id(n),
    character_id: character,
    name: n === 2 ? 'Mira' : `Goblin ${n}`,
    initiative,
    sort_order: n,
    version: 3,
  }) satisfies CombatantDto;

function setup(role: Role, combat: CombatDto | null, combatants: CombatantDto[] = []) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [
      {
        id: id(1),
        name: 'Crypt',
        background_asset_id: null,
        width: 100,
        height: 100,
        grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
        visible_to_players: true,
        version: 1,
      },
    ],
    tokens: [token(2, 'tokens', 'c-mira'), token(3, 'tokens'), token(4, 'background')],
    characters: [
      {
        id: 'c-mira',
        kind: 'pc',
        name: 'Mira',
        owner: ME,
        status: 'active',
        sheet_json: '{}',
        version: 1,
      },
    ],
    users: [],
    journal: [],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    assets: [],
    fog: [],
    combats: combat ? [combat] : [],
    combatants,
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: ME, role };
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
  useSelection.getState().openMap(id(1));
  render(
    <QueryClientProvider client={new QueryClient()}>
      <InitiativePanel />
    </QueryClientProvider>,
  );
  return sent;
}

const combat = (turn: string | null): CombatDto => ({
  id: id(50),
  map_id: id(1),
  round: 2,
  turn,
  version: 7,
});

afterEach(() => {
  useSelection.getState().reset();
});

describe('InitiativePanel', () => {
  it('GM starts a combat with every token of the map except background props', async () => {
    const sent = setup('gm', null);
    expect(await screen.findByText(/with the 2 tokens/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Start combat' }));
    await waitFor(() => {
      expect(sent).toHaveLength(3);
    });
    const [start, ...adds] = sent.map((e) => e.intent);
    expect(start).toMatchObject({ kind: 'start_combat', map_id: id(1) });
    const combatId = start?.kind === 'start_combat' ? start.id : '';
    expect(adds).toMatchObject([
      { kind: 'add_combatant', combat_id: combatId, token_id: id(2), character_id: 'c-mira' },
      { kind: 'add_combatant', combat_id: combatId, token_id: id(3), name: 'Token 3' },
    ]);
  });

  it('players cannot start a combat', async () => {
    setup('player', null);
    expect(await screen.findByText('No combat on this map')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Start combat' })).not.toBeInTheDocument();
  });

  it('lists combatants in turn order and marks whose turn it is', async () => {
    setup('gm', combat(id(102)), [combatant(3, 9), combatant(2, 15, 'c-mira'), combatant(5, null)]);
    const rows = await screen.findAllByRole('listitem');
    expect(rows.map((r) => within(r).getByText(/Goblin|Mira/).textContent)).toEqual([
      'Mira',
      'Goblin 3',
      'Goblin 5',
    ]);
    expect(rows[0]).toHaveAttribute('aria-current', 'step');
    expect(screen.getByText('Round 2')).toBeInTheDocument();
    expect(screen.getByText('Mira’s turn')).toBeInTheDocument();
  });

  it('GM passes the turn with the combat version and rolls for the unrolled', async () => {
    const sent = setup('gm', combat(id(102)), [combatant(2, 15, 'c-mira'), combatant(5, null)]);
    await userEvent.click(await screen.findByRole('button', { name: 'Next turn' }));
    await userEvent.click(screen.getByRole('button', { name: 'Roll for all' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 7,
      intent: { kind: 'next_turn', combat_id: id(50) },
    });
    expect(sent[1]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'roll_initiative', id: id(105) },
    });
  });

  it('GM types an initiative; an empty field clears it', async () => {
    const sent = setup('gm', combat(null), [combatant(3, 9)]);
    const input = await screen.findByLabelText('Initiative of Goblin 3');
    await userEvent.clear(input);
    await userEvent.type(input, '21{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'set_initiative', id: id(103), initiative: 21 },
    });
  });

  it('a player rolls only for their character and passes only their own turn', async () => {
    const sent = setup('player', combat(id(103)), [combatant(2, null, 'c-mira'), combatant(3, 9)]);
    expect(await screen.findByRole('button', { name: 'Roll initiative for Mira' })).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: 'Roll initiative for Goblin 3' }),
    ).not.toBeInTheDocument();
    // It is the goblin's turn: no "next turn" for the player, no GM tools.
    expect(screen.queryByRole('button', { name: 'Next turn' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'End combat' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Initiative of Goblin 3')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Roll initiative for Mira' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toEqual({ kind: 'roll_initiative', id: id(102) });
  });

  it('on their own turn a player may pass it', async () => {
    setup('player', combat(id(102)), [combatant(2, 15, 'c-mira'), combatant(3, 9)]);
    expect(await screen.findByRole('button', { name: 'Next turn' })).toBeEnabled();
  });

  it('ending the combat needs a second click', async () => {
    const sent = setup('gm', combat(null), [combatant(3, 9)]);
    await userEvent.click(await screen.findByRole('button', { name: 'End combat' }));
    expect(sent).toHaveLength(0);
    await userEvent.click(screen.getByRole('button', { name: 'End combat?' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 7,
      intent: { kind: 'end_combat', id: id(50) },
    });
  });
});
