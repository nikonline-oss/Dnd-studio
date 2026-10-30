import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { PluginsView } from './PluginsView';
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
      <PluginsView />
    </QueryClientProvider>,
  );
  return { sent, calls, release, client };
}
afterEach(() => {
  useSelection.getState().reset();
  localStorage.clear();
});

describe('plugin context menus', () => {
  it('protects the active system and confirms uninstall of the clicked plugin', async () => {
    const { sent } = setup();
    fireEvent.contextMenu(await screen.findByText('Rules'));
    expect(screen.getByRole('menuitem', { name: 'Uninstall' })).toBeDisabled();
    fireEvent.contextMenu(screen.getByText('Dark'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Uninstall' }));
    expect(sent).toHaveLength(0);
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Delete?' }),
    );
    await waitFor(() => {
      expect(sent[0]?.intent).toEqual({ kind: 'uninstall_plugin', id: 'theme' });
    });
  });
  it('installs the right bundled plugin and disables other writes while pending', async () => {
    const { calls, release } = setup('co_gm', true);
    fireEvent.contextMenu(await screen.findByText('Extra'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Install' }));
    await waitFor(() => {
      expect(calls).toContain('plugin_install_bundled');
    });
    fireEvent.contextMenu(screen.getByRole('list', { name: 'Plugins' }));
    expect(screen.getByRole('menuitem', { name: 'Install from file…' })).toBeDisabled();
    await act(async () => {
      release();
      await Promise.resolve();
    });
  });
  it.each(['player', 'spectator'] as const)('offers no management commands to %s', async (role) => {
    setup(role);
    fireEvent.contextMenu(await screen.findByText('Dark'));
    expect(screen.queryByRole('menuitem', { name: 'Uninstall' })).not.toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Install from file…' })).not.toBeInTheDocument();
  });
});
