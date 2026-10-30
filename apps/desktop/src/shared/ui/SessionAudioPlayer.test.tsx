import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { SessionAudioPlayer, CompactAudioPlayers } from './SessionAudioPlayer';
import { useSoundPlayback } from '@/shared/game/soundPlayback';
import { useSelection } from '@/shared/state/selection';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';

const id = '0190a000-0000-7000-8000-000000000050';
function setup(role: Role, compact = false) {
  const sent: IntentEnvelope[] = [];
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
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
    assets: [
      {
        id,
        usage: 'audio',
        sha256: 'a'.repeat(64),
        mime: 'audio/ogg',
        size_bytes: 30,
        original_name: 'Secret cave.ogg',
        width: 0,
        height: 0,
        display_sha256: null,
        thumbnail_sha256: 'a'.repeat(64),
        tiles: [],
        version: 1,
      },
    ],
  };
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return { name: 'C', path: '/c', user_id: id, role };
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(cmd);
  });
  useSoundPlayback.setState({
    muted: false,
    tracks: [
      {
        sound: {
          asset_id: id,
          sha256: 'a'.repeat(64),
          mime: 'audio/ogg',
          volume: 80,
          looped: true,
        },
        position: 20,
        duration: 100,
        blocked: false,
      },
    ],
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      {compact ? <CompactAudioPlayers /> : <SessionAudioPlayer assetId={id} />}
    </QueryClientProvider>,
  );
  return sent;
}
afterEach(() => {
  useSoundPlayback.setState({ tracks: [], muted: false });
  useSelection.getState().reset();
});

describe('session music controls', () => {
  it.each(['player', 'spectator'] as const)(
    'lets %s mute personally without commands or private names',
    async (role) => {
      const sent = setup(role);
      await userEvent.click(await screen.findByRole('button', { name: 'Mute for me' }));
      expect(useSoundPlayback.getState().muted).toBe(true);
      expect(screen.queryByRole('button', { name: 'Pause' })).not.toBeInTheDocument();
      expect(screen.queryByRole('slider')).not.toBeInTheDocument();
      expect(screen.queryByText('Secret cave.ogg')).not.toBeInTheDocument();
      expect(sent).toEqual([]);
    },
  );
  it.each(['gm', 'co_gm'] as const)('sends %s pause and seek through intents', async (role) => {
    const sent = setup(role);
    await userEvent.click(await screen.findByRole('button', { name: 'Pause' }));
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'set_sound_playback',
        asset_id: id,
        paused: true,
        position_ms: 20000,
      });
    });
    const slider = screen.getByRole('slider', { name: /Playback position/ });
    fireEvent.change(slider, { target: { value: '45' } });
    fireEvent.pointerUp(slider);
    await waitFor(() => {
      expect(sent[1]?.intent).toMatchObject({ kind: 'set_sound_playback', position_ms: 45000 });
    });
  });
  it('allows keyboard seeking in the compact GM player', async () => {
    const sent = setup('gm', true);
    const slider = await screen.findByRole('slider', { name: /Playback position/ });
    fireEvent.change(slider, { target: { value: '30' } });
    expect(sent).toEqual([]);
    fireEvent.keyUp(slider, { key: 'ArrowRight' });
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({ kind: 'set_sound_playback', position_ms: 30000 });
    });
  });
  it('shows a compact player after the player tab is closed', async () => {
    useSelection.getState().openMedia('audio', null, id);
    setup('gm', true);
    await waitFor(() => {
      expect(screen.queryByRole('region', { name: 'Music player' })).not.toBeInTheDocument();
    });
    useSelection.getState().closeTab({ kind: 'audio', id: 'root' });
    expect(await screen.findByRole('button', { name: 'Open player' })).toBeInTheDocument();
    expect(useSoundPlayback.getState().tracks).toHaveLength(1);
  });

  it('keeps the compact player when the selected file is no longer in the open folder', async () => {
    useSelection.getState().openMedia('audio', 'previous-folder', id);
    setup('gm', true);
    expect(await screen.findByRole('button', { name: 'Open player' })).toBeInTheDocument();
  });
});
