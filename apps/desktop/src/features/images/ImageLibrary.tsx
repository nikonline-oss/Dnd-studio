import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { useRemote } from '@/shared/game/useRemote';
import { assetUrl, ipc, type AssetDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { IconButton } from '@/shared/ui/IconButton';
import { NavSection } from '@/shared/ui/NavSection';
import { useToast } from '@/shared/ui/Toast';
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
  const showToast = useToast((s) => s.show);
  const [selected, setSelected] = useState<string | null>(null);
  const importImages = useMutation({ mutationFn: ipc.assetImportImages });

  const picture = assets.find((a) => a.id === selected);
  const boardId = active?.kind === 'board' ? active.id : null;

  return (
    <NavSection
      icon="image"
      title={t('images.title')}
      count={imageAssets.length}
      actions={
        // Pictures are imported into the local campaign; uploading to a session comes later.
        remote === null && (
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
      />
      {picture && (
        <div className="image-actions">
          <img
            className="image-selection-preview"
            src={assetUrl(picture.thumbnail_sha256)}
            alt={picture.original_name ?? ''}
          />
          <select
            aria-label={t('library.moveAsset', {
              name: picture.original_name ?? picture.sha256.slice(0, 8),
            })}
            value={picture.folder_id ?? ''}
            onChange={(event) => {
              submit.mutate({
                intent: {
                  kind: 'move_asset',
                  id: picture.id,
                  folder_id: event.target.value || null,
                },
                expectedVersion: picture.version,
              });
            }}
          >
            <option value="">{t('library.root')}</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
          <button
            type="button"
            disabled={boardId === null || submit.isPending}
            title={boardId === null ? t('images.openBoardFirst') : undefined}
            onClick={() => {
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
            }}
          >
            {t('images.toBoard')}
          </button>
          <button
            type="button"
            onClick={() => {
              void navigator.clipboard.writeText(pictureMarkdown(picture)).then(() => {
                showToast(t('images.copied'));
              });
            }}
          >
            {t('images.copyForJournal')}
          </button>
        </div>
      )}
      <ErrorText error={importImages.error ?? submit.error} />
    </NavSection>
  );
}
