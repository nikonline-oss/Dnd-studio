import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { MediaFolderView } from './MediaFolderView';
import type {
  AssetDto,
  CampaignInfo,
  IntentEnvelope,
  LibraryFolderDto,
  PlaylistDto,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const audio = (n: number, name: string, folderId: string | null = null): AssetDto => ({
  id: id(n),
  usage: 'audio',
  sha256: n.toString(16).padStart(64, '0'),
  mime: 'audio/ogg',
  size_bytes: 10,
  original_name: name,
  width: 0,
  height: 0,
  display_sha256: null,
  thumbnail_sha256: n.toString(16).padStart(64, '0'),
  tiles: [],
  version: 1,
  folder_id: folderId,
});

const tavern: PlaylistDto = {
  id: id(40),
  name: 'Tavern',
  folder_id: null,
  tracks: [
    { asset_id: id(1), sha256: 'a'.repeat(64), mime: 'audio/ogg', title: 'Lute' },
    { asset_id: id(2), sha256: 'b'.repeat(64), mime: 'audio/ogg', title: 'Crowd' },
  ],
  version: 7,
};

function setup(
  node: React.ReactNode,
  assets: AssetDto[] = [],
  playlists: PlaylistDto[] = [],
  libraryFolders: LibraryFolderDto[] = [],
  role: CampaignInfo['role'] = 'gm',
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
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
    assets,
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    library_folders: libraryFolders,
    playlists,
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
  (
    window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }
  ).__TAURI_INTERNALS__.convertFileSrc = (hash: string) => `asset://${hash}`;
  render(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
  return { sent };
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('playlists in the audio library', () => {
  it('creates a playlist from the folder in one click, tracks ordered by name', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'audio',
      parent: null,
      name: 'Combat',
      version: 1,
    };
    const { sent } = setup(
      <MediaFolderView library="audio" folderId={folder.id} />,
      [audio(2, 'b.ogg', folder.id), audio(1, 'a.ogg', folder.id)],
      [],
      [folder],
    );
    fireEvent.contextMenu(await screen.findByRole('listbox'));
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'Playlist from this folder' }),
    );
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_playlist',
        name: 'Combat',
        folder_id: folder.id,
        asset_ids: [id(1), id(2)],
      });
    });
  });
  it('creates an empty playlist through the name form', async () => {
    const { sent } = setup(<MediaFolderView library="audio" folderId={null} />);
    fireEvent.contextMenu(await screen.findByRole('listbox'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'New playlist' }));
    await userEvent.type(screen.getByRole('textbox'), 'Ambient');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_playlist',
        name: 'Ambient',
        folder_id: null,
        asset_ids: [],
      });
    });
  });
  it('offers no playlist commands to players', async () => {
    setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern], [], 'player');
    fireEvent.contextMenu(await screen.findByRole('option', { name: 'Tavern' }));
    expect(await screen.findByRole('menuitem', { name: 'Open' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /Rename playlist/ })).not.toBeInTheDocument();
    fireEvent.contextMenu(await screen.findByRole('listbox'));
    expect(screen.queryByRole('menuitem', { name: 'New playlist' })).not.toBeInTheDocument();
  });
  it('renames a playlist keeping its tracks and version', async () => {
    const { sent } = setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern]);
    fireEvent.contextMenu(await screen.findByRole('option', { name: 'Tavern' }));
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'Rename playlist "Tavern"' }),
    );
    const input = screen.getByRole('textbox');
    await userEvent.clear(input);
    await userEvent.type(input, 'Tavern (night)');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 7,
        intent: {
          kind: 'update_playlist',
          id: tavern.id,
          name: 'Tavern (night)',
          tracks: tavern.tracks,
        },
      });
    });
  });
  it('moves a playlist into a chosen folder', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'audio',
      parent: null,
      name: 'Combat',
      version: 1,
    };
    const { sent } = setup(
      <MediaFolderView library="audio" folderId={null} />,
      [],
      [tavern],
      [folder],
    );
    fireEvent.contextMenu(await screen.findByRole('option', { name: 'Tavern' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Move to folder…' }));
    await userEvent.selectOptions(screen.getByRole('combobox'), folder.id);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 7,
        intent: { kind: 'move_playlist', id: tavern.id, folder_id: folder.id },
      });
    });
  });
  it('deletes a playlist after a confirmation', async () => {
    const { sent } = setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern]);
    fireEvent.contextMenu(await screen.findByRole('option', { name: 'Tavern' }));
    await userEvent.click(
      await screen.findByRole('menuitem', { name: 'Delete playlist "Tavern"' }),
    );
    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'Delete playlist "Tavern"? The tracks stay in the library.',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 7,
        intent: { kind: 'delete_playlist', id: tavern.id },
      });
    });
  });
  it('opens a playlist with personal playback and the GM transport', async () => {
    setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern]);
    await userEvent.click(await screen.findByRole('option', { name: 'Tavern' }));
    expect(await screen.findByRole('heading', { name: 'Tavern' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Play' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Play for everyone' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Lute' })).toBeInTheDocument();
    // The header and the player's idle line both show the count.
    expect(screen.getAllByText('2 tracks')).toHaveLength(2);
  });
  it('reorders tracks with their version', async () => {
    const { sent } = setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern]);
    await userEvent.click(await screen.findByRole('option', { name: 'Tavern' }));
    const [firstMoveDown] = await screen.findAllByRole('button', { name: 'Move down' });
    if (!firstMoveDown) throw new Error('no move-down button');
    await userEvent.click(firstMoveDown);
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 7,
        intent: {
          kind: 'update_playlist',
          id: tavern.id,
          tracks: [tavern.tracks[1], tavern.tracks[0]],
        },
      });
    });
  });
  it('removes a track from the list', async () => {
    const { sent } = setup(<MediaFolderView library="audio" folderId={null} />, [], [tavern]);
    await userEvent.click(await screen.findByRole('option', { name: 'Tavern' }));
    await userEvent.click(
      await screen.findByRole('button', { name: 'Remove "Lute" from the playlist' }),
    );
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 7,
        intent: { kind: 'update_playlist', id: tavern.id, tracks: [tavern.tracks[1]] },
      });
    });
  });
});
