import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createEvent, fireEvent, render, screen, within } from '@testing-library/react';
import { mockConvertFileSrc, mockIPC } from '@tauri-apps/api/mocks';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { UnifiedLibrary, pictureMarkdown } from './UnifiedLibrary';
import type {
  AssetDto,
  CampaignInfo,
  Library,
  LibraryFolderDto,
  PlaylistDto,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const { mutate } = vi.hoisted(() => ({ mutate: vi.fn() }));

interface TestState {
  assets: AssetDto[];
  folders: LibraryFolderDto[];
  playlists: PlaylistDto[];
  sounds: Snapshot['sounds'];
  remote: { status: { state: 'online' }; hosted: boolean } | null;
  calls: string[];
}

const state = vi.hoisted<TestState>(() => ({
  assets: [],
  folders: [],
  playlists: [],
  sounds: [],
  remote: null,
  calls: [],
}));

vi.mock('@/shared/game/useGame', () => ({
  useSnapshotSelect: (
    select: (snapshot: {
      library_folders: LibraryFolderDto[];
      assets: AssetDto[];
      playlists: PlaylistDto[];
      sounds: TestState['sounds'];
      journal: never[];
      journal_folders: never[];
    }) => unknown,
  ) =>
    select({
      library_folders: state.folders,
      assets: state.assets,
      playlists: state.playlists,
      sounds: state.sounds,
      journal: [],
      journal_folders: [],
    }),
  useSubmitIntent: () => ({ mutate, isPending: false, error: null }),
  useCampaign: () => ({
    data: { name: 'C', path: '/c', user_id: 'u', role: 'gm' } satisfies CampaignInfo,
  }),
}));

vi.mock('@/shared/game/useRemote', () => ({
  useRemote: () => state.remote,
}));

const asset = (id: string, library: Library, overrides: Partial<AssetDto> = {}): AssetDto => ({
  id,
  usage: library === 'audio' ? 'audio' : 'token',
  sha256: 'a'.repeat(64),
  mime: library === 'audio' ? 'audio/mpeg' : 'image/webp',
  size_bytes: 10,
  original_name: 'Source',
  width: 0,
  height: 0,
  display_sha256: null,
  thumbnail_sha256: 'a'.repeat(64),
  tiles: [],
  folder_id: null,
  version: 3,
  ...overrides,
});

const imageFolder: LibraryFolderDto = {
  id: 'folder',
  library: 'images',
  name: 'Pictures',
  parent: null,
  version: 2,
};
const audioFolder: LibraryFolderDto = {
  id: 'audio-folder',
  library: 'audio',
  name: 'Sounds',
  parent: null,
  version: 2,
};

beforeEach(() => {
  state.assets = [];
  state.folders = [imageFolder, audioFolder];
  state.playlists = [];
  state.sounds = [];
  state.remote = null;
  state.calls = [];
  localStorage.clear();
  mutate.mockClear();
  mockConvertFileSrc('windows');
  mockIPC((cmd) => {
    state.calls.push(cmd);
    if (cmd === 'asset_import_images' || cmd === 'asset_import_audio') return [];
    throw new Error(`unexpected command ${cmd}`);
  });
});

afterEach(() => {
  useSelection.getState().reset();
});

/** The explorer imports files through TanStack mutations, so it needs a query client. */
function renderLibrary() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <UnifiedLibrary />
    </QueryClientProvider>,
  );
}

function setup(library: Library) {
  const source = asset('asset', library);
  state.assets = [source];
  const view = renderLibrary();
  const sourceRow = screen.getByRole('treeitem', { name: 'Source' });
  const target = screen.getByRole('treeitem', {
    name: library === 'audio' ? 'Sounds' : 'Pictures',
  });
  const root = view.container.querySelector('.library-tree-root');
  if (!root) throw new Error('missing library root');
  const dataTransfer = { setData: vi.fn(), effectAllowed: 'none', dropEffect: 'none' };
  fireEvent.dragStart(sourceRow, { dataTransfer });
  return { ...view, root, source: sourceRow, target, dataTransfer, asset: source };
}

