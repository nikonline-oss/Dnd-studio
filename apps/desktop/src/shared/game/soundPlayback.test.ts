import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  stopSound,
  playSound,
  setSoundMuted,
  useSoundPlayback,
  syncLoopingSounds,
} from './soundPlayback';
import type { SoundDto } from '@/shared/ipc/client';

class FakeAudio {
  loop = false;
  volume = 1;
  muted = false;
  duration = 120;
  ontimeupdate: (() => void) | null = null;
  onloadedmetadata: (() => void) | null = null;
  currentTime = 0;
  onended: (() => void) | null = null;
  paused = false;
  readonly src: string;

  constructor(src: string) {
    this.src = src;
    instances.push(this);
  }

  play() {
    this.paused = false;
    return Promise.resolve();
  }

  pause() {
    this.paused = true;
  }
}

const instances: FakeAudio[] = [];
const sound: SoundDto = {
  asset_id: '0190a000-0000-7000-8000-000000000050',
  sha256: 'a'.repeat(64),
  mime: 'audio/mpeg',
  volume: 40,
  looped: true,
};

describe('shared sound playback', () => {
  beforeEach(() => {
    (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__ = {
      convertFileSrc: (sha: string) => `dnd-asset://${sha}`,
    };
  });
  afterEach(() => {
    stopSound(null);
    setSoundMuted(false);
    instances.length = 0;
    vi.unstubAllGlobals();
  });

  it('plays content-addressed audio at the broadcast volume and stops it', () => {
    vi.stubGlobal('Audio', FakeAudio);
    (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__ = {
      convertFileSrc: (sha: string) => `dnd-asset://${sha}`,
    };

    playSound(sound);
    expect(instances).toHaveLength(1);
    expect(instances[0]).toMatchObject({
      src: `dnd-asset://${sound.sha256}`,
      loop: true,
      volume: 0.4,
    });
    stopSound(sound.asset_id);
    expect(instances[0]?.paused).toBe(true);
  });

  it('mutes this listener without pausing or losing the shared playback', () => {
    vi.stubGlobal('Audio', FakeAudio);
    playSound(sound);
    setSoundMuted(true);
    expect(instances[0]).toMatchObject({ muted: true, paused: false });
    expect(useSoundPlayback.getState().tracks).toHaveLength(1);
    playSound({ ...sound, asset_id: 'second' });
    expect(instances[1]?.muted).toBe(true);
    setSoundMuted(false);
    expect(instances.every((audio) => !audio.muted)).toBe(true);
  });

  it('applies pause and seek and preserves position on an identical snapshot', () => {
    vi.stubGlobal('Audio', FakeAudio);
    playSound({ ...sound, position_ms: 30000, paused: true });
    expect(instances[0]).toMatchObject({ currentTime: 30, paused: true });
    playSound({ ...sound, position_ms: 30000, paused: false });
    expect(instances).toHaveLength(1);
    const audio = instances[0];
    if (!audio) throw new Error('missing audio');
    audio.currentTime = 35;
    audio.ontimeupdate?.();
    syncLoopingSounds([{ ...sound, position_ms: 30000, paused: false }]);
    expect(audio.currentTime).toBe(35);
    expect(useSoundPlayback.getState().tracks[0]?.position).toBe(35);
  });

  it('keeps the audio permission prompt through metadata and time updates', async () => {
    vi.stubGlobal('Audio', FakeAudio);
    const play = vi.spyOn(FakeAudio.prototype, 'play').mockRejectedValue(new Error('blocked'));
    playSound(sound);
    await Promise.resolve();
    await Promise.resolve();
    expect(useSoundPlayback.getState().tracks[0]?.blocked).toBe(true);
    instances[0]?.onloadedmetadata?.();
    instances[0]?.ontimeupdate?.();
    expect(useSoundPlayback.getState().tracks[0]?.blocked).toBe(true);
    play.mockRestore();
  });
});
