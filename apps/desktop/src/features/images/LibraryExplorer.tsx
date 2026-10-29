import { useMemo, useRef, useState, type CSSProperties, type DragEvent } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { assetUrl, type AssetDto, type Library, type LibraryFolderDto } from '@/shared/ipc/client';
import { useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';
import type { MenuItem } from '@/shared/ui/Menu';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';

type Row =
  | { kind: 'folder'; folder: LibraryFolderDto; depth: number; expanded: boolean }
  | { kind: 'asset'; asset: AssetDto; depth: number }
  | { kind: 'new'; parent: string | null; depth: number };
type DragItem = { kind: 'folder' | 'asset'; id: string };
const DRAG_TYPE = 'application/x-dndstudio-library';

/** Folder and file tree shared by the campaign's image and audio libraries. */
export function LibraryExplorer({
  library,
  assets,
  selected,
  onSelect,
  assetMenuItems,
}: {
  library: Library;
  assets: AssetDto[];
  selected: string | null;
  onSelect: (id: string | null) => void;
  assetMenuItems?: (asset: AssetDto) => (MenuItem | 'separator')[];
}) {
  const { t } = useTranslation();
  const folders = (useSnapshotSelect((s) => s.library_folders ?? []) ?? []).filter(
    (folder) => folder.library === library,
  );
  const submit = useSubmitIntent();
  const dragged = useRef<DragItem | null>(null);
  const previewTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [expandedState, setExpandedState] = useState<Set<string> | null>(null);
  const [creating, setCreating] = useState<{ parent: string | null; depth: number } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    folder: LibraryFolderDto | null;
    asset: AssetDto | null;
  } | null>(null);
  const [hoverPreview, setHoverPreview] = useState<{
    asset: AssetDto;
    left: number;
    top: number;
  } | null>(null);
  const [confirm, setConfirm] = useState<LibraryFolderDto | null>(null);
  const expanded = expandedState ?? new Set(folders.map((folder) => folder.id));
  const search = query.trim().toLocaleLowerCase();
  const matchingAssets = useMemo(
    () =>
      assets
        .filter(
          (asset) => !search || (asset.original_name ?? '').toLocaleLowerCase().includes(search),
        )
        .sort((a, b) => (a.original_name ?? '').localeCompare(b.original_name ?? '')),
    [assets, search],
  );
  const children = (parent: string | null) =>
    folders
      .filter((folder) => folder.parent === parent)
      .sort((a, b) => a.name.localeCompare(b.name));
  const containsMatch = (folderId: string): boolean => {
    const nested = new Set<string>([folderId]);
    const collect = (id: string) => {
      for (const child of children(id)) {
        nested.add(child.id);
        collect(child.id);
      }
    };
    collect(folderId);
    return matchingAssets.some((asset) => asset.folder_id != null && nested.has(asset.folder_id));
  };
  const rows = useMemo(() => {
    const result: Row[] = [];
    const visit = (parent: string | null, depth: number) => {
      for (const folder of children(parent)) {
        if (search && !containsMatch(folder.id)) continue;
        const isExpanded = search.length > 0 || expanded.has(folder.id);
        result.push({ kind: 'folder', folder, depth, expanded: isExpanded });
        if (isExpanded) visit(folder.id, depth + 1);
      }
      if (parent === null || expanded.has(parent)) {
        for (const asset of matchingAssets.filter((item) => (item.folder_id ?? null) === parent)) {
          result.push({ kind: 'asset', asset, depth });
        }
      }
      if (creating?.parent === parent) result.push({ kind: 'new', parent, depth });
    };
    visit(null, 0);
    return result;
    // Data is snapshotted; tree construction depends on current folder expansion and query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [folders, matchingAssets, expanded, creating, search]);

  const toggle = (id: string) => {
    setExpandedState((previous) => {
      const next = new Set(previous ?? folders.map((folder) => folder.id));
      if (!next.delete(id)) next.add(id);
      return next;
    });
  };
  const moveInto = (item: DragItem, parent: string | null) => {
    if (item.kind === 'asset') {
      const asset = assets.find((candidate) => candidate.id === item.id);
      if (!asset || (asset.folder_id ?? null) === parent) return;
      submit.mutate({
        intent: { kind: 'move_asset', id: asset.id, folder_id: parent },
        expectedVersion: asset.version,
      });
      if (selected === asset.id) onSelect(null);
      return;
    }
    const folder = folders.find((candidate) => candidate.id === item.id);
    if (!folder || folder.parent === parent || isDescendant(parent, folder.id)) return;
    submit.mutate({
      intent: { kind: 'move_library_folder', id: folder.id, parent },
      expectedVersion: folder.version,
    });
  };
  const isDescendant = (candidate: string | null, ancestor: string): boolean => {
    let id = candidate;
    while (id !== null) {
      if (id === ancestor) return true;
      id = folders.find((folder) => folder.id === id)?.parent ?? null;
    }
    return false;
  };
  const onDragOver = (event: DragEvent, parent: string | null) => {
    const item = dragged.current;
    if (!item || (item.kind === 'folder' && isDescendant(parent, item.id))) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    event.currentTarget.setAttribute('data-drop', 'true');
  };
  const onDrop = (event: DragEvent, parent: string | null) => {
    if (!dragged.current) return;
    event.preventDefault();
    event.currentTarget.removeAttribute('data-drop');
    moveInto(dragged.current, parent);
    dragged.current = null;
  };
  const startCreate = (parent: string | null) => {
    setCreating({ parent, depth: parent === null ? 0 : folderDepth(parent) + 1 });
    if (parent)
      setExpandedState((previous) => new Set([...(previous ?? folders.map((f) => f.id)), parent]));
  };
  const folderDepth = (id: string): number => {
    let depth = 0;
    let current = folders.find((folder) => folder.id === id);
    while (current?.parent) {
      depth += 1;
      current = folders.find((folder) => folder.id === current?.parent);
    }
    return depth;
  };
  const finishCreate = (name: string) => {
    const target = creating;
    setCreating(null);
    const trimmed = name.trim();
    if (!target || !trimmed) return;
    submit.mutate({
      intent: {
        kind: 'create_library_folder',
        id: uuidv7(),
        library,
        parent: target.parent,
        name: trimmed,
      },
    });
  };
  const removeFolder = (folder: LibraryFolderDto) => {
    setConfirm(null);
    submit.mutate({
      intent: { kind: 'delete_library_folder', id: folder.id },
      expectedVersion: folder.version,
    });
  };
  const folderMenu = (folder: LibraryFolderDto | null): MenuItem[] => {
    const items: MenuItem[] = [
      {
        label: t('library.addFolder'),
        onSelect: () => {
          startCreate(folder?.id ?? null);
        },
      },
    ];
    if (folder) {
      items.push(
        {
          label: t('library.renameFolder', { name: folder.name }),
          onSelect: () => {
            setRenaming(folder.id);
          },
        },
        {
          label: t('library.deleteFolder', { name: folder.name }),
          onSelect: () => {
            setConfirm(folder);
          },
        },
      );
    }
    return items;
  };
  const renderRow = (row: Row) => {
    const style = { '--depth': row.depth } as CSSProperties;
    if (row.kind === 'new') {
      return (
        <li role="none" className="nav-row tree-row library-tree-row" style={style}>
          <span className="tree-chevron-space" />
          <Icon name="folder" className="row-icon" />
          <LibraryNameInput
            label={t('library.folderName')}
            onDone={finishCreate}
            onCancel={() => {
              setCreating(null);
            }}
          />
        </li>
      );
    }
    if (row.kind === 'asset') {
      const isSelected = row.asset.id === selected;
      const openAssetMenu = (x: number, y: number) => {
        setMenu({ x, y, folder: null, asset: row.asset });
      };
      return (
        <li
          role="treeitem"
          aria-selected={isSelected}
          className="nav-row tree-row library-tree-row"
          style={style}
          key={row.asset.id}
          draggable
          onDragStart={(event) => {
            dragged.current = { kind: 'asset', id: row.asset.id };
            event.dataTransfer.setData(DRAG_TYPE, `asset:${row.asset.id}`);
            event.dataTransfer.effectAllowed = 'move';
          }}
          onDragOver={(event) => {
            onDragOver(event, row.asset.folder_id ?? null);
          }}
          onDrop={(event) => {
            onDrop(event, row.asset.folder_id ?? null);
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            openAssetMenu(event.clientX, event.clientY);
          }}
          onClick={() => {
            onSelect(row.asset.id);
          }}
        >
          <span className="tree-chevron-space" />
          {library === 'images' ? (
            <img
              className="library-tree-thumb"
              src={assetUrl(row.asset.thumbnail_sha256)}
              alt=""
              onMouseEnter={(event) => {
                if (previewTimer.current) clearTimeout(previewTimer.current);
                const rect = event.currentTarget.getBoundingClientRect();
                const width = Math.min(320, window.innerWidth - 16);
                setHoverPreview({
                  asset: row.asset,
                  left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)),
                  top: Math.max(8, Math.min(rect.bottom + 6, window.innerHeight - 260)),
                });
              }}
              onMouseLeave={() => {
                previewTimer.current = setTimeout(() => {
                  setHoverPreview(null);
                }, 180);
              }}
            />
          ) : (
            <Icon name="music" className="row-icon" />
          )}
          <span className="row-name" title={row.asset.original_name ?? undefined}>
            {row.asset.original_name ?? row.asset.sha256.slice(0, 8)}
          </span>
          {assetMenuItems && (
            <span
              className="library-row-actions"
              onClick={(event) => {
                event.stopPropagation();
              }}
            >
              <IconButton
                icon="more"
                label={t('library.assetActions', {
                  name: row.asset.original_name ?? row.asset.sha256.slice(0, 8),
                })}
                onClick={(event) => {
                  const rect = event.currentTarget.getBoundingClientRect();
                  openAssetMenu(rect.right, rect.bottom);
                }}
              />
            </span>
          )}
        </li>
      );
    }
    const { folder } = row;
    return (
      <li
        role="treeitem"
        aria-expanded={row.expanded}
        className="nav-row tree-row library-tree-row"
        style={style}
        key={folder.id}
        draggable={!renaming || renaming !== folder.id}
        onDragStart={(event) => {
          dragged.current = { kind: 'folder', id: folder.id };
          event.dataTransfer.setData(DRAG_TYPE, `folder:${folder.id}`);
          event.dataTransfer.effectAllowed = 'move';
        }}
        onDragEnd={() => {
          dragged.current = null;
        }}
        onDragOver={(event) => {
          onDragOver(event, folder.id);
        }}
        onDrop={(event) => {
          onDrop(event, folder.id);
        }}
        onClick={() => {
          toggle(folder.id);
        }}
        onDoubleClick={() => {
          setRenaming(folder.id);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu({ x: event.clientX, y: event.clientY, folder, asset: null });
        }}
      >
        <Icon name="chevron" className="tree-chevron" />
        <Icon name={row.expanded ? 'folderOpen' : 'folder'} className="row-icon" />
        {renaming === folder.id ? (
          <LibraryNameInput
            label={t('library.renameFolder', { name: folder.name })}
            initial={folder.name}
            onDone={(value) => {
              setRenaming(null);
              const name = value.trim();
              if (name && name !== folder.name)
                submit.mutate({
                  intent: { kind: 'rename_library_folder', id: folder.id, name },
                  expectedVersion: folder.version,
                });
            }}
            onCancel={() => {
              setRenaming(null);
            }}
          />
        ) : (
          <span className="row-name">{folder.name}</span>
        )}
      </li>
    );
  };

  const deleteContents = confirm ? countContents(confirm.id) : { assets: 0, folders: 0 };
  function countContents(id: string): { assets: number; folders: number } {
    const nested = new Set<string>([id]);
    const visit = (parent: string) => {
      for (const child of children(parent)) {
        nested.add(child.id);
        visit(child.id);
      }
    };
    visit(id);
    return {
      assets: assets.filter((asset) => asset.folder_id != null && nested.has(asset.folder_id))
        .length,
      folders: nested.size - 1,
    };
  }

  return (
    <section className="library-explorer" aria-label={t('library.folders')}>
      <div className="library-explorer-toolbar">
        <input
          type="search"
          aria-label={t('picker.search')}
          placeholder={t('picker.search')}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
        />
        <IconButton
          icon="folderPlus"
          label={t('library.addFolder')}
          onClick={() => {
            startCreate(null);
          }}
          disabled={creating !== null}
        />
        <IconButton
          icon="collapse"
          label={t('journal.collapseAll')}
          onClick={() => {
            setExpandedState(new Set());
          }}
        />
      </div>
      <div
        className="library-tree-root"
        data-drop="false"
        onDragOver={(event) => {
          onDragOver(event, null);
        }}
        onDragLeave={(event) => {
          event.currentTarget.removeAttribute('data-drop');
        }}
        onDrop={(event) => {
          onDrop(event, null);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu({ x: event.clientX, y: event.clientY, folder: null, asset: null });
        }}
      >
        <VirtualList<Row>
          items={rows}
          rowHeight={NAV_ROW_HEIGHT}
          getKey={(row) =>
            row.kind === 'folder'
              ? row.folder.id
              : row.kind === 'asset'
                ? row.asset.id
                : 'new-folder'
          }
          renderRow={renderRow}
          label={t('library.folders')}
          listProps={{ role: 'tree', tabIndex: 0 }}
        />
        {rows.length === 0 && <p className="nav-empty">{t('library.empty')}</p>}
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={menu.asset?.original_name ?? t('library.folders')}
          items={menu.asset ? (assetMenuItems?.(menu.asset) ?? []) : folderMenu(menu.folder)}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
      {hoverPreview &&
        createPortal(
          <div
            className="library-image-hover-preview"
            role="tooltip"
            style={{ left: hoverPreview.left, top: hoverPreview.top }}
            onMouseEnter={() => {
              if (previewTimer.current) clearTimeout(previewTimer.current);
            }}
            onMouseLeave={() => {
              setHoverPreview(null);
            }}
          >
            <img
              src={assetUrl(
                hoverPreview.asset.display_sha256 ?? hoverPreview.asset.thumbnail_sha256,
              )}
              alt={hoverPreview.asset.original_name ?? ''}
            />
          </div>,
          document.body,
        )}
      {confirm && (
        <div
          className="confirm-bar"
          role="alertdialog"
          aria-label={t('library.deleteFolder', { name: confirm.name })}
        >
          <p>
            {t('library.confirmDeleteFolder', { name: confirm.name, count: deleteContents.assets })}
          </p>
          {deleteContents.folders > 0 && (
            <p>{t('library.confirmDeleteFolderDetails', { folders: deleteContents.folders })}</p>
          )}
          <div className="confirm-actions">
            <button
              type="button"
              className="confirm-delete"
              onClick={() => {
                removeFolder(confirm);
              }}
            >
              {t('journal.delete')}
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirm(null);
              }}
            >
              {t('journal.cancel')}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}

function LibraryNameInput({
  label,
  initial = '',
  onDone,
  onCancel,
}: {
  label: string;
  initial?: string;
  onDone: (value: string) => void;
  onCancel: () => void;
}) {
  const completed = useRef(false);
  const finish = (value: string) => {
    if (completed.current) return;
    completed.current = true;
    onDone(value);
  };
  return (
    <input
      autoFocus
      defaultValue={initial}
      aria-label={label}
      onClick={(event) => {
        event.stopPropagation();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur();
        if (event.key === 'Escape') {
          completed.current = true;
          onCancel();
        }
      }}
      onBlur={(event) => {
        finish(event.currentTarget.value);
      }}
    />
  );
}
