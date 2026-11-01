import { create } from 'zustand';
import {
  commands,
  IntentRejected,
  unwrap,
  type Intent,
  type PlaylistDto,
  type PlaylistTrackDto,
} from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';

/** The playlist the GM is broadcasting right now; `null` when nothing is queued. */
export interface SharedPlaylistQueue {
  playlistId: string;
  title: string;
  index: number;
  /** Copied on start: edits during playback do not reshape a running queue. */
  tracks: PlaylistTrackDto[];
}

export const usePlaylistPlayback = create<{ queue: SharedPlaylistQueue | null }>()(() => ({
  queue: null,
}));

async function submit(intent: Intent): Promise<void> {
  const outcome = await unwrap(
    commands.submitIntent({ intent_id: uuidv7(), expected_version: null, intent }),
  );
  if (outcome.status === 'rejected') throw new IntentRejected(outcome.reason);
}

async function startTrack(track: PlaylistTrackDto): Promise<void> {
  await submit({
    kind: 'set_sound_playback',
    asset_id: track.asset_id,
    paused: false,
    position_ms: 0,
    volume: 80,
    looped: false,
  });
}

/**
 * Starts tracks in order, skipping ones whose source asset left the library (the core rejects
 * shared playback of those; local playback by hash still works). Resolves with the index that
 * actually started, or `null` when nothing in the range could play.
 */
async function startFrom(
  queue: SharedPlaylistQueue,
  index: number,
  step: 1 | -1,
): Promise<number | null> {
  for (let i = index; i >= 0 && i < queue.tracks.length; i += step) {
    const track = queue.tracks[i];
    if (!track) continue;
    try {
      await startTrack(track);
    } catch {
      continue;
    }
    return i;
  }
  return null;
}

/** GM: broadcast a playlist to the session, starting at `index`. Throws when nothing can play. */
export async function playSharedPlaylist(playlist: PlaylistDto, index = 0): Promise<void> {
  const queue: SharedPlaylistQueue = {
    playlistId: playlist.id,
    title: playlist.name,
    index,
    tracks: playlist.tracks,
  };
  const started = await startFrom(queue, index, 1);
  if (started === null) {
    usePlaylistPlayback.setState({ queue: null });
    throw new IntentRejected({ code: 'invalid', field: 'asset_id' });
  }
  usePlaylistPlayback.setState({ queue: { ...queue, index: started } });
}

/** GM: stop the broadcast and clear the queue. */
export async function stopSharedPlaylist(): Promise<void> {
  const queue = usePlaylistPlayback.getState().queue;
  usePlaylistPlayback.setState({ queue: null });
  const track = queue?.tracks[queue.index];
  if (track) await submit({ kind: 'stop_sound', asset_id: track.asset_id });
}

/** GM: jump within the queue; stops at the edges (delta past the end ends the broadcast). */
export async function skipSharedPlaylist(delta: 1 | -1): Promise<void> {
  const queue = usePlaylistPlayback.getState().queue;
  if (!queue) return;
  const started = await startFrom(queue, queue.index + delta, delta);
  if (started === null) {
    await stopSharedPlaylist();
    return;
  }
  const previous = queue.tracks[queue.index];
  if (previous && started !== queue.index) {
    await submit({ kind: 'stop_sound', asset_id: previous.asset_id });
  }
  usePlaylistPlayback.setState({ queue: { ...queue, index: started } });
}

/**
 * Natural end of a track (never an explicit stop): advance the queue if the finished track is the
 * one being broadcast. Wired only on the GM client, the single authority for shared playback.
 */
export async function advanceSharedPlaylist(endedAssetId: string): Promise<void> {
  const queue = usePlaylistPlayback.getState().queue;
  if (!queue || queue.tracks[queue.index]?.asset_id !== endedAssetId) return;
  const started = await startFrom(queue, queue.index + 1, 1);
  // A stop or manual skip during the await wins over this advance.
  const current = usePlaylistPlayback.getState().queue;
  if (current?.playlistId !== queue.playlistId || current.index !== queue.index) return;
  if (started === null) {
    usePlaylistPlayback.setState({ queue: null });
    return;
  }
  usePlaylistPlayback.setState({ queue: { ...queue, index: started } });
}
