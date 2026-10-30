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
  Snapshot,
  RemoteInfo,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

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
  render(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
  return { sent, calls };
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('media folder context commands', () => {
  it('renames a file with its version through a form', async () => {
    const { sent } = setup(<MediaFolderView library="images" folderId={null} />);
    const row = await screen.findByRole('option', { name: 'Great hall.png' });
    fireEvent.contextMenu(row);
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Rename file' }));
    const input = screen.getByRole('textbox');
    await userEvent.clear(input);
    await userEvent.type(input, 'Hall.png');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 1,
        intent: { kind: 'rename_asset', id: hall.id, name: 'Hall.png' },
      });
    });
  });
  it('creates a folder from the background menu', async () => {
    const { sent } = setup(<MediaFolderView library="images" folderId={null} />);
    fireEvent.contextMenu(await screen.findByRole('listbox'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'New folder' }));
    await userEvent.type(screen.getByRole('textbox'), 'Scenes');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_library_folder',
        library: 'images',
        parent: null,
        name: 'Scenes',
      });
    });
  });
  it.each(['images', 'audio'] as const)(
    'lets Co-GM rename and move a %s folder',
    async (library) => {
      const folder: LibraryFolderDto = {
        id: id(70),
        library,
        parent: null,
        name: 'Rooms',
        version: 3,
      };
      const destination = { ...folder, id: id(72), name: 'Archive' };
      const { sent } = setup(
        <MediaFolderView library={library} folderId={null} />,
        [],
        [folder, destination],
        null,
        'co_gm',
      );
      fireEvent.contextMenu(await screen.findByRole('option', { name: /Rooms/ }));
      await userEvent.click(await screen.findByRole('menuitem', { name: 'Rename “Rooms”' }));
      await userEvent.clear(screen.getByRole('textbox'));
      await userEvent.type(screen.getByRole('textbox'), 'Scenes');
      await userEvent.click(screen.getByRole('button', { name: 'Save' }));
      await waitFor(() => {
        expect(sent[0]).toMatchObject({
          expected_version: 3,
          intent: { kind: 'rename_library_folder', id: folder.id, name: 'Scenes' },
        });
      });
      fireEvent.contextMenu(await screen.findByRole('option', { name: /Rooms/ }));
      await userEvent.click(await screen.findByRole('menuitem', { name: 'Move to folder…' }));
      await userEvent.selectOptions(screen.getByRole('combobox'), destination.id);
      await userEvent.click(screen.getByRole('button', { name: 'Save' }));
      await waitFor(() => {
        expect(sent[1]).toMatchObject({
          expected_version: 3,
          intent: { kind: 'move_library_folder', id: folder.id, parent: destination.id },
        });
      });
    },
  );
  it('moves a file into a chosen folder with its version', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'images',
      parent: null,
      name: 'Rooms',
      version: 3,
    };
    const { sent } = setup(<MediaFolderView library="images" folderId={null} />, hall, [folder]);
    fireEvent.contextMenu(await screen.findByRole('option', { name: 'Great hall.png' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Move to folder…' }));
    await userEvent.selectOptions(screen.getByRole('combobox'), folder.id);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 1,
        intent: { kind: 'move_asset', id: hall.id, folder_id: folder.id },
      });
    });
  });
  it('excludes a folder and all its descendants from move destinations', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'images',
      parent: null,
      name: 'Rooms',
      version: 3,
    };
    const child = { ...folder, id: id(71), parent: folder.id, name: 'Cellar' };
    setup(<MediaFolderView library="images" folderId={null} />, hall, [folder, child]);
    fireEvent.contextMenu(await screen.findByRole('option', { name: /Rooms/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Move to folder…' }));
    expect(screen.getByRole('combobox').querySelectorAll('option')).toHaveLength(1);
  });
  it('confirms folder deletion with nested file count and supports cancellation', async () => {
    const folder: LibraryFolderDto = {
      id: id(70),
      library: 'images',
      parent: null,
      name: 'Rooms',
      version: 3,
    };
    const child = { ...folder, id: id(71), parent: folder.id, name: 'Cellar' };
    const { sent } = setup(
      <MediaFolderView library="images" folderId={null} />,
      { ...hall, folder_id: child.id },
      [folder, child],
    );
    const remove = async () => {
      fireEvent.contextMenu(await screen.findByRole('option', { name: /Rooms/ }));
      await userEvent.click(await screen.findByRole('menuitem', { name: 'Delete “Rooms”' }));
    };
    await remove();
    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'Its 1 files will move to the library root.',
    );
    expect(sent).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(sent).toEqual([]);
    await remove();
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: { kind: 'delete_library_folder', id: folder.id },
      });
    });
  });
  it.each(['player', 'spectator'] as const)(
    'offers %s only read-only opening commands',
    async (role) => {
      setup(<MediaFolderView library="images" folderId={null} />, hall, [], null, role);
      fireEvent.contextMenu(await screen.findByRole('option', { name: 'Great hall.png' }));
      expect(await screen.findByRole('menuitem', { name: 'Open' })).toBeInTheDocument();
      expect(screen.queryByRole('menuitem', { name: 'Rename file' })).not.toBeInTheDocument();
    },
  );
});