function leave(target: Element, relatedTarget: Element | null) {
  const event = createEvent.dragLeave(target);
  Object.defineProperty(event, 'relatedTarget', { value: relatedTarget });
  fireEvent(target, event);
}

it('renames an image from its context menu using its checked version', async () => {
  const { source } = setup('images');
  fireEvent.dragEnd(source);
  fireEvent.contextMenu(source);
  await userEvent.click(screen.getByRole('menuitem', { name: /rename/i }));
  const input = screen.getByRole('textbox', { name: /rename/i });
  await userEvent.clear(input);
  await userEvent.type(input, 'New image{Enter}');
  expect(mutate).toHaveBeenCalledWith({
    intent: { kind: 'rename_asset', id: 'asset', name: 'New image' },
    expectedVersion: 3,
  });
});

it('opens a folder from its row while toggling expansion, but not from its context menu', () => {
  renderLibrary();
  const folder = screen.getByRole('treeitem', { name: 'Pictures' });
  expect(folder).toHaveAttribute('aria-expanded', 'true');
  fireEvent.click(screen.getByText('Pictures'));
  expect(folder).toHaveAttribute('aria-expanded', 'false');
  expect(useSelection.getState().active).toEqual({ kind: 'images', id: 'folder' });
  fireEvent.contextMenu(folder);
  expect(useSelection.getState().active).toEqual({ kind: 'images', id: 'folder' });
  expect(folder).toHaveAttribute('aria-expanded', 'false');
});

