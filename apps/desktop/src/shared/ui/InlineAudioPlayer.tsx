import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AudioButton, audioTime } from './AudioControls';
import { Icon } from './Icon';

/** This player controls only its own element, never the shared session soundtrack. */
export function InlineAudioPlayer({ src, title }: { src: string; title: string }) {
  const { t } = useTranslation();
  const audio = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [blocked, setBlocked] = useState(false);
  return (
    <span className="audio-card journal-audio" role="group" aria-label={t('media.player')}>
      <audio
        ref={audio}
        aria-label={title}
        src={src}
        preload="metadata"
        onPlay={() => {
          setPlaying(true);
          setBlocked(false);
        }}
        onPause={() => {
          setPlaying(false);
        }}
        onEnded={() => {
          setPlaying(false);
        }}
        onTimeUpdate={(e) => {
          setPosition(e.currentTarget.currentTime);
        }}
        onLoadedMetadata={(e) => {
          const d = e.currentTarget.duration;
          setDuration(Number.isFinite(d) ? d : 0);
        }}
      />
      <span className="audio-heading">
        <span className="audio-art">
          <Icon name="music" size={22} />
        </span>
        <strong title={title}>{title}</strong>
      </span>
      <span className="audio-transport">
        <AudioButton
          className="audio-control audio-primary"
          paused={!playing}
          label={
            playing
              ? t('media.localPause', { defaultValue: 'Pause' })
              : t('media.localPlay', { defaultValue: 'Play' })
          }
          onClick={() => {
            const element = audio.current;
            if (!element) return;
            if (playing) element.pause();
            else
              void element.play().catch(() => {
                setBlocked(true);
              });
          }}
        />
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
      {blocked && <span role="status">{t('media.enableAudio')}</span>}
    </span>
  );
}
