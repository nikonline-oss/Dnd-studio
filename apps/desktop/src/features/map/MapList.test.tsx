import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockConvertFileSrc, mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { MapInspector } from './MapInspector';
import { MapList } from './MapList';
import type {
  AssetDto,
  CampaignInfo,
  IntentEnvelope,
  IntentOutcome,
  IpcError,
  MapImported,
  Role,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { useToast } from '@/shared/ui/Toast';

const MAP_ID = '0190a000-0000-7000-8000-000000000001';
const ASSET_ID = '0190a000-0000-7000-8000-000000000002';
const THUMB = 'ee'.repeat(32);

const asset: AssetDto = {
  id: ASSET_ID,
  usage: 'map',
  sha256: 'aa'.repeat(32),
  mime: 'image/png',
  size_bytes: 1234,
  original_name: 'crypt.png',
  width: 4000,
  height: 3000,
  display_sha256: null,
  thumbnail_sha256: THUMB,
  tiles: [],
  version: 1,
};

const snapshot: Snapshot = {
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  maps: [
    {
      id: MAP_ID,
      name: 'Crypt',
      background_asset_id: ASSET_ID,
      width: 4000,
      height: 3000,
      grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
      visible_to_players: false,
      version: 3,
    },
  ],
  tokens: [],
  characters: [],
  users: [],
  journal: [],
  journal_folders: [],
  plugins: [],
  system_plugin: null,
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  assets: [asset],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
};

interface Options {
  outcome?: IntentOutcome;
  role?: Role;
  imported?: MapImported | null | { error: IpcError };
}

function renderWithIpc(node: ReactNode, options: Options = {}) {
  const { outcome = { status: 'accepted', seq: 2 }, role = 'gm', imported = null } = options;
  const info: CampaignInfo = {
    name: 'C',
    path: '/c',
    user_id: '0190a000-0000-7000-8000-0000000000aa',
    role,
  };
  const sent: IntentEnvelope[] = [];
  const commands: string[] = [];
  mockIPC((cmd, args) => {
    commands.push(cmd);
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return outcome;
    }
    if (cmd === 'asset_import_map') {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- Tauri rejects with the plain error object
      if (imported && 'error' in imported) throw imported.error;
      return imported;
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
  return { sent, commands };
}

afterEach(() => {
  useSelection.setState({ mapId: null });
  useToast.setState({ message: null });
});

describe('MapList', () => {
  it('shows maps from the snapshot', async () => {
    renderWithIpc(<MapList />);
    expect(await screen.findByText('Crypt')).toBeInTheDocument();
  });

  it('sends create_map with a client-generated v7 id', async () => {
    const { sent } = renderWithIpc(<MapList />);
    await userEvent.click(await screen.findByRole('button', { name: 'Add map' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    const envelope = sent[0];
    expect(envelope?.expected_version).toBeNull();
    expect(envelope?.intent).toMatchObject({ kind: 'create_map', name: 'New map 2' });
    expect(envelope?.intent_id).toMatch(/^[0-9a-f-]{14}7/);
  });

  it('passes the entity version on updates (ADR-0003)', async () => {
    const { sent } = renderWithIpc(<MapList />);
    await userEvent.click(await screen.findByRole('checkbox'));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'update_map', visible_to_players: true },
    });
  });

  it('shows a localized message when the core rejects an intent', async () => {
    renderWithIpc(<MapList />, {
      outcome: { status: 'rejected', reason: { code: 'stale_version', current: 4 } },
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete?' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Someone changed this before you');
  });

  it('deletes only after a confirming second click', async () => {
    const { sent } = renderWithIpc(<MapList />);
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    expect(sent).toHaveLength(0);
    // Clicking elsewhere disarms the button.
    await userEvent.click(document.body);
    expect(screen.queryByRole('button', { name: 'Delete?' })).not.toBeInTheDocument();
    expect(sent).toHaveLength(0);

    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete?' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 3,
      intent: { kind: 'delete_map', id: MAP_ID },
    });
  });

  it('imports an image, selects the new map and reports a downscale', async () => {
    const newMap = '0190a000-0000-7000-8000-000000000009';
    const { commands } = renderWithIpc(<MapList />, {
      imported: { map_id: newMap, downscaled: true, source_width: 20000, source_height: 9000 },
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Import image…' }));
    await waitFor(() => {
      expect(useSelection.getState().mapId).toBe(newMap);
    });
    expect(commands).toContain('asset_import_map');
    expect(useToast.getState().message).toContain('20000×9000');
  });

  it('does nothing when the file dialog is cancelled', async () => {
    const { commands } = renderWithIpc(<MapList />, { imported: null });
    await userEvent.click(await screen.findByRole('button', { name: 'Import image…' }));
    await waitFor(() => {
      expect(commands).toContain('asset_import_map');
    });
    expect(useSelection.getState().mapId).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows a localized message for a rejected image', async () => {
    renderWithIpc(<MapList />, {
      imported: { error: { kind: 'user', code: 'asset_svg', message: 'svg' } },
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Import image…' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('SVG images are not supported');
  });

  it('hides image import from players', async () => {
    renderWithIpc(<MapList />, { role: 'player' });
    expect(await screen.findByText('Crypt')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Import image…' })).not.toBeInTheDocument();
  });
});

describe('MapInspector', () => {
  it('shows the background thumbnail served by the dnd-asset scheme', async () => {
    mockConvertFileSrc('windows');
    useSelection.setState({ mapId: MAP_ID });
    renderWithIpc(<MapInspector />);
    const img = await screen.findByRole('img', { name: 'crypt.png' });
    expect(img).toHaveAttribute('src', `http://dnd-asset.localhost/${THUMB}`);
    expect(screen.getByText('4000 × 3000')).toBeInTheDocument();
  });
});
