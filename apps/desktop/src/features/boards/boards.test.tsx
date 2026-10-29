import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, describe, expect, it } from 'vitest';
import { BoardList } from './BoardList';
import { BoardView } from './BoardView';
import { useSelection } from '@/shared/state/selection';
import type {
  BoardNodeDto,
  CampaignInfo,
  IntentEnvelope,
  MapDto,
  Role,
  Snapshot,
} from '@/shared/ipc/client';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const note = (n: number, text: string, visibility: BoardNodeDto['visibility']): BoardNodeDto => ({
  id: id(n),
  board_id: id(1),
  content: { type: 'note', markdown: text },
  x: 0,
  y: 0,
  width: 3200,
  height: 1600,
  visibility,
  version: 1,
});

const crypt: MapDto = {
  id: id(20),
  name: 'Sunken Crypt',
  background_asset_id: null,
  width: 4000,
  height: 3000,
  grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
  visible_to_players: false,
  version: 1,
};

function setup(role: Role) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [crypt],
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [{ id: id(1), name: 'Plot', sort_order: 0, visible_to_players: true, version: 1 }],
    board_nodes: [
      note(2, 'The missing mayor', 'players'),
      note(3, 'Secret passage', 'gm'),
      { ...note(4, '', 'gm'), content: { type: 'trigger', id: id(50) }, x: 6400 },
      { ...note(5, '', 'gm'), content: { type: 'link', target: { kind: 'map', id: crypt.id } } },
    ],
    board_edges: [],
    zones: [],
    triggers: [
      {
        id: id(50),
        definition: {
          name: 'Collapse',
          enabled: true,
          once: false,
          event: { type: 'manual' },
          tokens: [],
          condition: null,
          steps: [{ when: null, action: { type: 'reveal', target: { kind: 'node', id: id(3) } } }],
        },
        fired: false,
        version: 1,
      },
    ],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role };
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
      <BoardList />
      <BoardView boardId={id(1)} />
    </QueryClientProvider>,
  );
  return sent;
}

// jsdom does not capture pointers.
beforeAll(() => {
  Element.prototype.setPointerCapture = () => undefined;
});

describe('BoardView', () => {
  it('creates a map from the open board and places an opening card on that board', async () => {
    const sent = setup('gm');
    await userEvent.click(await screen.findByRole('button', { name: 'New map' }));
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    const createMap = sent.find(({ intent }) => intent.kind === 'create_map');
    const createNode = sent.find(({ intent }) => intent.kind === 'create_board_node');
    expect(createMap?.intent).toMatchObject({
      kind: 'create_map',
      name: 'New map 2',
      visible_to_players: false,
    });
    expect(createNode?.intent).toMatchObject({
      kind: 'create_board_node',
      board_id: id(1),
      content: {
        type: 'link',
        target: {
          kind: 'map',
          id: createMap?.intent.kind === 'create_map' ? createMap.intent.id : '',
        },
      },
    });
  });

  it('GM roles pick a link in the selection form and it becomes a card', async () => {
    const sent = setup('gm');
    await userEvent.click(await screen.findByRole('button', { name: 'Link' }));
    const dialog = screen.getByRole('dialog', { name: 'Link to…' });
    await userEvent.type(within(dialog).getByRole('searchbox'), 'crypt');
    await userEvent.dblClick(within(dialog).getByRole('option', { name: /Sunken Crypt/ }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_board_node',
        board_id: id(1),
        content: { type: 'link', target: { kind: 'map', id: crypt.id } },
        visibility: 'gm',
      });
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('GM roles open a card to players', async () => {
    const sent = setup('gm');
    await userEvent.click(await screen.findByText('Secret passage'));
    await userEvent.click(screen.getByRole('button', { name: 'GM only' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 1,
        intent: { kind: 'update_board_node', id: id(3), visibility: 'players' },
      });
    });
  });

  it('GM roles resize a card by its corner', async () => {
    const sent = setup('gm');
    const card = (await screen.findByText('Secret passage')).closest('.board-card') as HTMLElement;
    const corner = card.querySelector('.board-card-resize') as HTMLElement;
    const canvas = screen.getByRole('application', { name: 'Board canvas' });
    fireEvent.pointerDown(corner, { button: 0, clientX: 200, clientY: 100 });
    fireEvent.pointerMove(canvas, { clientX: 300, clientY: 150 });
    fireEvent.pointerUp(canvas, { clientX: 300, clientY: 150 });
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 1,
        // 200×100 px + 100×50 px, in Units16.
        intent: { kind: 'update_board_node', id: id(3), width: 300 * 16, height: 150 * 16 },
      });
    });
  });

  it('triggers are blocks: placed with the selection form, run from the board, linked to what they open', async () => {
    const sent = setup('gm');
    const block = await screen.findByRole('group', { name: 'Trigger' });
    expect(block).toHaveTextContent('Collapse');
    expect(block).toHaveTextContent('Show to players');
    expect(document.querySelectorAll('.board-trigger-line')).toHaveLength(1);
    await userEvent.click(within(block).getByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toEqual({ kind: 'fire_trigger', id: id(50), token: null });
    });

    await userEvent.click(screen.getByRole('button', { name: 'Trigger' }));
    await userEvent.dblClick(screen.getByRole('option', { name: /Collapse/ }));
    await waitFor(() => {
      expect(sent[1]?.intent).toMatchObject({
        kind: 'create_board_node',
        content: { type: 'trigger', id: id(50) },
      });
    });
  });

  it('players only look', async () => {
    setup('player');
    expect(await screen.findByText('The missing mayor')).toBeInTheDocument();
    expect(screen.queryByRole('toolbar', { name: 'Board tools' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'New board' })).not.toBeInTheDocument();
  });
});

describe('BoardList', () => {
  it('shows maps nested under an expandable board and opens a selected map', async () => {
    setup('gm');
    await userEvent.click(await screen.findByRole('button', { name: 'Show maps in Plot' }));
    const maps = await screen.findByRole('list', { name: 'Maps in Plot' });
    await userEvent.click(within(maps).getByRole('button', { name: 'Sunken Crypt' }));
    expect(useSelection.getState().mapId).toBe(crypt.id);
    expect(useSelection.getState().center).toBe('map');
  });
});
