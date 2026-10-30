import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { resumeBlockedSound, setSoundMuted, useSoundPlayback } from '@/shared/game/soundPlayback';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from './ErrorText';

function time(seconds: number) {
  const safe = Math.max(0, Math.floor(seconds));
  return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, '0')}`;
}

/** Shared playback controls issue intents; listeners can only mute their own output. */
export function SessionAudioPlayer({
  assetId,
  compact = false,
}: {
  assetId: string;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const asset = useSnapshotSelect((s) => s.assets.find((a) => a.id === assetId));
  const track = useSoundPlayback((s) => s.tracks.find((p) => p.sound.asset_id === assetId));
  const muted = useSoundPlayback((s) => s.muted);
  const submit = useSubmitIntent();
  const [seek, setSeek] = useState<number | null>(null);
  const [volume, setVolume] = useState<number | null>(null);
  const paused = track ? (track.sound.paused ?? false) : true;
  const position = track?.position ?? 0;
  const change = (patch: {
    paused?: boolean;
    position_ms?: number;
    volume?: number;
    looped?: boolean;
  }) => {
    submit.mutate({
      intent: {
        kind: 'set_sound_playback',
        asset_id: assetId,
        paused,
        position_ms: Math.min(86_400_000, Math.round(position * 1000)),
        volume: track?.sound.volume ?? 80,
        looped: track?.sound.looped ?? true,
        ...patch,
      },
    });
  };
  return (
    <section className={`session-audio ${compact ? 'compact' : ''}`} aria-label={t('media.player')}>
      <div className="session-audio-title">
        <span aria-hidden="true">♫</span>
        <strong>{isGm ? (asset?.original_name ?? t('media.playing')) : t('media.playing')}</strong>
        {isGm && compact && asset && (
          <button
            onClick={() => {
              useSelection.getState().openMedia('audio', asset.folder_id ?? null, asset.id);
            }}
          >
            {t('media.openPlayer')}
          </button>
        )}
      </div>
      <div className="session-audio-buttons">
        {isGm && (
          <>
            <button
              disabled={submit.isPending}
              onClick={() => {
                change({ paused: !paused });
              }}
            >
              {paused ? t('media.play') : t('media.pause')}
            </button>
            <button
              disabled={!track || submit.isPending}
              onClick={() => {
                submit.mutate({ intent: { kind: 'stop_sound', asset_id: assetId } });
              }}
            >
              {t('media.stop')}
            </button>
          </>
        )}
        <button
          aria-pressed={muted}
          onClick={() => {
            setSoundMuted(!muted);
          }}
        >
          {muted ? t('media.unmuteSelf') : t('media.muteSelf')}
        </button>
        {track?.blocked && (
          <button
            onClick={() => {
              resumeBlockedSound(assetId);
            }}
          >
            {t('media.enableAudio')}
          </button>
        )}
      </div>
      {isGm && !compact && (
        <>
          <label className="media-range">
            {t('media.position')}
            <input
              type="range"
              min="0"
              max={Math.max(track?.duration ?? 0, position, 1)}
              step="0.1"
              value={seek ?? position}
              disabled={!track || !track.duration}
              onChange={(e) => {
                setSeek(Number(e.target.value));
              }}
              onPointerUp={(e) => {
                change({ position_ms: Math.round(Number(e.currentTarget.value) * 1000) });
                setSeek(null);
              }}
              onKeyUp={(e) => {
                if (
                  ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)
                ) {
                  change({ position_ms: Math.round(Number(e.currentTarget.value) * 1000) });
                  setSeek(null);
                }
              }}
            />
            <output>
              {time(seek ?? position)} / {time(track?.duration ?? 0)}
            </output>
          </label>
          <label className="media-range">
            {t('media.volume')}
            <input
              type="range"
              min="0"
              max="100"
              value={volume ?? track?.sound.volume ?? 80}
              onChange={(e) => {
                setVolume(Number(e.target.value));
              }}
              onPointerUp={(e) => {
                change({ volume: Number(e.currentTarget.value) });
                setVolume(null);
              }}
              onKeyUp={(e) => {
                if (
                  ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)
                ) {
                  change({ volume: Number(e.currentTarget.value) });
                  setVolume(null);
                }
              }}
            />
          </label>
          <label>
            <input
              type="checkbox"
              checked={track?.sound.looped ?? true}
              onChange={(e) => {
                change({ looped: e.target.checked });
              }}
            />
            {t('media.repeat')}
          </label>
        </>
      )}
      <ErrorText error={submit.error} />
    </section>
  );
}

export function CompactAudioPlayers() {
  const tracks = useSoundPlayback((s) => s.tracks);
  const active = useSelection((s) => s.active);
  const selections = useSelection((s) => s.mediaSelection);
  const selected = active?.kind === 'audio' ? selections[`audio:${active.id}`] : null;
  const fullPlayerVisible =
    useSnapshotSelect((s) =>
      s.assets.some(
        (a) => a.id === selected && a.usage === 'audio' && (a.folder_id ?? 'root') === active?.id,
      ),
    ) ?? false;
  const visible = tracks.filter((track) => !fullPlayerVisible || track.sound.asset_id !== selected);
  if (visible.length === 0) return null;
  return (
    <div className="compact-audio-players">
      {visible.map((track) => (
        <SessionAudioPlayer key={track.sound.asset_id} assetId={track.sound.asset_id} compact />
      ))}
    </div>
  );
}
