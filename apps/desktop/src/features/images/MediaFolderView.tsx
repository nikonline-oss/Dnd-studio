import { useTranslation } from 'react-i18next';
import { useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type Library } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { SessionAudioPlayer } from '@/shared/ui/SessionAudioPlayer';

/** A folder's files stay alongside the selected image or session music player. */
export function MediaFolderView({
  library,
  folderId,
}: {
  library: Library;
  folderId: string | null;
}) {
  const { t } = useTranslation();
  const assets = useSnapshotSelect((s) => s.assets) ?? [];
  const folders = useSnapshotSelect((s) => s.library_folders ?? []) ?? [];
  const openMedia = useSelection((s) => s.openMedia);
  const key = `${library}:${folderId ?? 'root'}`;
  const selected = useSelection((s) => s.mediaSelection[key]);
  const files = assets
    .filter(
      (a) =>
        (library === 'audio' ? a.usage === 'audio' : a.usage !== 'audio') &&
        (a.folder_id ?? null) === folderId,
    )
    .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? ''));
  const children = folders.filter((f) => f.library === library && f.parent === folderId);
  const folder = folders.find((f) => f.id === folderId);
  const asset = files.find((a) => a.id === selected);
  return (
    <section className="media-folder-view" aria-label={folder?.name ?? t(`${library}.title`)}>
      <aside className="media-folder-files">
        <header>
          <strong>{folder?.name ?? t(`${library}.title`)}</strong>
          {folderId && (
            <button
              onClick={() => {
                openMedia(library, folder?.parent ?? null);
              }}
            >
              {t('media.up')}
            </button>
          )}
        </header>
        <div role="listbox" aria-label={t('media.files')}>
          {children.map((child) => (
            <button
              key={child.id}
              role="option"
              aria-selected={false}
              onClick={() => {
                openMedia(library, child.id);
              }}
            >
              ▸ {child.name}
            </button>
          ))}
          {files.map((file) => (
            <button
              key={file.id}
              role="option"
              aria-selected={asset?.id === file.id}
              onClick={() => {
                openMedia(library, folderId, file.id);
              }}
            >
              {library === 'images' && <img src={assetUrl(file.thumbnail_sha256)} alt="" />}
              <span>{file.original_name ?? file.sha256.slice(0, 12)}</span>
            </button>
          ))}
        </div>
        {files.length === 0 && children.length === 0 && <p className="muted">{t('media.empty')}</p>}
      </aside>
      <div className="media-folder-preview">
        {!asset ? (
          <p className="muted">{t('media.selectFile')}</p>
        ) : library === 'audio' ? (
          <SessionAudioPlayer key={asset.id} assetId={asset.id} />
        ) : (
          <>
            <h2>{asset.original_name}</h2>
            {asset.usage === 'map' && !asset.display_sha256 && asset.tiles.length > 0 ? (
              <div
                role="img"
                aria-label={asset.original_name ?? ''}
                className="media-tiled-image"
                style={{ aspectRatio: `${asset.width} / ${asset.height}` }}
              >
                {asset.tiles.map((tile) => (
                  <img
                    key={`${tile.x}:${tile.y}`}
                    alt=""
                    src={assetUrl(tile.sha256)}
                    style={{
                      left: `${(tile.x / asset.width) * 100}%`,
                      top: `${(tile.y / asset.height) * 100}%`,
                      width: `${(tile.width / asset.width) * 100}%`,
                      height: `${(tile.height / asset.height) * 100}%`,
                    }}
                  />
                ))}
              </div>
            ) : (
              <img
                className="media-full-image"
                src={assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256)}
                alt={asset.original_name ?? ''}
              />
            )}
          </>
        )}
      </div>
    </section>
  );
}