describe.each<Library>(['images', 'audio'])('%s library drag cleanup', (library) => {
  it('highlights only the nearest target and clears all outlines after a bubbling drop', () => {
    const { container, root, target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    expect(target).toHaveAttribute('data-drop', 'true');
    expect(root).not.toHaveAttribute('data-drop', 'true');
    fireEvent.drop(target, { dataTransfer });
    expect(mutate).toHaveBeenCalledTimes(1);
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'move_asset',
        id: 'asset',
        folder_id: library === 'audio' ? 'audio-folder' : 'folder',
      },
      expectedVersion: 3,
    });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it('moves the outline between targets and clears it when leaving an asset row', () => {
    const { container, root, source, target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    fireEvent.dragOver(source, { dataTransfer });
    expect(target).not.toHaveAttribute('data-drop', 'true');
    expect(source).toHaveAttribute('data-drop', 'true');
    leave(source, root);
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it('keeps the target highlighted when crossing its children', () => {
    const { target, dataTransfer } = setup(library);
    fireEvent.dragOver(target, { dataTransfer });
    leave(target, target.querySelector('.row-name'));
    expect(target).toHaveAttribute('data-drop', 'true');
  });

  it('clears a folder drag on dragend and does not treat its own row as a root drop', () => {
    const { container, root, target, dataTransfer } = setup(library);
    fireEvent.dragEnd(screen.getByRole('treeitem', { name: 'Source' }));
    fireEvent.dragStart(target, { dataTransfer });
    fireEvent.dragOver(root, { dataTransfer });
    expect(root).toHaveAttribute('data-drop', 'true');
    fireEvent.dragOver(target, { dataTransfer });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
    fireEvent.drop(target, { dataTransfer });
    expect(mutate).not.toHaveBeenCalled();
    fireEvent.dragStart(target, { dataTransfer });
    fireEvent.dragOver(root, { dataTransfer });
    fireEvent.dragEnd(target, { dataTransfer });
    expect(container.querySelector('[data-drop="true"]')).toBeNull();
  });

  it.each(['dragend', 'escape', 'blur', 'outside-drop'])(
    'clears the outline and stale drag payload after %s',
    (ending) => {
      const { container, source, target, dataTransfer } = setup(library);
      fireEvent.dragOver(target, { dataTransfer });
      if (ending === 'dragend') fireEvent.dragEnd(source, { dataTransfer });
      if (ending === 'escape') fireEvent.keyDown(window, { key: 'Escape' });
      if (ending === 'blur') fireEvent.blur(window);
      if (ending === 'outside-drop') fireEvent.drop(document.body, { dataTransfer });
      expect(container.querySelector('[data-drop="true"]')).toBeNull();
      fireEvent.dragOver(target, { dataTransfer });
      fireEvent.drop(target, { dataTransfer });
      expect(mutate).not.toHaveBeenCalled();
    },
  );
});

describe('audio playlists in the unified tree', () => {
  it('creates a playlist from the toolbar', async () => {
    renderLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'New playlist' }));
    await userEvent.type(
      screen.getByRole('textbox', { name: 'New playlist' }),
      'Boss themes{Enter}',
    );
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'create_playlist',
        id: expect.any(String) as string,
        name: 'Boss themes',
        folder_id: null,
        asset_ids: [],
      },
    });
  });

  it('shows pictures, sounds and playlists in one tree', () => {
    state.assets = [
      asset('image', 'images', { original_name: 'Cover.png', folder_id: 'folder' }),
      asset('sound', 'audio', { original_name: 'Cavern.mp3', folder_id: 'audio-folder' }),
    ];
    state.playlists = [{ id: 'pl', name: 'Themes', folder_id: null, tracks: [], version: 7 }];
    renderLibrary();
    const tree = screen.getByRole('tree');
    expect(within(tree).getByRole('treeitem', { name: 'Cover.png' })).toBeInTheDocument();
    expect(within(tree).getByRole('treeitem', { name: 'Cavern.mp3' })).toBeInTheDocument();
    expect(within(tree).getByRole('treeitem', { name: /Themes/ })).toBeInTheDocument();
  });

  it('imports audio, creates a folder of its kind and broadcasts looping playback', async () => {
    state.assets = [asset('sound', 'audio', { original_name: 'Cavern.mp3' })];
    renderLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Import audio' }));
    expect(state.calls).toContain('asset_import_audio');
    await userEvent.click(screen.getByRole('treeitem', { name: 'Cavern.mp3' }));
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'New audio folder' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Folder name' }), 'Ambience{Enter}');
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'create_library_folder',
        id: expect.any(String) as string,
        library: 'audio',
        parent: null,
        name: 'Ambience',
      },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Cavern.mp3' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Play music for everyone' }));
    expect(mutate).toHaveBeenCalledWith({
      intent: { kind: 'play_sound', asset_id: 'sound', volume: 80, looped: true },
    });
  });

  it('offers play, rename and delete on a playlist row', () => {
    state.playlists = [{ id: 'pl', name: 'Themes', folder_id: null, tracks: [], version: 7 }];
    renderLibrary();
    fireEvent.contextMenu(screen.getByRole('treeitem', { name: /Themes/ }));
    expect(screen.getByRole('menuitem', { name: 'Play for everyone' })).toBeDisabled();
    expect(screen.getByRole('menuitem', { name: 'Rename playlist "Themes"' })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: 'Delete playlist "Themes"' })).toBeInTheDocument();
  });

  it('drops an audio asset onto a playlist row as a new track', () => {
    state.playlists = [{ id: 'pl', name: 'Themes', folder_id: null, tracks: [], version: 7 }];
    const dataTransfer = { setData: vi.fn(), effectAllowed: 'none', dropEffect: 'none' };
    const { source } = setup('audio');
    const row = screen.getByRole('treeitem', { name: /Themes/ });
    fireEvent.dragStart(source, { dataTransfer });
    fireEvent.dragOver(row, { dataTransfer });
    expect(row).toHaveAttribute('data-drop', 'true');
    fireEvent.drop(row, { dataTransfer });
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_playlist',
        id: 'pl',
        name: 'Themes',
        tracks: [
          { asset_id: 'asset', sha256: 'a'.repeat(64), mime: 'audio/mpeg', title: 'Source' },
        ],
      },
      expectedVersion: 7,
    });
  });

  it('does not duplicate a track already in the playlist', () => {
    state.playlists = [
      {
        id: 'pl',
        name: 'Themes',
        folder_id: null,
        version: 7,
        tracks: [
          { asset_id: 'asset', sha256: 'a'.repeat(64), mime: 'audio/mpeg', title: 'Source' },
        ],
      },
    ];
    const dataTransfer = { setData: vi.fn(), effectAllowed: 'none', dropEffect: 'none' };
    const { source } = setup('audio');
    const row = screen.getByRole('treeitem', { name: /Themes/ });
    fireEvent.dragStart(source, { dataTransfer });
    fireEvent.dragOver(row, { dataTransfer });
    expect(row).not.toHaveAttribute('data-drop', 'true');
    fireEvent.drop(row, { dataTransfer });
    expect(mutate).not.toHaveBeenCalled();
  });
});

