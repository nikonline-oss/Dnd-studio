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
  const active = useSelection((s) => s.active);
  const submit = useSubmitIntent();
  const remote = useRemote();
  const showToast = useToast((s) => s.show);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const importImages = useMutation({ mutationFn: ipc.assetImportImages });

  const q = query.trim().toLowerCase();
  const shown = assets
    .filter((a) => !q || (a.original_name ?? '').toLowerCase().includes(q))
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const picture = assets.find((a) => a.id === selected);
  const boardId = active?.kind === 'board' ? active.id : null;

  return (
    <NavSection
      icon="image"
      title={t('images.title')}
      count={assets.length}
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
      <input
        type="search"
        className="image-search"
        aria-label={t('picker.search')}
        placeholder={t('picker.search')}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
        }}
      />
      {assets.length === 0 && <p className="nav-empty">{t('images.empty')}</p>}
      <ul className="image-grid" role="listbox" aria-label={t('images.title')}>
        {shown.map((asset) => (
          <li
            key={asset.id}
            role="option"
            aria-selected={asset.id === selected}
            title={asset.original_name ?? undefined}
            onClick={() => {
              setSelected(asset.id);
            }}
          >
            <img src={assetUrl(asset.thumbnail_sha256)} alt={asset.original_name ?? ''} />
            <span>{asset.original_name ?? asset.sha256.slice(0, 8)}</span>
          </li>
        ))}
      </ul>
      {picture && (
        <div className="image-actions">
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
