import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { JournalTree } from './JournalTree';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const snapshot: Snapshot = {
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [],
  tokens: [],
  characters: [],
  users: [],
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  journal: [
    {
      id: 'entry',
      title: 'Shared note',
      folder_id: null,
      sort_order: 0,
      visibility: 'players',
      players_can_edit: true,
      author: null,
      version: 3,
    },
  ],
  journal_folders: [{ id: 'folder', name: 'Lore', parent: null, sort_order: 0, version: 2 }],
  plugins: [
    {
      id: 'system',
      name: 'Rules',
      kind: 'system',
      version: '1.0.0',
      license: 'MIT',
      attribution: null,
      archive_sha256: 'a'.repeat(64),
      system: null,
    },
    {
      id: 'theme',
      name: 'Dark',
      kind: 'theme',
      version: '1.0.0',
      license: 'MIT',
      attribution: null,
      archive_sha256: 'b'.repeat(64),
      system: null,
    },
  ],
  system_plugin: 'system',
  triggers: [
    {
      id: 'trigger',
      definition: {
        name: 'Ambush',
        enabled: true,
        once: false,
        event: { type: 'manual' },
        tokens: [],
        condition: null,
        steps: [{ when: null, action: { type: 'chat', text: 'Boo', gm_only: false } }],
      },
      fired: false,
      version: 4,
    },
  ],
};
function setup(role: Role = 'gm', pending = false) {
  const sent: IntentEnvelope[] = [];
  const calls: string[] = [];
  let release = () => {};
  const waiting = new Promise<{ status: 'accepted'; seq: number }>((resolve) => {
    release = () => {
      resolve({ status: 'accepted', seq: 2 });
    };
  });
  mockIPC((cmd, args) => {
    calls.push(cmd);
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return { name: 'C', path: '/menus', user_id: 'u', role };
    if (cmd === 'remote_current') return null;
    if (cmd === 'plugin_bundled')
      return [{ id: 'extra', name: 'Extra', version: '1.0.0', kind: 'system', license: 'MIT' }];
    if (cmd === 'compendium_list')
      return [
        { id: 'monsters', plugin_id: 'system', name: 'Monsters', entry_type: 'monster', count: 3 },
      ];
    if (cmd === 'plugin_install_bundled' || cmd === 'plugin_install_file')
      return pending ? waiting : { status: 'accepted', seq: 2 };
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return pending ? waiting : { status: 'accepted', seq: 2 };
    }
    throw new Error(cmd);
  });
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  render(
    <QueryClientProvider client={client}>
      <JournalTree />
    </QueryClientProvider>,
  );
  return { sent, calls, release, client };
}
afterEach(() => {
  useSelection.getState().reset();
  localStorage.clear();
});

describe('journal context menus', () => {
  it('opens a shared entry for a reader without offering destructive commands', async () => {
    const { sent } = setup('spectator');
    fireEvent.contextMenu(await screen.findByText('Shared note'));
    expect(screen.queryByRole('menuitem', { name: 'Delete' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: 'Open' }));
    expect(useSelection.getState().journalId).toBe('entry');
    expect(sent).toHaveLength(0);
  });
  it('changes visibility with the current version and blocks repeat writes', async () => {
    const { sent, release } = setup('co_gm', true);
    fireEvent.contextMenu(await screen.findByText('Shared note'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Hide from players' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: { kind: 'update_journal_entry', id: 'entry', visibility: 'gm' },
      });
    });
    fireEvent.contextMenu(screen.getByText('Shared note'));
    expect(screen.getByRole('menuitem', { name: 'Delete' })).toBeDisabled();
    expect(screen.getByRole('menuitem', { name: 'New entry' })).toBeDisabled();
    await act(async () => {
      release();
      await Promise.resolve();
    });
  });
  it('moves the clicked entry through a destination dialog', async () => {
    const { sent } = setup();
    fireEvent.contextMenu(await screen.findByText('Shared note'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Move to…' }));
    const dialog = screen.getByRole('dialog');
    await userEvent.selectOptions(within(dialog).getByRole('combobox'), 'folder');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Move' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: { kind: 'move_journal_entry', id: 'entry', folder_id: 'folder' },
      });
    });
  });
  it('background expansion is available to readers and closes on Escape', async () => {
    setup('player');
    fireEvent.contextMenu(await screen.findByRole('tree'));
    expect(screen.getByRole('menuitem', { name: 'Expand all' })).toBeEnabled();
    expect(screen.queryByRole('menuitem', { name: 'New entry' })).not.toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});
