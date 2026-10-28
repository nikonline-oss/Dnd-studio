import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { ImageLibrary, pictureMarkdown } from './ImageLibrary';
import type { AssetDto, CampaignInfo, IntentEnvelope, Snapshot } from '@/shared/ipc/client';
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

function setup(node: React.ReactNode) {
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
    assets: [hall],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role: 'gm' };
  const sent: IntentEnvelope[] = [];
  const calls: string[] = [];
  mockIPC((cmd, args) => {
    calls.push(cmd);
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return null;
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

describe('Image library', () => {
  it('imports pictures and puts one onto the open board', async () => {
    useSelection.getState().openBoard(id(1));
    const { sent, calls } = setup(<ImageLibrary />);
    await userEvent.click(await screen.findByRole('button', { name: 'Import pictures…' }));
    await waitFor(() => {
      expect(calls).toContain('asset_import_images');
    });
    await userEvent.click(screen.getByRole('treeitem', { name: /Great hall/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Onto the open board' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_board_node',
        board_id: id(1),
        content: { type: 'image', asset_id: hall.id },
      });
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
