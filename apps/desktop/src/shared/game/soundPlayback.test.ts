import { afterEach, describe, expect, it, vi } from 'vitest';
import { stopSound, playSound } from './soundPlayback';
import type { SoundDto } from '@/shared/ipc/client';

class FakeAudio {
  loop = false;
  volume = 1;
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
  afterEach(() => {
    stopSound(null);
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
});
