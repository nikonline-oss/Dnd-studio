import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { ipc, type AssetDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { useToast } from '@/shared/ui/Toast';
import type { MenuItem } from '@/shared/ui/Menu';
import { LibraryExplorer } from './LibraryExplorer';

/** How a picture is written into a journal text (ADR-0024). */
export function pictureMarkdown(asset: AssetDto): string {
  const alt = (asset.original_name ?? '').replace(/\.[a-z0-9]+$/i, '').replace(/[[\]]/g, '');
  return `![${alt}](asset:${asset.id})`;
}

/**
 * The campaign's image library (GM roles): every imported picture and map image, searchable;
 * new pictures are imported here. A picture goes onto the open board or into a journal text.
 */
export function ImageLibrary() {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const imageAssets = assets.filter((a) => a.usage !== 'audio');
  const folders = (useSnapshotSelect((s) => s.library_folders) ?? []).filter(
    (f) => f.library === 'images',
  );
  const active = useSelection((s) => s.active);
  const submit = useSubmitIntent();
  const remote = useRemote();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const canImport =
    isGm &&
    (remote === null ||
      remote.status.state === 'online' ||
      (remote.hosted && remote.status.state === 'reconnecting'));
  const showToast = useToast((s) => s.show);
  const [selected, setSelected] = useState<string | null>(null);
  const importImages = useMutation({ mutationFn: ipc.assetImportImages });

  const boardId = active?.kind === 'board' ? active.id : null;
  const folderPath = (folderId: string): string => {
    const folder = folders.find((item) => item.id === folderId);
    if (!folder) return '';
    return folder.parent ? `${folderPath(folder.parent)} / ${folder.name}` : folder.name;
  };
  const moveToFolder = (picture: AssetDto, folderId: string | null) => {
    submit.mutate({
      intent: { kind: 'move_asset', id: picture.id, folder_id: folderId },
      expectedVersion: picture.version,
    });
    setSelected(null);
  };
  const pictureMenu = (picture: AssetDto): (MenuItem | 'separator')[] => [
    {
      label: t('images.toBoard'),
      disabled: boardId === null || submit.isPending,
      onSelect: () => {
        if (boardId === null) return;
        submit.mutate({
          intent: {
            kind: 'create_board_node',
            id: uuidv7(),
            board_id: boardId,
            content: { type: 'image', asset_id: picture.id },
            x: 0,
            y: 0,
            width: 240 * 16,
            height: 180 * 16,
            visibility: 'gm',
          },
        });
      },
    },
    {
      label: t('images.copyForJournal'),
      onSelect: () => {
        void navigator.clipboard.writeText(pictureMarkdown(picture)).then(() => {
          showToast(t('images.copied'));
        });
      },
    },
    'separator',
    {
      label: t('library.moveAssetRoot'),
      onSelect: () => {
        moveToFolder(picture, null);
      },
    },
    ...folders.map((folder) => ({
      label: t('images.moveToFolder', { name: folderPath(folder.id) }),
      checked: picture.folder_id === folder.id,
      onSelect: () => {
        moveToFolder(picture, folder.id);
      },
    })),
  ];

  return (
    <NavSection
      icon="image"
      title={t('images.title')}
      count={imageAssets.length}
      actions={
        canImport && (
          <IconButton
            icon="plus"
            label={t('images.import')}
            disabled={importImages.isPending}
            onClick={() => {
              importImages.mutate();
            }}
          />
        )
      }
    >
      <LibraryExplorer
        library="images"
        assets={imageAssets}
        selected={selected}
        onSelect={setSelected}
        assetMenuItems={pictureMenu}
      />
      <ErrorText error={importImages.error ?? submit.error} />
    </NavSection>
  );
}
