import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { InlineAudioPlayer } from './InlineAudioPlayer';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('journal audio player', () => {
  it('keeps play available after a browser playback rejection', async () => {
    vi.spyOn(HTMLMediaElement.prototype, 'play').mockRejectedValue(new Error('blocked'));
    render(<InlineAudioPlayer src="asset://rain" title="Rain" />);
    fireEvent.click(screen.getByRole('button', { name: 'Play' }));
    expect(await screen.findByText('Enable audio')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Play' })).toBeEnabled();
  });
  it('plays, pauses and seeks its own audio element', async () => {
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue();
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
    const { container } = render(<InlineAudioPlayer src="asset://rain" title="Rain" />);
    const audio = container.querySelector('audio');
    if (!audio) throw new Error('Audio element missing');
    Object.defineProperty(audio, 'duration', { value: 120 });
    fireEvent.loadedMetadata(audio);
    fireEvent.click(screen.getByRole('button', { name: 'Play' }));
    await waitFor(() => {
      expect(play).toHaveBeenCalled();
    });
    fireEvent.play(audio);
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    expect(pause).toHaveBeenCalled();
    fireEvent.change(screen.getByRole('slider', { name: 'Playback position' }), {
      target: { value: '45' },
    });
    expect(audio.currentTime).toBe(45);
    fireEvent.timeUpdate(audio);
    expect(screen.getByText('0:45 / 2:00')).toBeInTheDocument();
    fireEvent.change(screen.getByRole('slider', { name: 'Volume' }), { target: { value: '0.35' } });
    expect(audio.volume).toBe(0.35);
    fireEvent.ended(audio);
    expect(screen.getByRole('button', { name: 'Play' })).toBeInTheDocument();
  });
});
