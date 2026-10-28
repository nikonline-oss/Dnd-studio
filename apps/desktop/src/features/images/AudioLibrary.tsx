import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ipc } from '@/shared/ipc/client';
import { useRemote } from '@/shared/game/useRemote';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { LibraryExplorer } from './LibraryExplorer';

/** Music and sound library. GM playback is broadcast through the authoritative game event stream. */
export function AudioLibrary() {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const sounds = assets
    .filter((asset) => asset.usage === 'audio')
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const [selected, setSelected] = useState<string | null>(null);
  const remote = useRemote();
  const submit = useSubmitIntent();
  const importing = useMutation({ mutationFn: ipc.assetImportAudio });
  const current = useSnapshotSelect((s) => s.sounds ?? []) ?? [];
  const selectedSound = sounds.find((asset) => asset.id === selected);

  return (
    <NavSection
      icon="music"
      title={t('audio.title')}
      count={sounds.length}
      actions={
        remote === null && (
          <IconButton
            icon="plus"
            label={t('audio.import')}
            disabled={importing.isPending}
            onClick={() => {
              importing.mutate();
            }}
          />
        )
      }
    >
      <LibraryExplorer library="audio" assets={sounds} selected={selected} onSelect={setSelected} />
      {selectedSound && (
        <div
          className="audio-selection-actions"
          aria-label={selectedSound.original_name ?? t('audio.title')}
        >
          <span className="row-name">
            {selectedSound.original_name ?? selectedSound.sha256.slice(0, 8)}
          </span>
          <IconButton
            icon="play"
            label={t('audio.playOnce')}
            onClick={() => {
              submit.mutate({
                intent: {
                  kind: 'play_sound',
                  asset_id: selectedSound.id,
                  volume: 80,
                  looped: false,
                },
              });
            }}
          />
          <IconButton
            icon="music"
            label={t('audio.playLoop')}
            onClick={() => {
              submit.mutate({
                intent: {
                  kind: 'play_sound',
                  asset_id: selectedSound.id,
                  volume: 80,
                  looped: true,
                },
              });
            }}
          />
          {current.some((sound) => sound.asset_id === selectedSound.id) && (
            <IconButton
              icon="stop"
              label={t('audio.stop')}
              onClick={() => {
                submit.mutate({ intent: { kind: 'stop_sound', asset_id: selectedSound.id } });
              }}
            />
          )}
        </div>
      )}
      <ErrorText error={importing.error ?? submit.error} />
    </NavSection>
  );
}
