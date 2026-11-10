import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockConvertFileSrc, mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { LibraryPanel } from './LibraryPanel';
import type { AssetDto, CampaignInfo, Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const asset = (id: string, name: string, usage: AssetDto['usage'], folder: string): AssetDto => ({
  id,
  usage,
  sha256: (id[0] ?? 'a').repeat(64),
  mime: usage === 'audio' ? 'audio/mpeg' : 'image/webp',
  size_bytes: 10,
  original_name: name,
  width: 0,
  height: 0,
  display_sha256: null,
  thumbnail_sha256: (id[0] ?? 'a').repeat(64),
  tiles: [],
  folder_id: folder,
  version: 1,
});

function library(): Snapshot {
  return {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [],
    users: [],
    journal: [
      {
        id: 'plan',
        title: 'Plan',
        folder_id: null,
        sort_order: 0,
        visibility: 'gm',
        players_can_edit: false,
        author: null,
        version: 3,
      },
    ],
    journal_folders: [{ id: 'lore', name: 'Lore', parent: null, sort_order: 0, version: 2 }],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
    assets: [
      asset('image', 'Cover.png', 'token', 'pictures'),
      asset('sound', 'Cavern.mp3', 'audio', 'sounds'),
    ],
    library_folders: [
      { id: 'pictures', library: 'images', name: 'Pictures', parent: null, version: 1 },
      { id: 'sounds', library: 'audio', name: 'Sounds', parent: null, version: 1 },
    ],
    playlists: [{ id: 'pl', name: 'Themes', folder_id: null, tracks: [], version: 1 }],
    sounds: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
}

function setup(snapshot: Snapshot = library()) {
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'u', role: 'gm' };
  mockIPC((cmd) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return null;
    throw new Error(`unexpected command ${cmd}`);
  });
  mockConvertFileSrc('windows');
  render(
    <QueryClientProvider client={new QueryClient()}>
      <LibraryPanel />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('LibraryPanel', () => {
  it('renders one explorer instead of stacked Journal, Pictures and Music sections', async () => {
    setup();
    await screen.findByRole('treeitem', { name: 'Lore' });
    const trees = screen.getAllByRole('tree');
    expect(trees).toHaveLength(1);
    expect(document.querySelector('details')).toBeNull();
    expect(document.querySelector('summary')).toBeNull();
    for (const name of ['Lore', 'Pictures', 'Sounds', 'Cover.png', 'Cavern.mp3', /Themes/]) {
      expect(screen.getByRole('treeitem', { name })).toBeInTheDocument();
    }
  });

  it('sorts folders first, then files by name, in the single tree', async () => {
    setup();
    await screen.findByRole('treeitem', { name: 'Lore' });
    const tree = screen.getByRole('tree');
    const names = Array.from(tree.querySelectorAll('[role="treeitem"]')).map(
      (row) => row.querySelector('.row-name')?.textContent ?? row.textContent,
    );
    expect(names).toEqual([
      'Lore',
      'Pictures',
      'Cover.png',
      'Sounds',
      'Cavern.mp3',
      'Plan',
      'Themes',
    ]);
  });

  it('opens a journal entry, a media folder and a playlist as tabs', async () => {
    setup();
    await userEvent.click(await screen.findByRole('treeitem', { name: 'Plan' }));
    expect(useSelection.getState().active).toEqual({ kind: 'journal', id: 'plan' });

    await userEvent.click(screen.getByRole('treeitem', { name: 'Pictures' }));
    expect(useSelection.getState().active).toEqual({ kind: 'images', id: 'pictures' });

    await userEvent.click(screen.getByRole('treeitem', { name: /Themes/ }));
    expect(useSelection.getState().active).toEqual({ kind: 'audio', id: 'root' });
    expect(useSelection.getState().mediaSelection['audio:root']).toBe('playlist:pl');
  });

  it('shows the empty art only while the campaign has nothing to explore', async () => {
    setup({
      ...library(),
      journal: [],
      assets: [],
      playlists: [],
    });
    expect(
      await screen.findByRole('img', { name: 'An enchanted lantern over a blank map' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('tree')).toBeInTheDocument();
  });
});
