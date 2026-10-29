import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { PluginsView } from './PluginsView';
import type {
  BundledPlugin,
  CampaignInfo,
  IntentEnvelope,
  IntentOutcome,
  PluginDto,
  Role,
  RemoteInfo,
  Snapshot,
} from '@/shared/ipc/client';

const plugin = (id: string, kind: PluginDto['kind'], name: string): PluginDto => ({
  id,
  version: '0.1.0',
  kind,
  name: { ru: name, en: name },
  license: 'MIT',
  attribution: null,
  archive_sha256: 'a'.repeat(64),
  system: null,
});

const BUNDLED: BundledPlugin[] = [
  { id: 'srd5', name: 'SRD 5.2', version: '0.1.0', kind: 'system', license: 'CC-BY-4.0' },
  { id: 'generic-d6', name: 'Generic 2d6', version: '0.1.0', kind: 'system', license: 'MIT' },
];

function setup(
  role: Role = 'gm',
  remote: RemoteInfo | null = null,
  installResult: IntentOutcome = { status: 'accepted', seq: 2 },
) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    journal_folders: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    plugins: [plugin('srd5', 'system', 'SRD 5.2'), plugin('dark', 'theme', 'Dark')],
    system_plugin: 'srd5',
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role };
  const sent: IntentEnvelope[] = [];
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case 'get_snapshot':
        return snapshot;
      case 'campaign_current':
        return info;
      case 'remote_current':
        return remote;
      case 'plugin_bundled':
        return BUNDLED;
      case 'plugin_install_bundled':
        return installResult;
      case 'submit_intent':
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        return { status: 'accepted', seq: 2 };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <PluginsView />
    </QueryClientProvider>,
  );
  return { sent, calls };
}

describe('PluginsView', () => {
  it('shows an authoritative install rejection instead of silently treating it as success', async () => {
    setup('gm', null, { status: 'rejected', reason: { code: 'invalid', field: 'plugin' } });
    const bundled = await screen.findByRole('region', { name: 'Bundled' });
    await userEvent.click(within(bundled).getByRole('button', { name: 'Install' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Invalid value');
  });

  it.each(['gm', 'co_gm'] as const)(
    'lets a connected %s install a bundled plugin without leaving the session',
    async (role) => {
      const { calls } = setup(role, {
        server: 'http://localhost',
        session_id: 's',
        session_name: 'C',
        role,
        hosted: true,
        status: { state: 'online' },
      });
      const bundled = await screen.findByRole('region', { name: 'Bundled' });
      await userEvent.click(within(bundled).getByRole('button', { name: 'Install' }));
      await waitFor(() => {
        expect(calls.some((c) => c.cmd === 'plugin_install_bundled')).toBe(true);
      });
      expect(screen.getByRole('button', { name: 'Install from file…' })).toBeInTheDocument();
    },
  );

  it.each(['player', 'spectator'] as const)(
    'keeps remote plugin management unavailable to %s',
    async (role) => {
      setup(role, {
        server: 'http://localhost',
        session_id: 's',
        session_name: 'C',
        role,
        hosted: false,
        status: { state: 'online' },
      });
      await screen.findByText('SRD 5.2');
      expect(screen.queryByRole('region', { name: 'Bundled' })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Install from file…' })).not.toBeInTheDocument();
    },
  );

  it('lists plugins with the system and the unsigned trust level', async () => {
    setup();
    const list = await screen.findByRole('list', { name: 'Plugins' });
    const srd = (await within(list).findByText('SRD 5.2')).closest('li') as HTMLElement;
    expect(srd).toHaveAttribute('aria-current', 'true');
    expect(within(srd).getByText('Campaign system')).toBeInTheDocument();
    expect(within(list).getAllByText('Unsigned')).toHaveLength(2);
    // The active system cannot be uninstalled.
    expect(within(srd).queryByRole('button', { name: 'Uninstall' })).not.toBeInTheDocument();
  });

  it('installs a bundled plugin that is not installed yet', async () => {
    const { calls } = setup();
    const bundled = await screen.findByRole('region', { name: 'Bundled' });
    expect(within(bundled).queryByText('SRD 5.2')).not.toBeInTheDocument();
    await userEvent.click(within(bundled).getByRole('button', { name: 'Install' }));
    await waitFor(() => {
      expect(calls.find((c) => c.cmd === 'plugin_install_bundled')?.args).toEqual({
        id: 'generic-d6',
      });
    });
  });

  it('uninstalls after a second click', async () => {
    const { sent } = setup();
    const list = await screen.findByRole('list', { name: 'Plugins' });
    const dark = (await within(list).findByText('Dark')).closest('li') as HTMLElement;
    await userEvent.click(within(dark).getByRole('button', { name: 'Uninstall' }));
    expect(sent).toHaveLength(0);
    await userEvent.click(within(dark).getByRole('button', { name: 'Delete?' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toEqual({ kind: 'uninstall_plugin', id: 'dark' });
    });
  });

  it('players only look', async () => {
    setup('player');
    await screen.findByText('SRD 5.2');
    expect(screen.queryByRole('button', { name: 'Install from file…' })).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Bundled' })).not.toBeInTheDocument();
  });
});
