import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ipc, type AssetDto } from '@/shared/ipc/client';
import { LibraryFolderNav } from './LibraryFolderNav';
import { useRemote } from '@/shared/game/useRemote';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { ErrorText } from '@/shared/ui/ErrorText';

/** Music and sound library. GM playback is broadcast through the authoritative game event stream. */
export function AudioLibrary() {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const sounds = assets
    .filter((asset) => asset.usage === 'audio')
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const [folder, setFolder] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const remote = useRemote();
  const submit = useSubmitIntent();
  const importing = useMutation({ mutationFn: ipc.assetImportAudio });
  const current = useSnapshotSelect((s) => s.sounds ?? []) ?? [];
  const folders = (useSnapshotSelect((s) => s.library_folders ?? []) ?? []).filter(
    (f) => f.library === 'audio',
  );
  const shown = sounds.filter((asset) => (asset.folder_id ?? null) === folder);

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
      <LibraryFolderNav library="audio" selected={folder} onSelect={setFolder} />
      {shown.length === 0 && <p className="nav-empty">{t('audio.empty')}</p>}
      <ul className="audio-list" aria-label={t('audio.title')}>
        {shown.map((asset) => (
          <AudioRow
            key={asset.id}
            asset={asset}
            selected={asset.id === selected}
            playing={current.some((sound) => sound.asset_id === asset.id)}
            folders={folders}
            onSelect={() => {
              setSelected(asset.id);
            }}
            onMove={(folderId) => {
              submit.mutate({
                intent: { kind: 'move_asset', id: asset.id, folder_id: folderId },
                expectedVersion: asset.version,
              });
            }}
            onPlay={(looped) => {
              submit.mutate({
                intent: { kind: 'play_sound', asset_id: asset.id, volume: 80, looped },
              });
            }}
            onStop={() => {
              submit.mutate({ intent: { kind: 'stop_sound', asset_id: asset.id } });
            }}
          />
        ))}
      </ul>
      <ErrorText error={importing.error ?? submit.error} />
    </NavSection>
  );
}

function AudioRow({
  asset,
  selected,
  playing,
  folders,
  onSelect,
  onMove,
  onPlay,
  onStop,
}: {
  asset: AssetDto;
  selected: boolean;
  playing: boolean;
  folders: { id: string; name: string; parent: string | null }[];
  onSelect: () => void;
  onMove: (folder: string | null) => void;
  onPlay: (looped: boolean) => void;
  onStop: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li className="audio-row" aria-selected={selected} onClick={onSelect}>
      <span className="audio-row-name" title={asset.original_name ?? undefined}>
        {asset.original_name ?? asset.sha256.slice(0, 8)}
      </span>
      <select
        aria-label={t('library.moveAsset', {
          name: asset.original_name ?? asset.sha256.slice(0, 8),
        })}
        value={asset.folder_id ?? ''}
        onChange={(event) => {
          onMove(event.target.value || null);
        }}
      >
        <option value="">{t('library.root')}</option>
        {folders.map((folder) => (
          <option key={folder.id} value={folder.id}>
            {folder.name}
          </option>
        ))}
      </select>
      <IconButton
        icon="play"
        label={t('audio.playOnce')}
        onClick={() => {
          onPlay(false);
        }}
      />
      <IconButton
        icon="music"
        label={t('audio.playLoop')}
        onClick={() => {
          onPlay(true);
        }}
      />
      {playing && <IconButton icon="stop" label={t('audio.stop')} onClick={onStop} />}
    </li>
  );
}
