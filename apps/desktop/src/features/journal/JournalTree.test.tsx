import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { JournalTree } from './JournalTree';
import type {
  CampaignInfo,
  IntentEnvelope,
  JournalEntryMeta,
  JournalFolderDto,
  Role,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const folder = (id: string, name: string, parent: string | null = null): JournalFolderDto => ({
  id,
  parent,
  name,
  sort_order: 0,
  version: 2,
});

const entry = (
  id: string,
  title: string,
  folder_id: string | null,
  visibility: 'gm' | 'players' = 'gm',
): JournalEntryMeta => ({
  id,
  folder_id,
  title,
  sort_order: 0,
  visibility,
  players_can_edit: false,
  author: null,
  version: 5,
});

// Lore > Deep > Secret; Lore > Crypt; Plan at the root.
const FOLDERS = [folder('lore', 'Lore'), folder('deep', 'Deep', 'lore')];
const ENTRIES = [
  entry('crypt', 'Crypt', 'lore', 'players'),
  entry('secret', 'Secret', 'deep'),
  entry('plan', 'Plan', null),
];

let sent: IntentEnvelope[];

function setup(role: Role = 'gm') {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [],
    users: [],
    journal: ENTRIES,
    journal_folders: FOLDERS,
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role };
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <JournalTree />
    </QueryClientProvider>,
  );
}

const row = (name: string) => screen.getByRole('treeitem', { name });
const tree = () => screen.getByRole('tree');

beforeEach(() => {
  sent = [];
  localStorage.clear();
});

afterEach(() => {
  useSelection.getState().reset();
});

describe('JournalTree', () => {
  it('starts collapsed; a click opens a folder and the tree remembers it', async () => {
    const view = setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    expect(row('Lore')).toHaveAttribute('aria-expanded', 'true');
    expect(row('Crypt')).toBeInTheDocument();
    expect(screen.queryByRole('treeitem', { name: 'Secret' })).not.toBeInTheDocument();

    view.unmount();
    setup();
    expect(await screen.findByRole('treeitem', { name: 'Crypt' })).toBeInTheDocument();
  });

  it('creates a folder inside the selected folder', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    await userEvent.click(screen.getByRole('button', { name: 'New folder' }));
    await userEvent.type(screen.getByLabelText('Folder name'), 'Places{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'create_journal_folder',
      parent: 'lore',
      name: 'Places',
    });
  });

  it('creates an entry in the folder of the selected entry; Escape cancels', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    await userEvent.click(row('Crypt'));
    await userEvent.click(screen.getByRole('button', { name: 'New entry' }));
    await userEvent.type(screen.getByLabelText('Entry title'), 'Draft{Escape}');
    expect(screen.queryByLabelText('Entry title')).not.toBeInTheDocument();
    expect(sent).toHaveLength(0);

    await userEvent.click(screen.getByRole('button', { name: 'New entry' }));
    await userEvent.type(screen.getByLabelText('Entry title'), 'Ghosts{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'create_journal_entry',
      folder_id: 'lore',
      title: 'Ghosts',
    });
    // The new entry opens in a tab.
    await waitFor(() => {
      expect(useSelection.getState().journalId).toBe(
        (sent[0]?.intent as { id: string } | undefined)?.id,
      );
    });
  });

  it('F2 renames the selected folder with its version', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Plan' }));
    await userEvent.click(row('Lore'));
    tree().focus();
    await userEvent.keyboard('{F2}');
    const input = screen.getByLabelText('Rename');
    await userEvent.clear(input);
    await userEvent.type(input, 'Legends{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 2,
      intent: { kind: 'rename_journal_folder', id: 'lore', name: 'Legends' },
    });
  });

  it('double click renames an entry; an unchanged name sends nothing', async () => {
    setup();
    await userEvent.dblClick(await screen.findByRole('treeitem', { name: 'Plan' }));
    await userEvent.keyboard('{Enter}');
    expect(sent).toHaveLength(0);
    await userEvent.dblClick(row('Plan'));
    await userEvent.keyboard('{Control>}a{/Control}Scheme{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 5,
      intent: { kind: 'update_journal_entry', id: 'plan', title: 'Scheme', markdown: null },
    });
  });

  it('Delete asks first and counts what goes with the folder', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    tree().focus();
    await userEvent.keyboard('{Delete}');
    const dialog = screen.getByRole('alertdialog');
    expect(dialog).toHaveTextContent('Entries: 2, subfolders: 1');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(sent).toHaveLength(0);

    await userEvent.keyboard('{Delete}');
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
    );
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 2,
      intent: { kind: 'delete_journal_folder', id: 'lore' },
    });
  });

  it('drags an entry into a folder, never a folder into its own subfolder', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    const drag = (from: HTMLElement, to: HTMLElement) => {
      const types: string[] = [];
      const dataTransfer = {
        types,
        setData: (type: string) => types.push(type),
        effectAllowed: 'none',
        dropEffect: 'none',
      };
      fireEvent.dragStart(from, { dataTransfer });
      fireEvent.dragOver(to, { dataTransfer });
      fireEvent.drop(to, { dataTransfer });
      fireEvent.dragEnd(from, { dataTransfer });
    };

    drag(row('Lore'), row('Deep'));
    expect(sent).toHaveLength(0);

    drag(row('Plan'), row('Deep'));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 5,
      intent: { kind: 'move_journal_entry', id: 'plan', folder_id: 'deep', sort_order: 1 },
    });

    // Dropping on an entry means its folder; on empty space — the root.
    drag(row('Deep'), document.querySelector('.journal-tree') as HTMLElement);
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent[1]?.intent).toMatchObject({
      kind: 'move_journal_folder',
      id: 'deep',
      parent: null,
    });
  });

  it('arrow keys walk and fold the tree; Enter opens an entry', async () => {
    setup();
    await screen.findByRole('treeitem', { name: 'Lore' });
    tree().focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(row('Lore')).toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{ArrowRight}');
    expect(row('Lore')).toHaveAttribute('aria-expanded', 'true');
    await userEvent.keyboard('{ArrowRight}{ArrowDown}');
    expect(row('Crypt')).toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{Enter}');
    expect(useSelection.getState().journalId).toBe('crypt');
    await userEvent.keyboard('{ArrowLeft}');
    expect(row('Lore')).toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{ArrowLeft}');
    expect(row('Lore')).toHaveAttribute('aria-expanded', 'false');
  });

  it('the context menu creates inside the clicked folder', async () => {
    setup();
    fireEvent.contextMenu(await screen.findByRole('treeitem', { name: 'Lore' }));
    const menu = screen.getByRole('menu');
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'New entry' }));
    await userEvent.type(screen.getByLabelText('Entry title'), 'Omens{Enter}');
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({ kind: 'create_journal_entry', folder_id: 'lore' });
  });

  it('players see shared entries and their folders only, and change nothing', async () => {
    setup('player');
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Lore' }));
    expect(row('Crypt')).toBeInTheDocument();
    expect(screen.queryByRole('treeitem', { name: 'Deep' })).not.toBeInTheDocument();
    expect(screen.queryByRole('treeitem', { name: 'Plan' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'New entry' })).not.toBeInTheDocument();
    tree().focus();
    await userEvent.keyboard('{F2}{Delete}');
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });
});