describe('pictures in the unified tree', () => {
  it('opens the clicked image in its folder tab', async () => {
    setup('images');
    await userEvent.click(screen.getByRole('treeitem', { name: 'Source' }));
    expect(useSelection.getState().active).toEqual({ kind: 'images', id: 'root' });
    expect(useSelection.getState().mediaSelection['images:root']).toBe('asset');
  });

  it('imports pictures while connected to an online session', async () => {
    state.remote = { status: { state: 'online' }, hosted: false };
    renderLibrary();
    await userEvent.click(screen.getByRole('button', { name: 'Import' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Import pictures…' }));
    expect(state.calls).toContain('asset_import_images');
  });

  it('puts a picture onto the open board', async () => {
    state.assets = [asset('asset', 'images')];
    useSelection.getState().openBoard('board');
    renderLibrary();
    await userEvent.click(screen.getByRole('treeitem', { name: 'Source' }));
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Source' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Onto the open board' }));
    expect(mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'create_board_node',
        id: expect.any(String) as string,
        board_id: 'board',
        content: { type: 'image', asset_id: 'asset' },
        x: 0,
        y: 0,
        width: 3840,
        height: 2880,
        visibility: 'gm',
      },
    });
  });

  it('shows the full image in a hover preview below its thumbnail', async () => {
    state.assets = [asset('map', 'images', { usage: 'map', thumbnail_sha256: 'c'.repeat(64) })];
    renderLibrary();
    const item = screen.getByRole('treeitem', { name: 'Source' });
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
    state.assets = [asset('asset', 'images', { folder_id: 'folder' })];
    state.folders = [{ ...imageFolder, name: 'Maps' }];
    renderLibrary();
    const item = screen.getByRole('treeitem', { name: 'Source' });
    await userEvent.click(item);
    expect(item).toHaveAttribute('aria-selected', 'true');
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Source' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Move to library root' }));
    expect(item).toHaveAttribute('aria-selected', 'false');
    expect(mutate).toHaveBeenCalledWith({
      intent: { kind: 'move_asset', id: 'asset', folder_id: null },
      expectedVersion: 3,
    });
  });

  it('clears the selected file after moving it into a folder', async () => {
    state.assets = [asset('asset', 'images')];
    state.folders = [{ ...imageFolder, name: 'Maps' }];
    renderLibrary();
    const item = screen.getByRole('treeitem', { name: 'Source' });
    await userEvent.click(item);
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Source' }));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Move to folder: Maps' }));
    expect(item).toHaveAttribute('aria-selected', 'false');
    expect(mutate).toHaveBeenCalledWith({
      intent: { kind: 'move_asset', id: 'asset', folder_id: 'folder' },
      expectedVersion: 3,
    });
  });

  it('writes a picture for the journal', () => {
    const image = { ...asset('asset', 'images'), original_name: 'Great hall.png' };
    expect(pictureMarkdown(image)).toBe('![Great hall](asset:asset)');
  });
});
