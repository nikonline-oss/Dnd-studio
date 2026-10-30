import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSubmitIntent, useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type Library } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { ErrorText } from '@/shared/ui/ErrorText';
import type { MenuItem } from '@/shared/ui/Menu';
import { uuidv7 } from '@/shared/ipc/uuid';
import type { AssetDto, LibraryFolderDto, Intent } from '@/shared/ipc/client';
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
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const submit = useSubmitIntent();
  type Target = { file: AssetDto } | { folder: LibraryFolderDto } | null;
  type Edit = {
    kind: 'rename' | 'move' | 'create' | 'delete';
    target: Target;
    parent: string | null;
  };
  const [menu, setMenu] = useState<{ x: number; y: number; target: Target } | null>(null);
  const [edit, setEdit] = useState<Edit | null>(null);
  const [name, setName] = useState('');
  const [destination, setDestination] = useState('');
  const context = (event: MouseEvent, target: Target) => {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ x: event.clientX, y: event.clientY, target });
  };
  const begin = (kind: Edit['kind'], target: Target, parent: string | null = folderId) => {
    submit.reset();
    setName(
      target && 'file' in target ? (target.file.original_name ?? '') : (target?.folder.name ?? ''),
    );
    setDestination(
      target
        ? 'file' in target
          ? (target.file.folder_id ?? '')
          : (target.folder.parent ?? '')
        : (parent ?? ''),
    );
    setEdit({ kind, target, parent });
  };
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
  const descendants = (id: string) => {
    const ids = new Set([id]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const candidate of folders) {
        if (candidate.parent && ids.has(candidate.parent) && !ids.has(candidate.id)) {
          ids.add(candidate.id);
          changed = true;
        }
      }
    }
    return ids;
  };
  const target = menu?.target;
  const items: (MenuItem | 'separator')[] = [];
  if (target) {
    items.push({
      label: t('media.open', { defaultValue: 'Open' }),
      onSelect: () => {
        if ('file' in target) openMedia(library, target.file.folder_id ?? null, target.file.id);
        else openMedia(library, target.folder.id);
      },
    });
    if (isGm) {
      items.push(
        'separator',
        {
          label:
            'file' in target
              ? t('library.renameAsset')
              : t('library.renameFolder', { name: target.folder.name }),
          onSelect: () => {
            begin('rename', target);
          },
        },
        {
          label: t('media.moveTo', { defaultValue: 'Move to folder…' }),
          onSelect: () => {
            begin('move', target);
          },
        },
      );
      if ('folder' in target) {
        items.push(
          {
            label: t('media.newFolder', { defaultValue: 'New folder' }),
            onSelect: () => {
              begin('create', null, target.folder.id);
            },
          },
          {
            label: t('library.deleteFolder', { name: target.folder.name }),
            onSelect: () => {
              begin('delete', target);
            },
          },
        );
      }
    }
  } else {
    if (folderId)
      items.push({
        label: t('media.up'),
        onSelect: () => {
          openMedia(library, folder?.parent ?? null);
        },
      });
    if (isGm)
      items.push({
        label: t('media.newFolder', { defaultValue: 'New folder' }),
        onSelect: () => {
          begin('create', null);
        },
      });
    if (folder && isGm)
      items.push(
        {
          label: t('library.renameFolder', { name: folder.name }),
          onSelect: () => {
            begin('rename', { folder });
          },
        },
        {
          label: t('media.moveTo', { defaultValue: 'Move to folder…' }),
          onSelect: () => {
            begin('move', { folder });
          },
        },
        {
          label: t('library.deleteFolder', { name: folder.name }),
          onSelect: () => {
            begin('delete', { folder });
          },
        },
      );
  }
  const excluded =
    edit?.target && 'folder' in edit.target
      ? descendants(edit.target.folder.id)
      : new Set<string>();
  const save = () => {
    if (!edit || !isGm || submit.isPending) return;
    const current = edit.target;
    let intent: Intent;
    let version: number | undefined;
    if (edit.kind === 'create')
      intent = {
        kind: 'create_library_folder',
        id: uuidv7(),
        library,
        parent: edit.parent,
        name: name.trim(),
      };
    else if (!current) return;
    else {
      version = 'file' in current ? current.file.version : current.folder.version;
      if (edit.kind === 'delete' && 'folder' in current)
        intent = { kind: 'delete_library_folder', id: current.folder.id };
      else if (edit.kind === 'rename')
        intent =
          'file' in current
            ? { kind: 'rename_asset', id: current.file.id, name: name.trim() }
            : { kind: 'rename_library_folder', id: current.folder.id, name: name.trim() };
      else if (edit.kind === 'move')
        intent =
          'file' in current
            ? { kind: 'move_asset', id: current.file.id, folder_id: destination || null }
            : { kind: 'move_library_folder', id: current.folder.id, parent: destination || null };
      else return;
    }
    submit.mutate(
      { intent, ...(version === undefined ? {} : { expectedVersion: version }) },
      {
        onSuccess: () => {
          setEdit(null);
        },
      },
    );
  };
  const folderPath = (id: string) => {
    const parts: string[] = [];
    const seen = new Set<string>();
    let cursor: string | null = id;
    while (cursor && !seen.has(cursor)) {
      seen.add(cursor);
      const f = folders.find((item) => item.id === cursor);
      if (!f) break;
      parts.unshift(f.name);
      cursor = f.parent;
    }
    return parts.join(' / ');
  };
  return (
    <>
      <section className="media-folder-view" aria-label={folder?.name ?? t(`${library}.title`)}>
        <aside
          className="media-folder-files"
          onContextMenu={(event) => {
            context(event, null);
          }}
        >
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
                onContextMenu={(event) => {
                  context(event, { folder: child });
                }}
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
                onContextMenu={(event) => {
                  context(event, { file });
                }}
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
          {files.length === 0 && children.length === 0 && (
            <p className="muted">{t('media.empty')}</p>
          )}
        </aside>
        <div
          className="media-folder-preview"
          onContextMenu={(event) => {
            context(event, asset ? { file: asset } : null);
          }}
        >
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
      {menu && items.length > 0 && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={items}
          label={t('media.files')}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
      {edit && isGm && (
        <form
          role={edit.kind === 'delete' ? 'alertdialog' : 'dialog'}
          aria-label={t('media.fileCommands', { defaultValue: 'Library commands' })}
          className="confirm-bar"
          onSubmit={(event) => {
            event.preventDefault();
            save();
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape' && !submit.isPending) setEdit(null);
          }}
        >
          {edit.kind === 'rename' || edit.kind === 'create' ? (
            <label>
              {t('media.itemName', { defaultValue: 'Name' })}
              <input
                autoFocus
                required
                maxLength={255}
                aria-label={t('media.itemName', { defaultValue: 'Name' })}
                value={name}
                onChange={(event) => {
                  setName(event.target.value);
                }}
              />
            </label>
          ) : edit.kind === 'move' ? (
            <label>
              {t('media.moveTo', { defaultValue: 'Move to folder…' })}
              <select
                autoFocus
                value={destination}
                onChange={(event) => {
                  setDestination(event.target.value);
                }}
              >
                <option value="">{t('library.root')}</option>
                {folders
                  .filter((f) => f.library === library && !excluded.has(f.id))
                  .map((f) => (
                    <option key={f.id} value={f.id}>
                      {folderPath(f.id)}
                    </option>
                  ))}
              </select>
            </label>
          ) : edit.target && 'folder' in edit.target ? (
            <p>
              {t('library.confirmDeleteFolder', {
                name: edit.target.folder.name,
                count: assets.filter((a) => a.folder_id && excluded.has(a.folder_id)).length,
              })}
            </p>
          ) : null}
          <ErrorText error={submit.error} />
          <div className="confirm-actions">
            <button
              type="submit"
              disabled={
                submit.isPending ||
                ((edit.kind === 'create' || edit.kind === 'rename') && !name.trim())
              }
            >
              {edit.kind === 'delete'
                ? t('journal.delete')
                : t('media.save', { defaultValue: 'Save' })}
            </button>
            <button
              type="button"
              autoFocus={edit.kind === 'delete'}
              disabled={submit.isPending}
              onClick={() => {
                setEdit(null);
              }}
            >
              {t('journal.cancel')}
            </button>
          </div>
        </form>
      )}
    </>
  );
}
