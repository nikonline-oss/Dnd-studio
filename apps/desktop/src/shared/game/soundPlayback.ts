import { create } from 'zustand';
import { assetUrl, type SoundDto } from '@/shared/ipc/client';

export interface PlayingTrack {
  sound: SoundDto;
  position: number;
  duration: number;
  blocked: boolean;
}
export const useSoundPlayback = create<{ muted: boolean; tracks: PlayingTrack[] }>()(() => ({
  muted: false,
  tracks: [],
}));
const active = new Map<string, { player: HTMLAudioElement; sound: SoundDto }>();

function publish(id: string, blocked?: boolean) {
  const entry = active.get(id);
  if (!entry) return;
  const track: PlayingTrack = {
    sound: entry.sound,
    position: entry.player.currentTime,
    duration: Number.isFinite(entry.player.duration) ? entry.player.duration : 0,
    blocked:
      blocked ??
      useSoundPlayback.getState().tracks.find((t) => t.sound.asset_id === id)?.blocked ??
      false,
  };
  useSoundPlayback.setState((state) => ({
    tracks: state.tracks.some((t) => t.sound.asset_id === id)
      ? state.tracks.map((t) => (t.sound.asset_id === id ? track : t))
      : [...state.tracks, track],
  }));
}

/** Personal mute never sends an intent or changes the session's playback position. */
export function setSoundMuted(muted: boolean): void {
  useSoundPlayback.setState({ muted });
  for (const { player } of active.values()) player.muted = muted;
}

export function resumeBlockedSound(id: string): void {
  const entry = active.get(id);
  if (!entry || entry.sound.paused) return;
  void entry.player
    .play()
    .then(() => {
      if (active.get(id) === entry) publish(id, false);
    })
    .catch(() => {
      if (active.get(id) === entry) publish(id, true);
    });
}

/** Apply committed playback commands to one audio element, retained across tab changes. */
export function playSound(sound: SoundDto): void {
  const id = sound.asset_id;
  const previous = active.get(id);
  if (previous && previous.sound.sha256 !== sound.sha256) stopSound(id);
  const player = active.get(id)?.player ?? new Audio(assetUrl(sound.sha256));
  active.set(id, { player, sound });
  player.loop = sound.looped;
  player.volume = Math.max(0, Math.min(1, sound.volume / 100));
  player.muted = useSoundPlayback.getState().muted;
  const seek = () => {
    if (active.get(id)?.sound !== sound) return;
    player.currentTime = (sound.position_ms ?? 0) / 1000;
    publish(id);
  };
  player.onloadedmetadata = seek;
  seek();
  player.ontimeupdate = () => {
    publish(id);
  };
  player.onended = () => {
    if (active.get(id)?.player === player) stopSound(id);
  };
  player.onerror = () => {
    publish(id, true);
  };
  if (sound.paused) {
    player.pause();
    publish(id, false);
  } else resumeBlockedSound(id);
}

export function stopSound(assetId: string | null): void {
  const ids = assetId === null ? [...active.keys()] : [assetId];
  for (const id of ids) {
    const entry = active.get(id);
    if (!entry) continue;
    active.delete(id);
    entry.player.onended = null;
    entry.player.ontimeupdate = null;
    entry.player.onloadedmetadata = null;
    entry.player.onerror = null;
    entry.player.pause();
    entry.player.currentTime = 0;
  }
  useSoundPlayback.setState((state) => ({
    tracks: state.tracks.filter((t) => !ids.includes(t.sound.asset_id)),
  }));
}

/** Identical snapshots must not restart a track; changed controls still reconcile. */
export function syncLoopingSounds(sounds: readonly SoundDto[]): void {
  const expected = new Set(sounds.map((sound) => sound.asset_id));
  for (const [id, entry] of active) {
    if ((entry.sound.looped || entry.sound.paused) && !expected.has(id)) stopSound(id);
  }
  for (const sound of sounds) {
    const current = active.get(sound.asset_id)?.sound;
    if (
      !current ||
      current.sha256 !== sound.sha256 ||
      current.volume !== sound.volume ||
      current.looped !== sound.looped ||
      (current.paused ?? false) !== (sound.paused ?? false) ||
      (current.position_ms ?? 0) !== (sound.position_ms ?? 0)
    )
      playSound(sound);
  }
}
