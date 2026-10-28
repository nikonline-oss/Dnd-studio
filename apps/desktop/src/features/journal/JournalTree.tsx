import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type DragEvent,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import { JournalTreeModel, playerView, type Pending, type TreeRow } from './tree';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { newTable } from './table';
import { IconButton } from '@/shared/ui/IconButton';
import type { MenuItem } from '@/shared/ui/Menu';
import { NavSection } from '@/shared/ui/NavSection';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';

/** Drag payload inside the explorer: `folder:<id>` or `entry:<id>`. */
const JOURNAL_DRAG_TYPE = 'application/x-dndstudio-journal';

type Item = { kind: 'folder'; id: string } | { kind: 'entry'; id: string };

const rowKey = (row: TreeRow) =>
  row.kind === 'folder' ? row.folder.id : row.kind === 'entry' ? row.entry.id : 'new';

const isItem = (row: TreeRow, item: Item | null) =>
  item !== null &&
  ((row.kind === 'folder' && item.kind === 'folder' && row.folder.id === item.id) ||
    (row.kind === 'entry' && item.kind === 'entry' && row.entry.id === item.id));

const toItem = (row: TreeRow): Item | null =>
  row.kind === 'folder'
    ? { kind: 'folder', id: row.folder.id }
    : row.kind === 'entry'
      ? { kind: 'entry', id: row.entry.id }
      : null;

/**
 * Expanded folders, remembered per campaign in localStorage (a per-viewer convenience: without
 * storage the tree just starts collapsed).
 */
function useExpanded(campaignPath: string | undefined) {
  const key = campaignPath ? `dndstudio.journal.expanded:${campaignPath}` : null;
  const [state, setState] = useState(() => ({ key, set: loadExpanded(key) }));
  if (state.key !== key) setState({ key, set: loadExpanded(key) });

  const update = useCallback((change: (prev: ReadonlySet<string>) => Set<string>) => {
    setState((prev) => {
      const set = change(prev.set);
      saveExpanded(prev.key, set);
      return { key: prev.key, set };
    });
  }, []);
  return [state.set, update] as const;
}

