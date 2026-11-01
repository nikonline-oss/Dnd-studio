import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import {
  advanceSharedPlaylist,
  playSharedPlaylist,
  skipSharedPlaylist,
  stopSharedPlaylist,
  usePlaylistPlayback,
} from '@/shared/game/playlistPlayback';
import { onSoundEnded } from '@/shared/game/soundPlayback';
import type { PlaylistDto, PlaylistTrackDto } from '@/shared/ipc/client';
import { AudioButton } from '@/shared/ui/AudioControls';
import { ElementPicker } from '@/shared/ui/ElementPicker';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { PlaylistPlayer } from '@/shared/ui/PlaylistPlayer';

/**
 * A playlist opened in the library: personal sequential playback for everyone, plus the GM
 * transport broadcasting it to the session and the track list editing (order, contents).
 */
export function PlaylistView({ playlist }: { playlist: PlaylistDto }) {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const submit = useSubmitIntent();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const queue = usePlaylistPlayback((s) => s.queue);
  const sharedIndex = queue?.playlistId === playlist.id ? queue.index : null;
  const [sharedError, setSharedError] = useState<unknown>(null);
  const [localStart, setLocalStart] = useState<number | null>(null);
  const [picking, setPicking] = useState(false);

  // The GM client is the single authority for shared playback: it advances the queue when the
  // current track ends naturally (players only listen, their players never send intents).
  useEffect(() => {
    if (!isGm) return;
    return onSoundEnded((assetId) => {
      void advanceSharedPlaylist(assetId).catch(() => undefined);
    });
  }, [isGm]);

  const runShared = (action: () => Promise<void>) => {
    setSharedError(null);
    void action().catch((error: unknown) => {
      setSharedError(error);
    });
  };
  const commit = (tracks: PlaylistTrackDto[]) => {
    submit.mutate({
      intent: { kind: 'update_playlist', id: playlist.id, name: playlist.name, tracks },
      expectedVersion: playlist.version,
    });
  };
  const move = (index: number, delta: -1 | 1) => {
    const target = index + delta;
    if (target < 0 || target >= playlist.tracks.length) return;
    const tracks = [...playlist.tracks];
    const [entry] = tracks.splice(index, 1);
    if (entry) tracks.splice(target, 0, entry);
    commit(tracks);
  };

  return (
    <section className="playlist-view" aria-label={playlist.name}>
      <header className="playlist-header">
        <h2>{playlist.name}</h2>
        <span className="muted">
          {t('playlists.tracksCount', { count: playlist.tracks.length })}
        </span>
      </header>
      <PlaylistPlayer
        key={localStart === null ? playlist.id : `${playlist.id}:${localStart}`}
        playlist={playlist}
        initialIndex={localStart}
        autoPlay={localStart !== null}
      />
      {isGm && (
        <div className="audio-transport playlist-shared">
          {sharedIndex === null ? (
            <AudioButton
              className="audio-control audio-primary"
              icon="play"
              label={t('playlists.playForAll')}
              disabled={playlist.tracks.length === 0}
              onClick={() => {
                runShared(() => playSharedPlaylist(playlist, 0));
              }}
            />
          ) : (
            <>
              <AudioButton
                className="audio-control"
                icon="prev"
                label={t('playlists.previous')}
                onClick={() => {
                  runShared(() => skipSharedPlaylist(-1));
                }}
              />
              <AudioButton
                className="audio-control"
                icon="stop"
                label={t('media.stop')}
                onClick={() => {
                  runShared(stopSharedPlaylist);
                }}
              />
              <AudioButton
                className="audio-control"
                icon="next"
                label={t('playlists.next')}
                onClick={() => {
                  runShared(() => skipSharedPlaylist(1));
                }}
              />
            </>
          )}
          <span className="muted">
            {sharedIndex === null
              ? t('playlists.sharedIdle')
              : t('playlists.sharedNow', {
                  title: queue?.tracks[sharedIndex]?.title ?? '',
                })}
          </span>
        </div>
      )}
      {playlist.tracks.length === 0 ? (
        <p className="muted">{t('playlists.empty')}</p>
      ) : (
        <ol className="playlist-tracks">
          {playlist.tracks.map((track, index) => (
            <li key={`${track.asset_id}:${index}`} data-shared={sharedIndex === index || undefined}>
              <button
                type="button"
                className="playlist-track-name"
                onClick={() => {
                  setLocalStart(index);
                }}
              >
                {track.title}
              </button>
              {isGm && (
                <span className="playlist-track-actions">
                  <button
                    type="button"
                    aria-label={t('playlists.playFromHere')}
                    title={t('playlists.playFromHere')}
                    onClick={() => {
                      runShared(() => playSharedPlaylist(playlist, index));
                    }}
                  >
                    <Icon name="play" />
                  </button>
                  <button
                    type="button"
                    aria-label={t('playlists.moveUp')}
                    title={t('playlists.moveUp')}
                    disabled={index === 0 || submit.isPending}
                    onClick={() => {
                      move(index, -1);
                    }}
                  >
                    <Icon name="arrowUp" />
                  </button>
                  <button
                    type="button"
                    aria-label={t('playlists.moveDown')}
                    title={t('playlists.moveDown')}
                    disabled={index + 1 >= playlist.tracks.length || submit.isPending}
                    onClick={() => {
                      move(index, 1);
                    }}
                  >
                    <Icon name="arrowDown" />
                  </button>
                  <button
                    type="button"
                    aria-label={t('playlists.removeTrack', { title: track.title })}
                    title={t('playlists.removeTrack', { title: track.title })}
                    disabled={submit.isPending}
                    onClick={() => {
                      commit(playlist.tracks.filter((_, i) => i !== index));
                    }}
                  >
                    <Icon name="trash" />
                  </button>
                </span>
              )}
            </li>
          ))}
        </ol>
      )}
      {isGm && (
        <button
          type="button"
          className="playlist-add"
          disabled={submit.isPending}
          onClick={() => {
            setPicking(true);
          }}
        >
          <Icon name="plus" /> {t('playlists.addTracks')}
        </button>
      )}
      {picking && (
        <ElementPicker
          title={t('playlists.addTracks')}
          kinds={['audio']}
          onClose={() => {
            setPicking(false);
          }}
          onPick={({ id }) => {
            const asset = assets.find((a) => a.id === id);
            if (!asset) return;
            commit([
              ...playlist.tracks,
              {
                asset_id: asset.id,
                sha256: asset.sha256,
                mime: asset.mime,
                title: asset.original_name ?? asset.sha256.slice(0, 12),
              },
            ]);
          }}
        />
      )}
      <ErrorText error={submit.error ?? sharedError} />
    </section>
  );
}
