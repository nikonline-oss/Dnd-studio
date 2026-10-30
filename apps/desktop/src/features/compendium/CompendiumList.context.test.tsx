import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { CompendiumList } from './CompendiumList';
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
      <CompendiumList />
    </QueryClientProvider>,
  );
  return { sent, calls, release, client };
}
afterEach(() => {
  useSelection.getState().reset();
  localStorage.clear();
});

describe('compendium list context menus', () => {
  it.each(['player', 'spectator', 'gm', 'co_gm'] as const)(
    'opens the clicked compendium as %s',
    async (role) => {
      setup(role);
      fireEvent.contextMenu(await screen.findByText('Monsters'));
      await userEvent.click(screen.getByRole('menuitem', { name: 'Open' }));
      expect(useSelection.getState().active).toEqual({ kind: 'compendium', id: 'system/monsters' });
      expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    },
  );
  it('refreshes from the background without opening a compendium', async () => {
    const { calls } = setup();
    await screen.findByText('Monsters');
    const count = calls.filter((c) => c === 'compendium_list').length;
    fireEvent.contextMenu(screen.getByRole('list', { name: 'Compendia' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Refresh' }));
    await waitFor(() => {
      expect(calls.filter((c) => c === 'compendium_list')).toHaveLength(count + 1);
    });
    expect(useSelection.getState().active).toBeNull();
  });
});
