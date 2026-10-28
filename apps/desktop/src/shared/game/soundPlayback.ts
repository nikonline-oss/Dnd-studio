import type { SoundDto } from '@/shared/ipc/client';
import { assetUrl } from '@/shared/ipc/client';

const active = new Map<string, HTMLAudioElement>();

/** Plays a committed server event on this client. One-shot sounds are not persisted in snapshots. */
export function playSound(sound: SoundDto): void {
  stopSound(sound.asset_id);
  const player = new Audio(assetUrl(sound.sha256));
  player.loop = sound.looped;
  player.volume = Math.max(0, Math.min(1, sound.volume / 100));
  active.set(sound.asset_id, player);
  player.onended = () => {
    if (active.get(sound.asset_id) === player) active.delete(sound.asset_id);
  };
  void player.play().catch(() => {
    if (active.get(sound.asset_id) === player) active.delete(sound.asset_id);
  });
}

export function stopSound(assetId: string | null): void {
  const targets =
    assetId === null ? [...active.entries()] : [[assetId, active.get(assetId)] as const];
  for (const [id, player] of targets) {
    if (!player) continue;
    player.pause();
    player.currentTime = 0;
    active.delete(id);
  }
}

/** Restores only looping music after a reconnect snapshot; transient one-shot effects stay silent. */
export function syncLoopingSounds(sounds: readonly SoundDto[]): void {
  const expected = new Set(sounds.map((sound) => sound.asset_id));
  for (const [id, player] of active) {
    if (player.loop && !expected.has(id)) stopSound(id);
  }
  for (const sound of sounds) {
    if (!active.has(sound.asset_id)) playSound(sound);
  }
}