function loadExpanded(key: string | null): ReadonlySet<string> {
  if (!key) return new Set();
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
    return new Set(Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

function saveExpanded(key: string | null, set: ReadonlySet<string>) {
  if (!key) return;
  try {
    localStorage.setItem(key, JSON.stringify([...set]));
  } catch {
    // Storage blocked or full: the tree still works, it just won't remember.
  }
}

/** Name input of a row being created or renamed: Enter/blur commits, Escape cancels. */
function NameInput({
  initial,
  label,
  onDone,
}: {
  initial: string;
  label: string;
  onDone: (name: string | null) => void;
}) {
  const [value, setValue] = useState(initial);
  const done = useRef(false);
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(name);
  };
  return (
    <input
      aria-label={label}
      autoFocus
      value={value}
      maxLength={200}
      onFocus={(e) => {
        e.currentTarget.select();
      }}
      onChange={(e) => {
        setValue(e.target.value);
      }}
      onBlur={() => {
        finish(value.trim() || null);
      }}
      onClick={(e) => {
        e.stopPropagation();
      }}
      onKeyDown={(e) => {
        // The tree's own keys (arrows, Delete, F2) must not act while typing.
        e.stopPropagation();
        if (e.key === 'Enter') finish(value.trim() || null);
        if (e.key === 'Escape') finish(null);
      }}
    />
  );
}

/**
 * Navigator: the journal as an explorer (ADR-0021). Folders expand and collapse; GM roles create
 * entries and folders right in the selected folder, rename (F2 / double click), drag them between
 * folders and delete them. Players see the entries open to them and the folders holding those.
 */
export function JournalTree() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const journal = useSnapshotSelect((s) => s.journal);
  const allFolders = useSnapshotSelect((s) => s.journal_folders);
  const submit = useSubmitIntent();
  const openId = useSelection((s) => s.journalId);
  const openJournal = useSelection((s) => s.openJournal);
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const treeId = useId();
  const treeRef = useRef<HTMLUListElement>(null);

  const model = useMemo(() => {
    const view = isGm
      ? { entries: journal ?? [], folders: allFolders ?? [] }
      : playerView(journal ?? [], allFolders ?? []);
    return new JournalTreeModel(view.entries, view.folders);
  }, [journal, allFolders, isGm]);
  const entryById = useMemo(() => new Map((journal ?? []).map((e) => [e.id, e])), [journal]);

  const [expanded, setExpanded] = useExpanded(campaign?.path);
  const [selected, setSelected] = useState<Item | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [renaming, setRenaming] = useState<Item | null>(null);
  const [confirm, setConfirm] = useState<Item | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; target: Item | null } | null>(null);
  /** Folder under the dragged item: an id, `null` for the root, `undefined` when not dragging. */
  const [dropTarget, setDropTarget] = useState<string | null | undefined>(undefined);
  const dragged = useRef<Item | null>(null);
  const closeMenu = useCallback(() => {
    setMenu(null);
  }, []);

  const rows = useMemo(() => model.rows(expanded, pending), [model, expanded, pending]);
  const current: Item | null = selected ?? (openId ? { kind: 'entry', id: openId } : null);
  const currentIndex = rows.findIndex((r) => isItem(r, current));
  const pendingIndex = rows.findIndex((r) => r.kind === 'new');

  const expand = useCallback(
    (folders: string[]) => {
      if (folders.length === 0) return;
      setExpanded((prev) => new Set([...prev, ...folders]));
    },
    [setExpanded],
  );

  // An entry opened anywhere (a tab, a link) is revealed: its folders open.
  const openFolder = openId ? entryById.get(openId)?.folder_id : undefined;
  useEffect(() => {
    if (openFolder) expand(model.path(openFolder));
  }, [openFolder, model, expand]);

  const folderOf = (item: Item | null): string | null => {
    if (!item) return null;
    if (item.kind === 'folder') return item.id;
    return entryById.get(item.id)?.folder_id ?? null;
  };

  const select = (item: Item | null) => {
    setSelected(item);
    if (item?.kind === 'entry') openJournal(item.id);
  };

  const startCreate = (type: Pending['type'], parent: string | null) => {
    setRenaming(null);
    setPending({ type, parent });
    expand(model.path(parent));
  };

  const finishCreate = (name: string | null) => {
    const target = pending;
    setPending(null);
    treeRef.current?.focus();
    if (!target || name === null) return;
    const id = uuidv7();
    if (target.type === 'folder') {
      submit.mutate(
        { intent: { kind: 'create_journal_folder', id, parent: target.parent, name } },
        {
          onSuccess: () => {
            setSelected({ kind: 'folder', id });
          },
        },
      );
      return;
    }
    submit.mutate(
      {
        intent: {
          kind: 'create_journal_entry',
          id,
          folder_id: target.parent,
          title: name,
          markdown: '',
          visibility: 'gm',
          players_can_edit: false,
          table: target.type === 'table' ? newTable() : null,
        },
      },
      {
        onSuccess: () => {
          select({ kind: 'entry', id });
        },
      },
    );
  };

  const finishRename = (item: Item, name: string | null) => {
    setRenaming(null);
    treeRef.current?.focus();
    if (name === null) return;
    if (item.kind === 'folder') {
      const folder = model.folders.get(item.id);
      if (!folder || folder.name === name) return;
      submit.mutate({
        intent: { kind: 'rename_journal_folder', id: folder.id, name },
        expectedVersion: folder.version,
      });
      return;
    }
    const entry = entryById.get(item.id);
    if (!entry || entry.title === name) return;
    submit.mutate({
      intent: {
        kind: 'update_journal_entry',
        id: entry.id,
        title: name,
        markdown: null,
        visibility: null,
        players_can_edit: null,
      },
      expectedVersion: entry.version,
    });
  };

  const remove = (item: Item) => {
    setConfirm(null);
    treeRef.current?.focus();
    if (item.kind === 'folder') {
      const folder = model.folders.get(item.id);
      if (!folder) return;
      submit.mutate({
        intent: { kind: 'delete_journal_folder', id: folder.id },
        expectedVersion: folder.version,
      });
    } else {
      const entry = entryById.get(item.id);
      if (!entry) return;
      submit.mutate({
        intent: { kind: 'delete_journal_entry', id: entry.id },
        expectedVersion: entry.version,
      });
    }
    if (selected && selected.id === item.id) setSelected(null);
  };

  /** Moves the dragged item into `folder` (`null` = root); no-op if it is already there. */
  const moveInto = (item: Item, folder: string | null) => {
    if (item.kind === 'entry') {
      const entry = entryById.get(item.id);
      if (!entry || entry.folder_id === folder) return;
      submit.mutate({
        intent: {
          kind: 'move_journal_entry',
          id: entry.id,
          folder_id: folder,
          sort_order: model.entries(folder).length,
        },
        expectedVersion: entry.version,
      });
    } else {
      const moving = model.folders.get(item.id);
      if (!moving || moving.parent === folder || model.isInside(folder, moving.id)) return;
      submit.mutate({
        intent: {
          kind: 'move_journal_folder',
          id: moving.id,
          parent: folder,
          sort_order: model.subfolders(folder).length,
        },
        expectedVersion: moving.version,
      });
    }
    expand(model.path(folder));
  };

  const canDropInto = (folder: string | null) => {
    const item = dragged.current;
    if (!item) return false;
    if (item.kind === 'folder') return !model.isInside(folder, item.id);
    return true;
  };

  const onDragOver = (e: DragEvent, folder: string | null) => {
    if (!e.dataTransfer.types.includes(JOURNAL_DRAG_TYPE) || !canDropInto(folder)) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'move';
    setDropTarget(folder);
  };

  const onDrop = (e: DragEvent, folder: string | null) => {
    const item = dragged.current;
    if (!item || !canDropInto(folder)) return;
    e.preventDefault();
    e.stopPropagation();
    dragged.current = null;
    setDropTarget(undefined);
    moveInto(item, folder);
  };

  const openMenu = (e: MouseEvent, target: Item | null) => {
    if (!isGm) return;
    e.preventDefault();
    e.stopPropagation();
    if (target) setSelected(target);
    setMenu({ x: e.clientX, y: e.clientY, target });
  };

  const menuItems = (target: Item | null): (MenuItem | 'separator')[] => {
    const into = folderOf(target);
    const items: (MenuItem | 'separator')[] = [
      {
        label: t('journal.add'),
        onSelect: () => {
          startCreate('entry', into);
        },
      },
      {
        label: t('journal.addTable'),
        onSelect: () => {
          startCreate('table', into);
        },
      },
      {
        label: t('journal.addFolder'),
        onSelect: () => {
          startCreate('folder', into);
        },
      },
    ];
    if (target) {
      items.push(
        'separator',
        {
          label: t('journal.rename'),
          onSelect: () => {
            setRenaming(target);
          },
        },
        {
          label: t('journal.delete'),
          onSelect: () => {
            setConfirm(target);
          },
        },
      );
    }
    return items;
  };

  const toggle = (folder: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (!next.delete(folder)) next.add(folder);
      return next;
    });
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const row = rows[currentIndex];
    const go = (index: number) => {
      const target = rows[index];
      const item = target && toItem(target);
      if (item) setSelected(item);
    };
    switch (e.key) {
      case 'ArrowDown':
        go(currentIndex < 0 ? 0 : Math.min(rows.length - 1, currentIndex + 1));
        break;
      case 'ArrowUp':
        go(currentIndex < 0 ? 0 : Math.max(0, currentIndex - 1));
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(rows.length - 1);
        break;
      case 'ArrowRight':
        if (row?.kind !== 'folder') return;
        if (!row.expanded) toggle(row.folder.id);
        else go(currentIndex + 1);
        break;
      case 'ArrowLeft': {
        if (row?.kind === 'folder' && row.expanded) {
          toggle(row.folder.id);
          break;
        }
        const parent =
          row?.kind === 'folder'
            ? row.folder.parent
            : row?.kind === 'entry'
              ? row.entry.folder_id
              : null;
        if (parent && model.folders.has(parent)) setSelected({ kind: 'folder', id: parent });
        break;
      }
      case 'Enter':
        if (row?.kind === 'folder') toggle(row.folder.id);
        else if (row?.kind === 'entry') openJournal(row.entry.id);
        else return;
        break;
      case 'F2':
        if (!isGm || !current || currentIndex < 0) return;
        setRenaming(current);
        break;
      case 'Delete':
        if (!isGm || !current || currentIndex < 0) return;
        setConfirm(current);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  const confirmText = (item: Item) => {
    if (item.kind === 'entry') {
      return t('journal.confirmDeleteEntry', { title: entryById.get(item.id)?.title ?? '' });
    }
    const name = model.folders.get(item.id)?.name ?? '';
    const { entries, folders } = model.contents(item.id);
    return entries === 0 && folders === 0
      ? t('journal.confirmDeleteEmptyFolder', { name })
      : t('journal.confirmDeleteFolder', { name, entries, folders });
  };

  const renderRow = (row: TreeRow) => {
    const style = { '--depth': row.depth } as CSSProperties;
    if (row.kind === 'new') {
      return (
        <li className="nav-row tree-row" role="none" style={style}>
          <span className="tree-chevron-space" />
          <Icon
            name={row.type === 'folder' ? 'folder' : row.type === 'table' ? 'table' : 'book'}
            className="row-icon"
          />
          <NameInput
            initial=""
            label={row.type === 'folder' ? t('journal.folderName') : t('journal.entryName')}
            onDone={finishCreate}
          />
        </li>
      );
    }
    const item = toItem(row);
    if (!item) return null;
    const isSelected = isItem(row, current);
    const isRenaming = renaming !== null && isItem(row, renaming);
    const name = row.kind === 'folder' ? row.folder.name : row.entry.title;
    const folderId = row.kind === 'folder' ? row.folder.id : row.entry.folder_id;
    return (
      <li
        id={`${treeId}-${item.id}`}
        role="treeitem"
        className="nav-row tree-row"
        style={style}
        aria-level={row.depth + 1}
        aria-expanded={row.kind === 'folder' ? row.expanded : undefined}
        aria-selected={isSelected}
        aria-current={row.kind === 'entry' && row.entry.id === openId}
        data-drop={row.kind === 'folder' && dropTarget === row.folder.id}
        draggable={isGm && !isRenaming}
        onDragStart={(e) => {
          dragged.current = item;
          e.dataTransfer.setData(JOURNAL_DRAG_TYPE, `${item.kind}:${item.id}`);
          e.dataTransfer.effectAllowed = 'move';
        }}
        onDragEnd={() => {
          dragged.current = null;
          setDropTarget(undefined);
        }}
        onDragOver={(e) => {
          onDragOver(e, folderId);
        }}
        onDrop={(e) => {
          onDrop(e, folderId);
        }}
        onClick={() => {
          select(item);
          if (row.kind === 'folder') toggle(row.folder.id);
        }}
        onDoubleClick={() => {
          if (isGm) setRenaming(item);
        }}
        onContextMenu={(e) => {
          openMenu(e, item);
        }}
      >
        {row.kind === 'folder' ? (
          <Icon name="chevron" className="tree-chevron" />
        ) : (
          <span className="tree-chevron-space" />
        )}
        {row.kind === 'folder' ? (
          <Icon name={row.expanded ? 'folderOpen' : 'folder'} className="row-icon" />
        ) : (
          <span title={t(`journal.visibility.${row.entry.visibility}`)} className="row-icon-box">
            <Icon
              name={
                row.entry.visibility === 'gm'
                  ? 'lock'
                  : row.entry.kind === 'table'
                    ? 'table'
                    : 'book'
              }
              className="row-icon"
            />
          </span>
        )}
        {isRenaming ? (
          <NameInput
            initial={name}
            label={t('journal.rename')}
            onDone={(next) => {
              finishRename(item, next);
            }}
          />
        ) : (
          <span className="row-name">{name}</span>
        )}
      </li>
    );
  };

  const target = folderOf(current);
  return (
    <NavSection
      icon="book"
      title={t('journal.title')}
      count={journal?.length ?? 0}
      actions={
        <>
          {isGm && (
            <>
              <IconButton
                icon="filePlus"
                label={t('journal.add')}
                onClick={() => {
                  startCreate('entry', target);
                }}
                disabled={submit.isPending}
              />
              <IconButton
                icon="table"
                label={t('journal.addTable')}
                onClick={() => {
                  startCreate('table', target);
                }}
                disabled={submit.isPending}
              />
              <IconButton
                icon="folderPlus"
                label={t('journal.addFolder')}
                onClick={() => {
                  startCreate('folder', target);
                }}
                disabled={submit.isPending}
              />
            </>
          )}
          <IconButton
            icon="collapse"
            label={t('journal.collapseAll')}
            onClick={() => {
              setExpanded(() => new Set());
            }}
          />
        </>
      }
    >
      <div
        className="journal-tree"
        data-drop={dropTarget === null}
        onDragOver={(e) => {
          onDragOver(e, null);
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropTarget(undefined);
        }}
        onDrop={(e) => {
          onDrop(e, null);
        }}
        onContextMenu={(e) => {
          openMenu(e, null);
        }}
      >
        {rows.length === 0 && <p className="nav-empty">{t('journal.empty')}</p>}
        <VirtualList<TreeRow>
          items={rows}
          rowHeight={NAV_ROW_HEIGHT}
          getKey={rowKey}
          renderRow={renderRow}
          scrollToIndex={pendingIndex >= 0 ? pendingIndex : currentIndex}
          listProps={{
            ref: treeRef,
            role: 'tree',
            'aria-label': t('journal.title'),
            tabIndex: 0,
            'aria-activedescendant':
              current && currentIndex >= 0 ? `${treeId}-${current.id}` : undefined,
            onKeyDown,
          }}
        />
      </div>
      {confirm && (
        <div
          className="confirm-bar"
          role="alertdialog"
          aria-label={t('journal.delete')}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              setConfirm(null);
              treeRef.current?.focus();
            }
          }}
        >
          <p>{confirmText(confirm)}</p>
          <div className="confirm-actions">
            <button
              type="button"
              className="confirm-delete"
              autoFocus
              onClick={() => {
                remove(confirm);
              }}
            >
              {t('journal.delete')}
            </button>
            <button
              type="button"
              onClick={() => {
                setConfirm(null);
                treeRef.current?.focus();
              }}
            >
              {t('journal.cancel')}
            </button>
          </div>
        </div>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t('journal.title')}
          items={menuItems(menu.target)}
          onClose={closeMenu}
        />
      )}
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
