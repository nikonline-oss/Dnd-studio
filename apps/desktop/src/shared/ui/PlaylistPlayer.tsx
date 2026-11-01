import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AudioButton, audioTime } from './AudioControls';
import { Icon } from './Icon';
import { assetUrl, type PlaylistDto } from '@/shared/ipc/client';

/**
 * Personal sequential playback of a playlist: one local `<audio>` element, advancing to the next
 * track when the current one ends. Never touches the shared session soundtrack — the GM transport
 * for that lives in the playlist view.
 */
export function PlaylistPlayer({
  playlist,
  compact = false,
  initialIndex = null,
  autoPlay = false,
}: {
  playlist: PlaylistDto;
  compact?: boolean;
  /** Track to start on (with `autoPlay`): the playlist view remounts the player to change it. */
  initialIndex?: number | null;
  autoPlay?: boolean;
}) {
  const { t } = useTranslation();
  const audio = useRef<HTMLAudioElement>(null);
  const [index, setIndex] = useState<number | null>(initialIndex);
  const [playing, setPlaying] = useState(autoPlay);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [blocked, setBlocked] = useState(false);
  const track = index === null ? undefined : playlist.tracks[index];

  useEffect(() => {
    const element = audio.current;
    if (!element) return;
    if (playing)
      void element.play().catch(() => {
        setBlocked(true);
        setPlaying(false);
      });
    else element.pause();
  }, [playing, index, track?.sha256]);

  const playAt = (next: number) => {
    if (next < 0 || next >= playlist.tracks.length) return;
    setIndex(next);
    setPosition(0);
    setPlaying(true);
    setBlocked(false);
  };
  const toggle = () => {
    if (playing) setPlaying(false);
    else if (index === null) playAt(0);
    else setPlaying(true);
  };

  return (
    <span
      className={`audio-card playlist-player ${compact ? 'compact' : ''}`}
      role="group"
      aria-label={playlist.name}
    >
      {track && (
        <audio
          key={track.sha256}
          ref={audio}
          aria-label={track.title}
          src={assetUrl(track.sha256)}
          preload="metadata"
          onPlay={() => {
            setPlaying(true);
            setBlocked(false);
          }}
          onPause={() => {
            setPlaying(false);
          }}
          onEnded={() => {
            if (index !== null && index + 1 < playlist.tracks.length) playAt(index + 1);
            else setPlaying(false);
          }}
          onTimeUpdate={(e) => {
            setPosition(e.currentTarget.currentTime);
          }}
          onLoadedMetadata={(e) => {
            const d = e.currentTarget.duration;
            setDuration(Number.isFinite(d) ? d : 0);
          }}
        />
      )}
      <span className="audio-heading">
        <span className="audio-art">
          <Icon name="playlist" size={22} />
        </span>
        <strong title={playlist.name}>{playlist.name}</strong>
      </span>
      <span className="audio-transport">
        <AudioButton
          className="audio-control"
          icon="prev"
          label={t('playlists.previous')}
          disabled={playlist.tracks.length === 0 || (index ?? 0) <= 0}
          onClick={() => {
            playAt((index ?? 0) - 1);
          }}
        />
        <AudioButton
          className="audio-control audio-primary"
          paused={!playing}
          label={
            playing
              ? t('media.localPause', { defaultValue: 'Pause' })
              : t('media.localPlay', { defaultValue: 'Play' })
          }
          disabled={playlist.tracks.length === 0}
          onClick={toggle}
        />
        <AudioButton
          className="audio-control"
          icon="next"
          label={t('playlists.next')}
          disabled={
            playlist.tracks.length === 0 || (index !== null && index + 1 >= playlist.tracks.length)
          }
          onClick={() => {
            playAt((index ?? -1) + 1);
          }}
        />
        <span className="playlist-current muted">
          {track ? track.title : t('playlists.tracksCount', { count: playlist.tracks.length })}
        </span>
      </span>
      {!compact && (
        <>
          <span className="audio-timeline">
            <input
              type="range"
              aria-label={t('media.position')}
              min="0"
              max={duration || 1}
              step="0.1"
              disabled={!duration}
              value={position}
              onChange={(e) => {
                const value = Number(e.target.value);
                if (audio.current) audio.current.currentTime = value;
                setPosition(value);
              }}
            />
            <output>
              {audioTime(position)} / {audioTime(duration)}
            </output>
          </span>
          <span className="audio-volume">
            <Icon name={volume ? 'volume' : 'volumeOff'} />
            <input
              type="range"
              aria-label={t('media.localVolume', { defaultValue: 'Volume' })}
              min="0"
              max="1"
              step="0.01"
              value={volume}
              onChange={(e) => {
                const value = Number(e.target.value);
                setVolume(value);
                if (audio.current) audio.current.volume = value;
              }}
            />
          </span>
        </>
      )}
      {blocked && <span role="status">{t('media.enableAudio')}</span>}
    </span>
  );
}
