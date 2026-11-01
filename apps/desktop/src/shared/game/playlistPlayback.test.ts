import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import {
  advanceSharedPlaylist,
  playSharedPlaylist,
  skipSharedPlaylist,
  stopSharedPlaylist,
  usePlaylistPlayback,
} from './playlistPlayback';
import type { IntentEnvelope, PlaylistDto, PlaylistTrackDto } from '@/shared/ipc/client';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
const track = (n: number): PlaylistTrackDto => ({
  asset_id: id(n),
  sha256: n.toString(16).padStart(64, '0'),
  mime: 'audio/ogg',
  title: `Track ${n}`,
});
const playlist: PlaylistDto = {
  id: id(40),
  name: 'Tavern',
  folder_id: null,
  tracks: [track(1), track(2), track(3)],
  version: 7,
};

/** Every intent is accepted, except playback commands for the listed (deleted) assets. */
function setup(goneAssets: string[] = []) {
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'submit_intent') {
      const envelope = (args as { envelope: IntentEnvelope }).envelope;
      sent.push(envelope);
      const intent = envelope.intent;
      const assetId =
        intent.kind === 'set_sound_playback' || intent.kind === 'stop_sound'
          ? intent.asset_id
          : null;
      if (assetId && goneAssets.includes(assetId))
        return { status: 'rejected', reason: { code: 'invalid', field: 'asset_id' } };
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  return sent;
}

afterEach(() => {
  usePlaylistPlayback.setState({ queue: null });
});

describe('shared playlist playback', () => {
  it('starts the queue at the requested track', async () => {
    const sent = setup();
    await playSharedPlaylist(playlist);
    expect(sent[0]?.intent).toMatchObject({
      kind: 'set_sound_playback',
      asset_id: id(1),
      paused: false,
      position_ms: 0,
      looped: false,
    });
    expect(usePlaylistPlayback.getState().queue).toMatchObject({
      playlistId: playlist.id,
      index: 0,
    });
  });
  it('skips tracks whose source left the library and fails when none can play', async () => {
    const sent = setup([id(1)]);
    await playSharedPlaylist(playlist);
    expect(sent.map((e) => e.intent)).toEqual([
      expect.objectContaining({ kind: 'set_sound_playback', asset_id: id(1) }),
      expect.objectContaining({ kind: 'set_sound_playback', asset_id: id(2) }),
    ]);
    expect(usePlaylistPlayback.getState().queue?.index).toBe(1);

    setup([id(1), id(2), id(3)]);
    await expect(playSharedPlaylist(playlist)).rejects.toThrow('invalid');
    expect(usePlaylistPlayback.getState().queue).toBeNull();
  });
  it('a natural end advances the queue; an unrelated end does not', async () => {
    const sent = setup();
    await playSharedPlaylist(playlist);
    await advanceSharedPlaylist(id(9));
    expect(sent).toHaveLength(1);
    await advanceSharedPlaylist(id(1));
    expect(sent[1]?.intent).toMatchObject({ kind: 'set_sound_playback', asset_id: id(2) });
    expect(usePlaylistPlayback.getState().queue?.index).toBe(1);
    await advanceSharedPlaylist(id(2));
    await advanceSharedPlaylist(id(3));
    expect(usePlaylistPlayback.getState().queue).toBeNull();
  });
  it('stop silences the current track and clears the queue', async () => {
    const sent = setup();
    await playSharedPlaylist(playlist, 1);
    await stopSharedPlaylist();
    expect(sent[1]?.intent).toMatchObject({ kind: 'stop_sound', asset_id: id(2) });
    expect(usePlaylistPlayback.getState().queue).toBeNull();
  });
  it('skip moves within the queue and stops the replaced track', async () => {
    const sent = setup();
    await playSharedPlaylist(playlist, 1);
    await skipSharedPlaylist(1);
    expect(sent.slice(1).map((e) => e.intent)).toEqual([
      expect.objectContaining({ kind: 'set_sound_playback', asset_id: id(3) }),
      expect.objectContaining({ kind: 'stop_sound', asset_id: id(2) }),
    ]);
    expect(usePlaylistPlayback.getState().queue?.index).toBe(2);
    await skipSharedPlaylist(1);
    expect(usePlaylistPlayback.getState().queue).toBeNull();
    expect(sent.at(-1)?.intent).toMatchObject({ kind: 'stop_sound', asset_id: id(3) });
  });
});
