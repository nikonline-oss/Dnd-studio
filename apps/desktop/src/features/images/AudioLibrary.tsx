import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { ipc } from '@/shared/ipc/client';
import { useRemote } from '@/shared/game/useRemote';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { ErrorText } from '@/shared/ui/ErrorText';
import type { MenuItem } from '@/shared/ui/Menu';
import { LibraryExplorer } from './LibraryExplorer';
import { useSelection } from '@/shared/state/selection';

/** Music and sound library. GM playback is broadcast through the authoritative game event stream. */
export function AudioLibrary() {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const sounds = assets
    .filter((asset) => asset.usage === 'audio')
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const [selected, setSelected] = useState<string | null>(null);
  const remote = useRemote();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canImport =
    isGm &&
    (remote === null ||
      remote.status.state === 'online' ||
      (remote.hosted && remote.status.state === 'reconnecting'));
  const submit = useSubmitIntent();
  const importing = useMutation({ mutationFn: ipc.assetImportAudio });
  const current = useSnapshotSelect((s) => s.sounds ?? []) ?? [];
  const soundMenu = (asset: (typeof sounds)[number]): MenuItem[] => [
    {
      label: t('audio.playOnce'),
      onSelect: () => {
        submit.mutate({
          intent: { kind: 'play_sound', asset_id: asset.id, volume: 80, looped: false },
        });
      },
    },
    {
      label: t('audio.playLoop'),
      onSelect: () => {
        submit.mutate({
          intent: { kind: 'play_sound', asset_id: asset.id, volume: 80, looped: true },
        });
      },
    },
    ...(current.some((sound) => sound.asset_id === asset.id)
      ? [
          {
            label: t('audio.stop'),
            onSelect: () => {
              submit.mutate({ intent: { kind: 'stop_sound', asset_id: asset.id } });
            },
          },
        ]
      : []),
  ];

  return (
    <NavSection
      icon="music"
      title={t('audio.title')}
      count={sounds.length}
      actions={
        canImport && (
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
      <LibraryExplorer
        library="audio"
        assets={sounds}
        selected={selected}
        onSelect={(id) => {
          setSelected(id);
          const asset = sounds.find((a) => a.id === id);
          if (asset) useSelection.getState().openMedia('audio', asset.folder_id ?? null, asset.id);
        }}
        onOpenFolder={(id) => {
          useSelection.getState().openMedia('audio', id);
        }}
        assetMenuItems={soundMenu}
      />
      <ErrorText error={importing.error ?? submit.error} />
    </NavSection>
  );
}
