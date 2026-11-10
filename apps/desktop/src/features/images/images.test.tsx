import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { MediaFolderView } from './MediaFolderView';
import type { AssetDto, CampaignInfo, LibraryFolderDto, Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { Markdown } from '@/shared/ui/Markdown';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const hall: AssetDto = {
  id: id(50),
  usage: 'token',
  sha256: 'a'.repeat(64),
  mime: 'image/png',
  size_bytes: 10,
  original_name: 'Great hall.png',
  width: 800,
  height: 600,
  display_sha256: 'b'.repeat(64),
  thumbnail_sha256: 'c'.repeat(64),
  tiles: [],
  version: 1,
};

function setup(
  node: React.ReactNode,
  asset: AssetDto | AssetDto[] = hall,
  libraryFolders: LibraryFolderDto[] = [],
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
    plugins: [],
    system_plugin: null,
    boards: [{ id: id(1), name: 'Plot', sort_order: 0, visible_to_players: true, version: 1 }],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
    assets: Array.isArray(asset) ? asset : [asset],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    library_folders: libraryFolders,
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role: 'gm' };
  mockIPC((cmd) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return null;
    if (cmd === 'asset_import_images') return [];
    if (cmd === 'submit_intent') return { status: 'accepted', seq: 2 };
    throw new Error(`unexpected command ${cmd}`);
  });
  // Asset URLs: the hash is all the tests look at.
  (
    window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
  ).__TAURI_INTERNALS__.convertFileSrc = (hash: string) => `asset://${hash}`;
  render(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('Media folder view', () => {
  it('shows only files from the chosen folder beside the selected full image', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'images',
      name: 'Rooms',
      parent: null,
      version: 1,
    };
    const filed = { ...hall, folder_id: folder.id };
    useSelection.getState().openMedia('images', folder.id, hall.id);
    setup(
      <MediaFolderView library="images" folderId={folder.id} />,
      [filed, { ...hall, id: id(51), original_name: 'Outside.png' }],
      [folder],
    );
    expect(await screen.findByRole('option', { name: 'Great hall.png' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.queryByRole('option', { name: 'Outside.png' })).not.toBeInTheDocument();
    expect(await screen.findByRole('img', { name: 'Great hall.png' })).toHaveAttribute(
      'src',
      `asset://${hall.display_sha256}`,
    );
  });
});

describe('pictures in journal text', () => {
  it('renders only the campaign’s own pictures, never a remote image', async () => {
    setup(
      <Markdown
        source={`![Great hall](asset:${hall.id}) and ![remote](https://example.com/x.png)`}
      />,
    );
    const img = await screen.findByRole('img', { name: 'Great hall' });
    expect(img.getAttribute('src')).toContain('b'.repeat(64));
    // A remote picture is never loaded: only its alt text is shown.
    expect(screen.queryByRole('img', { name: 'remote' })).not.toBeInTheDocument();
    expect(screen.getByText('remote')).toBeInTheDocument();
  });
});
