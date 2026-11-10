import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CatalogView } from './CatalogView';
import { useSettings } from '@/shared/settings/settings';
import type {
  CampaignInfo,
  CatalogView as Catalog,
  IntentOutcome,
  Snapshot,
} from '@/shared/ipc/client';

const campaign: CampaignInfo = { name: 'C', path: '/c', user_id: 'u1', role: 'gm' };

const snapshot = (plugins: Snapshot['plugins']): Snapshot => ({
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
  plugins,
  system_plugin: null,
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  triggers: [],
});

const catalog: Catalog = {
  key_id: 'catalog-2026',
  generated_at: '2026-11-08T00:00:00Z',
  plugins: [
    {
      id: 'srd5',
      version: '0.13.0',
      kind: 'system',
      license: 'CC-BY-4.0',
      app_compat: '>=0.0.1, <0.2.0',
      sha256: 'a'.repeat(64),
      revoked: false,
      installed_version: '0.12.0',
      compatible: true,
      can_install: true,
      dependencies: [],
    },
    {
      id: 'old-pack',
      version: '0.1.0',
      kind: 'content',
      license: 'MIT',
      app_compat: '>=0.0.1',
      sha256: 'b'.repeat(64),
      revoked: true,
      installed_version: null,
      compatible: true,
      can_install: false,
      dependencies: ['srd5 ^0.13'],
    },
  ],
};

function setup(
  options: {
    plugins?: Snapshot['plugins'];
    role?: CampaignInfo['role'];
    configured?: boolean;
    installResult?: IntentOutcome;
  } = {},
) {
  const {
    plugins = [],
    role = 'gm',
    configured = true,
    installResult = { status: 'accepted', seq: 2 },
  } = options;
  useSettings.getState().update({
    catalogUrl: configured ? 'https://example.org/plugins' : '',
    catalogKey: configured ? 'a'.repeat(64) : '',
  });
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      switch (cmd) {
        case 'campaign_current':
          return { ...campaign, role };
        case 'remote_current':
          return null;
        case 'get_snapshot':
          return snapshot(plugins);
        case 'catalog_fetch':
          return catalog;
        case 'catalog_install':
          return installResult;
        default:
          throw new Error(`unexpected command ${cmd}`);
      }
    },
    { shouldMockEvents: true },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CatalogView />
    </QueryClientProvider>,
  );
  return { calls };
}

const plugin = (id: string, version: string): Snapshot['plugins'][number] => ({
  id,
  version,
  kind: 'system',
  name: { en: id },
  license: 'MIT',
  attribution: null,
  archive_sha256: 'c'.repeat(64),
  system: null,
});

describe('CatalogView', () => {
  it('shows the signed index, the installed version and the trust of its key', async () => {
    setup({ plugins: [plugin('srd5', '0.12.0')] });
    expect((await screen.findAllByText('Signed by catalog catalog-2026')).length).toBe(2);
    expect(screen.getByText('Installed 0.12.0')).toBeInTheDocument();
    expect(screen.getByText('Revoked')).toBeInTheDocument();
    expect(screen.getByText('Depends on: srd5 ^0.13')).toBeInTheDocument();
  });

  it('installs an update through the catalog command', async () => {
    const { calls } = setup({ plugins: [plugin('srd5', '0.12.0')] });
    await screen.findAllByText('Signed by catalog catalog-2026');
    await userEvent.click(screen.getByRole('button', { name: 'Update' }));
    await waitFor(() => {
      expect(calls.some((call) => call.cmd === 'catalog_install')).toBe(true);
    });
    expect(calls.find((call) => call.cmd === 'catalog_install')?.args).toMatchObject({
      id: 'srd5',
      version: '0.13.0',
      baseUrl: 'https://example.org/plugins',
    });
  });

  it('never offers a revoked package, even with permissions', async () => {
    const { calls } = setup({ plugins: [] });
    await screen.findAllByText('Signed by catalog catalog-2026');
    const rows = screen.getAllByRole('listitem');
    const revoked = rows.find((row) => within(row).queryByText('Revoked'));
    expect(revoked).toBeDefined();
    expect(within(revoked as HTMLElement).queryByRole('button')).toBeNull();
    expect(calls.some((call) => call.cmd === 'catalog_install')).toBe(false);
  });

  it('asks for the address and key before talking to any server', async () => {
    const { calls } = setup({ configured: false });
    expect(
      await screen.findByText(/Packages from the catalog are not installed/),
    ).toBeInTheDocument();
    expect(calls.some((call) => call.cmd === 'catalog_fetch')).toBe(false);
    await userEvent.type(screen.getByLabelText('Catalog address'), 'https://example.org/plugins');
    await userEvent.type(screen.getByLabelText('Public key (hex)'), 'a'.repeat(64));
    await waitFor(() => {
      expect(calls.some((call) => call.cmd === 'catalog_fetch')).toBe(true);
    });
  });

  it('filters by the search box and tells a player the catalog is not theirs', async () => {
    setup({ plugins: [] });
    await screen.findAllByText('Signed by catalog catalog-2026');
    await userEvent.type(screen.getByRole('searchbox'), 'srd');
    expect(screen.queryByText('old-pack')).not.toBeInTheDocument();
    expect(screen.getByText('srd5')).toBeInTheDocument();
  });

  it('shows a rejected install as an error instead of success', async () => {
    setup({
      plugins: [],
      installResult: { status: 'rejected', reason: { code: 'invalid', field: 'plugin' } },
    });
    await screen.findAllByText('Signed by catalog catalog-2026');
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });
});
