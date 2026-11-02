import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { ImageLibrary, pictureMarkdown } from './ImageLibrary';
import { MediaFolderView } from './MediaFolderView';
import { ImagePresentation } from './ImagePresentation';
import type {
  AssetDto,
  CampaignInfo,
  IntentEnvelope,
  LibraryFolderDto,
  Snapshot,
  RemoteInfo,
} from '@/shared/ipc/client';
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
  remote: RemoteInfo | null = null,
  presentation: Snapshot['presented_image'] = null,
  role: CampaignInfo['role'] = 'gm',
) {
  const snapshot: Snapshot = {
    presented_image: presentation,
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
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role };
  const sent: IntentEnvelope[] = [];
  const calls: string[] = [];
  mockIPC((cmd, args) => {
    calls.push(cmd);
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return remote;
    if (cmd === 'asset_import_images') return [];
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  // Asset URLs: the hash is all the tests look at.
  (
    window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
  ).__TAURI_INTERNALS__.convertFileSrc = (hash: string) => `asset://${hash}`;
  const client = new QueryClient();
  render(<QueryClientProvider client={client}>{node}</QueryClientProvider>);
  return { sent, calls, client, snapshot };
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('Image library', () => {
  it('lets a player dismiss a presentation locally and reopens a new show of the same art', async () => {
    const presentation = { id: id(500), asset_id: hall.id, version: 1 };
    const { sent, client, snapshot } = setup(
      <ImagePresentation />,
      hall,
      [],
      null,
      presentation,
      'player',
    );
    const dialog = await screen.findByRole('dialog', { name: 'Image from the GM' });
    expect(screen.getByRole('img', { name: 'Great hall.png' })).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(sent).toHaveLength(0);
    act(() => {
      client.setQueryData(['snapshot'], {
        ...snapshot,
        presented_image: { ...presentation, id: id(501) },
      });
    });
    expect(await screen.findByRole('dialog', { name: 'Image from the GM' })).not.toBe(dialog);
    act(() => {
      client.setQueryData(['snapshot'], { ...snapshot, presented_image: null });
    });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });
  it('lets a GM close the current show for everyone with its version', async () => {
    const { sent } = setup(<ImagePresentation />, hall, [], null, {
      id: id(500),
      asset_id: hall.id,
      version: 7,
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Stop showing to everyone' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      intent: { kind: 'hide_image', id: id(500) },
      expected_version: 7,
    });
  });
  it('lets a GM dismiss locally and keeps the shared stop command available', async () => {
    const { sent } = setup(<ImagePresentation />, hall, [], null, {
      id: id(500),
      asset_id: hall.id,
      version: 1,
    });
    await screen.findByRole('dialog');
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(sent).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Stop showing to everyone' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'View image' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });
  it('keeps the close control accessible if an image cannot load', async () => {
    setup(
      <ImagePresentation />,
      hall,
      [],
      null,
      { id: id(500), asset_id: hall.id, version: 1 },
      'player',
    );
    const image = await screen.findByRole('img', { name: 'Great hall.png' });
    fireEvent.error(image.querySelector('img') ?? image);
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not load the image.');
    await userEvent.click(screen.getByRole('button', { name: 'Close image' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('opens the clicked image in its folder tab', async () => {
    setup(<ImageLibrary />);
    await userEvent.click(await screen.findByRole('treeitem', { name: /Great hall/ }));
    expect(useSelection.getState().active).toEqual({ kind: 'images', id: 'root' });
    expect(useSelection.getState().mediaSelection['images:root']).toBe(hall.id);
  });

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
  it('imports pictures while connected to an online session', async () => {
    const { calls } = setup(<ImageLibrary />, hall, [], {
      server: 'http://localhost:8080',
      session_id: id(100),
      session_name: 'C',
      role: 'gm',
      hosted: false,
      status: { state: 'online' },
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Import pictures…' }));
    await waitFor(() => {
      expect(calls).toContain('asset_import_images');
    });
  });

  it('imports pictures and puts one onto the open board', async () => {
    useSelection.getState().openBoard(id(1));
    const { sent, calls } = setup(<ImageLibrary />);
    await userEvent.click(await screen.findByRole('button', { name: 'Import pictures…' }));
    await waitFor(() => {
      expect(calls).toContain('asset_import_images');
    });
    await userEvent.click(screen.getByRole('treeitem', { name: /Great hall/ }));
    expect(screen.queryByRole('button', { name: 'Onto the open board' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Great hall.png' }));
    expect(screen.getByRole('menuitem', { name: 'Onto the open board' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: 'Onto the open board' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_board_node',
        board_id: id(1),
        content: { type: 'image', asset_id: hall.id },
      });
    });
  });

  it('shows the full image in a hover preview below its thumbnail', async () => {
    const mapImage: AssetDto = { ...hall, usage: 'map', display_sha256: null };
    setup(<ImageLibrary />, mapImage);
    const item = await screen.findByRole('treeitem', { name: /Great hall/ });
    const thumbnail = item.querySelector('img');
    if (!thumbnail) throw new Error('Image thumbnail was not rendered');
    fireEvent.mouseEnter(thumbnail);
    expect(await screen.findByRole('tooltip')).toBeInTheDocument();
    expect(screen.getByRole('tooltip').querySelector('img')).toHaveAttribute(
      'src',
      expect.stringContaining('c'.repeat(64)),
    );
  });

  it('clears the selected file after moving it out of a folder', async () => {
    const folder: LibraryFolderDto = {
      id: id(7),
      library: 'images',
      parent: null,
      name: 'Maps',
      version: 1,
    };
    const filed = { ...hall, folder_id: folder.id };
    const { sent } = setup(<ImageLibrary />, filed, [folder]);
    const item = await screen.findByRole('treeitem', { name: /Great hall/ });
    await userEvent.click(item);
    expect(item).toHaveAttribute('aria-selected', 'true');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Great hall.png' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Move to library root' }));
    expect(item).toHaveAttribute('aria-selected', 'false');
    await waitFor(() => {
      expect(sent.some(({ intent }) => intent.kind === 'move_asset')).toBe(true);
    });
  });

  it('clears the selected file after moving it into a folder', async () => {
    const folder: LibraryFolderDto = {
      id: id(8),
      library: 'images',
      parent: null,
      name: 'Maps',
      version: 1,
    };
    const { sent } = setup(<ImageLibrary />, hall, [folder]);
    const item = await screen.findByRole('treeitem', { name: /Great hall/ });
    await userEvent.click(item);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Great hall.png' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Move to folder: Maps' }));
    expect(item).toHaveAttribute('aria-selected', 'false');
    await waitFor(() => {
      expect(sent.some(({ intent }) => intent.kind === 'move_asset')).toBe(true);
    });
  });

  it('writes pictures for the journal, and the journal shows only the campaign’s own', async () => {
    expect(pictureMarkdown(hall)).toBe(`![Great hall](asset:${hall.id})`);
    setup(
      <Markdown source={`${pictureMarkdown(hall)} and ![remote](https://example.com/x.png)`} />,
    );
    const img = await screen.findByRole('img', { name: 'Great hall' });
    expect(img.getAttribute('src')).toContain('b'.repeat(64));
    // A remote picture is never loaded: only its alt text is shown.
    expect(screen.queryByRole('img', { name: 'remote' })).not.toBeInTheDocument();
    expect(screen.getByText('remote')).toBeInTheDocument();
  });
});
