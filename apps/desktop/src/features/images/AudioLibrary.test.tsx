import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { AudioLibrary } from './AudioLibrary';
import type { AssetDto, CampaignInfo, IntentEnvelope, Snapshot } from '@/shared/ipc/client';

const id = '0190a000-0000-7000-8000-000000000050';
const folderId = '0190a000-0000-7000-8000-000000000051';
const audio: AssetDto = {
  id,
  usage: 'audio',
  sha256: 'a'.repeat(64),
  mime: 'audio/mpeg',
  size_bytes: 10,
  original_name: 'Cavern.mp3',
  width: 0,
  height: 0,
  display_sha256: 'a'.repeat(64),
  thumbnail_sha256: 'a'.repeat(64),
  tiles: [],
  folder_id: folderId,
  version: 1,
};

describe('Audio library', () => {
  it('imports audio and broadcasts looping playback through an intent', async () => {
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
      assets: [audio],
      fog: [],
      combats: [],
      combatants: [],
      chat: [],
      library_folders: [
        { id: folderId, library: 'audio', parent: null, name: 'Ambience', version: 1 },
      ],
      sounds: [],
    };
    const info: CampaignInfo = { name: 'C', path: '/c', user_id: 'gm', role: 'gm' };
    const sent: IntentEnvelope[] = [];
    const calls: string[] = [];
    mockIPC((cmd, args) => {
      calls.push(cmd);
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'campaign_current') return info;
      if (cmd === 'remote_current') return null;
      if (cmd === 'asset_import_audio') return [];
      if (cmd === 'submit_intent') {
        sent.push((args as { envelope: IntentEnvelope }).envelope);
        return { status: 'accepted', seq: 2 };
      }
      throw new Error(`unexpected command ${cmd}`);
    });
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AudioLibrary />
      </QueryClientProvider>,
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Import audio' }));
    await waitFor(() => {
      expect(calls).toContain('asset_import_audio');
    });
    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }));
    const folderName = screen.getByRole('textbox', { name: 'Folder name' });
    await userEvent.type(folderName, 'Ambience');
    await userEvent.keyboard('{Enter}');
    await waitFor(() => {
      expect(sent.some(({ intent }) => intent.kind === 'create_library_folder')).toBe(true);
    });
    const folder = screen.getByRole('treeitem', { name: 'Ambience' });
    await userEvent.click(folder);
    expect(screen.queryByRole('treeitem', { name: 'Cavern.mp3' })).not.toBeInTheDocument();
    await userEvent.click(folder);
    expect(screen.getByRole('treeitem', { name: 'Cavern.mp3' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('treeitem', { name: 'Cavern.mp3' }));
    await userEvent.click(screen.getByRole('button', { name: 'Play music for everyone' }));
    await waitFor(() => {
      expect(
        sent.some(({ intent }) => intent.kind === 'play_sound' && intent.asset_id === id),
      ).toBe(true);
      expect(sent.find(({ intent }) => intent.kind === 'play_sound')?.intent).toMatchObject({
        kind: 'play_sound',
        asset_id: id,
        volume: 80,
        looped: true,
      });
    });
  });
});
