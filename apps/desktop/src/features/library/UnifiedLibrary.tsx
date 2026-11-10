import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type DragEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from 'react';
import { createPortal } from 'react-dom';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import {
  assetUrl,
  ipc,
  type AssetDto,
  type JournalTable,
  type Library,
  type LibraryFolderDto,
  type PlaylistDto,
} from '@/shared/ipc/client';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import { playSharedPlaylist } from '@/shared/game/playlistPlayback';
import { useRemote } from '@/shared/game/useRemote';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';
import type { MenuItem } from '@/shared/ui/Menu';
import { useToast } from '@/shared/ui/Toast';
import { NAV_ROW_HEIGHT, VirtualList } from '@/shared/ui/VirtualList';
import {
  assetLabel,
  assetLibrary,
  folderKey,
  LibraryTreeModel,
  playerView,
  type FolderTarget,
  type LibraryRow,
  type PendingCreate,
} from './tree';

/** Drag payload inside the explorer: `journalFolder:<id>`, `asset:<id>` and so on. */
const DRAG_TYPE = 'application/x-dndstudio-library';
/** Protocol `MAX_PLAYLIST_TRACKS`. */
const MAX_TRACKS = 100;
/** Where the expansion of the navigator is remembered, per campaign. */
const EXPANDED_KEY = 'dndstudio.library.expanded';

type Item =
  | { kind: 'journalFolder'; id: string }
  | { kind: 'entry'; id: string }
  | { kind: 'libraryFolder'; id: string }
  | { kind: 'asset'; id: string }
  | { kind: 'playlist'; id: string };

type Confirmation =
  | { kind: 'journal'; item: Item }
  | { kind: 'libraryFolder'; folder: LibraryFolderDto }
  | { kind: 'playlist'; playlist: PlaylistDto };

const ITEM_PREFIX = {
  journalFolder: 'j',
  entry: 'e',
  libraryFolder: 'l',
  asset: 'a',
  playlist: 'p',
} as const;

const itemKey = (item: Item) => `${ITEM_PREFIX[item.kind]}:${item.id}`;
const journalKey = (id: string) => folderKey({ kind: 'journal', id });
const libraryKey = (id: string) => folderKey({ kind: 'library', id });

/** The row as a selectable item, or `null` for the create-name row. */
function rowItem(row: LibraryRow): Item | null {
  switch (row.kind) {
    case 'journalFolder':
      return { kind: 'journalFolder', id: row.folder.id };
    case 'libraryFolder':
      return { kind: 'libraryFolder', id: row.folder.id };
    case 'entry':
      return { kind: 'entry', id: row.entry.id };
    case 'asset':
      return { kind: 'asset', id: row.asset.id };
    case 'playlist':
      return { kind: 'playlist', id: row.playlist.id };
    case 'new':
      return null;
  }
}

const rowKey = (row: LibraryRow) => {
  const item = rowItem(row);
  return item ? itemKey(item) : 'new';
};

const sameItem = (a: Item | null, b: Item | null) =>
  a !== null && b !== null && a.kind === b.kind && a.id === b.id;

/** A fresh journal table: three named columns and three empty rows (protocol defaults). */
function newTable(): JournalTable {
  const width = 140;
  return {
    columns: ['A', 'B', 'C'].map((name) => ({ name, width })),
    rows: Array.from({ length: 3 }, () => ['', '', '']),
  };
}

/** How a picture is written into a journal text (ADR-0024). */
export function pictureMarkdown(asset: AssetDto): string {
  const alt = (asset.original_name ?? '').replace(/\.[a-z0-9]+$/i, '').replace(/[[\]]/g, '');
  return `![${alt}](asset:${asset.id})`;
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

function loadExpanded(key: string | null): ReadonlySet<string> | null {
  if (!key) return null;
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(key) ?? 'null');
    if (!Array.isArray(raw)) return null;
    return new Set(raw.filter((value): value is string => typeof value === 'string'));
  } catch {
    return null;
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

/**
 * The campaign navigator: one tree for the journal and the media library (ADR-0021, ADR-0026).
 * Journal folders and entries, library folders, pictures, sounds and playlists share rows,
 * sorting and drag-move; the commands of each kind stay what they were. GM roles get the
 * library; players see the entries open to them and the folders holding them.
 */
export function UnifiedLibrary() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const journal = useSnapshotSelect((s) => s.journal);
  const journalFolders = useSnapshotSelect((s) => s.journal_folders);
  const libraryFolders = useSnapshotSelect((s) => s.library_folders);
  const assets = useSnapshotSelect((s) => s.assets);
  const playlists = useSnapshotSelect((s) => s.playlists);
  const sounds = useSnapshotSelect((s) => s.sounds);
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const remote = useRemote();
  const canImport =
    isGm &&
    (remote === null ||
      remote.status.state === 'online' ||
      (remote.hosted && remote.status.state === 'reconnecting'));
  const submit = useSubmitIntent();
  const treeRef = useRef<HTMLUListElement>(null);
  const treeId = useId();

  const model = useMemo(() => {
    const view = isGm
      ? { entries: journal ?? [], folders: journalFolders ?? [] }
      : playerView(journal ?? [], journalFolders ?? []);
    return new LibraryTreeModel({
      entries: view.entries,
      journalFolders: view.folders,
      libraryFolders: isGm ? (libraryFolders ?? []) : [],
      assets: isGm ? (assets ?? []) : [],
      playlists: isGm ? (playlists ?? []) : [],
    });
  }, [journal, journalFolders, libraryFolders, assets, playlists, isGm]);

  const storageKey = campaign?.path ? `${EXPANDED_KEY}:${campaign.path}` : null;
  const [expandedState, setExpandedState] = useState<ReadonlySet<string> | null>(() =>
    loadExpanded(storageKey),
  );
  const [expandedFor, setExpandedFor] = useState(storageKey);
  if (expandedFor !== storageKey) {
    // Another campaign: its own remembered expansion (or the default).
    setExpandedFor(storageKey);
    setExpandedState(loadExpanded(storageKey));
  }
  const defaultExpanded = useMemo(
    () => new Set((libraryFolders ?? []).map((folder) => libraryKey(folder.id))),
    [libraryFolders],
  );
  const expanded = expandedState ?? defaultExpanded;
  const updateExpanded = useCallback(
    (change: (next: Set<string>) => void) => {
      setExpandedState((previous) => {
        const next = new Set(previous ?? defaultExpanded);
        change(next);
        saveExpanded(storageKey, next);
        return next;
      });
    },
    [defaultExpanded, storageKey],
  );
  const expand = useCallback(
    (keys: string[]) => {
      updateExpanded((next) => {
        for (const key of keys) next.add(key);
      });
    },
    [updateExpanded],
  );

  const [selected, setSelected] = useState<Item | null>(null);
  const [pending, setPending] = useState<PendingCreate | null>(null);
  const [renaming, setRenaming] = useState<Item | null>(null);
  const [confirm, setConfirm] = useState<Confirmation | null>(null);
  const [moving, setMoving] = useState<Item | null>(null);
  const [moveFolder, setMoveFolder] = useState('');
  const [query, setQuery] = useState('');
  const [menu, setMenu] = useState<{ x: number; y: number; item: Item | null } | null>(null);
  const [playError, setPlayError] = useState<unknown>(null);
  const [hoverPreview, setHoverPreview] = useState<{
    asset: AssetDto;
    left: number;
    top: number;
  } | null>(null);
  const previewTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const importImages = useMutation({ mutationFn: ipc.assetImportImages });
  const importAudio = useMutation({ mutationFn: ipc.assetImportAudio });
  const importing = importImages.isPending || importAudio.isPending;
  const busy = submit.isPending || importing;
  const showToast = useToast((state) => state.show);
  const openId = useSelection((state) => state.journalId);
  const openJournal = useSelection((state) => state.openJournal);
  const openMedia = useSelection((state) => state.openMedia);
  const active = useSelection((state) => state.active);
  const lastBoard = useSelection((state) => state.tabs.findLast((tab) => tab.kind === 'board'));
  const boardId = active?.kind === 'board' ? active.id : (lastBoard?.id ?? null);
  const current: Item | null = selected ?? (openId ? { kind: 'entry', id: openId } : null);

  const rows = useMemo(
    () => model.rows(expanded, pending, query),
    [model, expanded, pending, query],
  );
  const currentIndex = rows.findIndex((row) => sameItem(rowItem(row), current));
  const pendingIndex = rows.findIndex((row) => row.kind === 'new');

  // An entry opened anywhere (a tab, a link) is revealed once: its folders open, but a user can
  // still collapse them afterwards.
  const openFolder = openId ? (model.entryById.get(openId)?.folder_id ?? null) : null;
  const [revealed, setRevealed] = useState<string | null>(null);
  if (revealed !== openFolder) {
    setRevealed(openFolder);
    if (openFolder) {
      const keys = model.journalPath(openFolder).map((id) => journalKey(id));
      setExpandedState((previous) => {
        const next = new Set(previous ?? defaultExpanded);
        for (const key of keys) next.add(key);
        return next;
      });
    }
  }

  const toggle = (key: string) => {
    updateExpanded((next) => {
      if (!next.delete(key)) next.add(key);
    });
  };
  const expandAll = () => {
    updateExpanded((next) => {
      for (const folder of model.journalFolders.keys()) next.add(journalKey(folder));
      for (const folder of model.libraryFolders.keys()) next.add(libraryKey(folder));
    });
  };
  const collapseAll = () => {
    updateExpanded((next) => {
      next.clear();
    });
  };

  const journalTarget = ((): string | null => {
    if (selected?.kind === 'journalFolder') return selected.id;
    if (selected?.kind === 'entry') return model.entryById.get(selected.id)?.folder_id ?? null;
    // No row selected: an entry open in a tab still decides where a new sibling goes.
    if (active?.kind === 'journal') return model.entryById.get(active.id)?.folder_id ?? null;
    return null;
  })();
  const libraryTarget = ((): { parent: string | null; library: Library } => {
    if (selected?.kind === 'libraryFolder') {
      const folder = model.libraryFolders.get(selected.id);
      return { parent: folder?.id ?? null, library: folder?.library ?? 'images' };
    }
    if (selected?.kind === 'asset') {
      const asset = model.assetById.get(selected.id);
      return {
        parent: asset?.folder_id ?? null,
        library: asset ? assetLibrary(asset) : 'images',
      };
    }
    if (selected?.kind === 'playlist') {
      return { parent: null, library: 'audio' };
    }
    // No row selected: the open media tab says whether folders are for pictures or for sound.
    if (active?.kind === 'audio' || active?.kind === 'images') {
      return { parent: null, library: active.kind };
    }
    return { parent: null, library: 'images' };
  })();

  const expandJournalPath = (folder: string | null) => {
    expand(model.journalPath(folder).map((id) => journalKey(id)));
  };
  const expandLibraryPath = (folder: string | null) => {
    expand(model.libraryPath(folder).map((id) => libraryKey(id)));
  };

  // A pending row lives inside its folder; a search that hides that folder would make the create
  // click do nothing visible. Starting one clears the query, which is what the reader expects.
  const startCreate = (type: 'entry' | 'table' | 'journalFolder', parent: string | null) => {
    if (!isGm || busy) return;
    setRenaming(null);
    setQuery('');
    setPending({ type, parent });
    expandJournalPath(parent);
  };
  const startLibraryFolder = (library: Library, parent: string | null) => {
    if (!isGm || busy) return;
    setRenaming(null);
    setQuery('');
    setPending({ type: 'libraryFolder', parent, library });
    expandLibraryPath(parent);
  };
  const startPlaylist = () => {
    if (!isGm || busy) return;
    setRenaming(null);
    setQuery('');
    setPending({ type: 'playlist' });
  };

  const finishCreate = (name: string | null) => {
    const target = pending;
    setPending(null);
    treeRef.current?.focus();
    if (target === null || name === null || !isGm || busy) return;
    if (target.type === 'playlist') {
      submit.mutate({
        intent: { kind: 'create_playlist', id: uuidv7(), name, folder_id: null, asset_ids: [] },
      });
      return;
    }
    if (target.type === 'libraryFolder') {
      submit.mutate({
        intent: {
          kind: 'create_library_folder',
          id: uuidv7(),
          library: target.library,
          parent: target.parent,
          name,
        },
      });
      return;
    }
    const id = uuidv7();
    if (target.type === 'journalFolder') {
      submit.mutate(
        { intent: { kind: 'create_journal_folder', id, parent: target.parent, name } },
        {
          onSuccess: () => {
            setSelected({ kind: 'journalFolder', id });
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
          setSelected({ kind: 'entry', id });
          openJournal(id);
        },
      },
    );
  };

  const canRename = (item: Item): boolean => {
    if (isGm) return true;
    if (item.kind !== 'entry') return false;
    const entry = model.entryById.get(item.id);
    return campaign?.role === 'player' && entry?.visibility === 'players' && entry.players_can_edit;
  };

  const finishRename = (item: Item, name: string | null) => {
    setRenaming(null);
    treeRef.current?.focus();
    if (name === null || busy) return;
    switch (item.kind) {
      case 'journalFolder': {
        const folder = model.journalFolders.get(item.id);
        if (!folder || folder.name === name) return;
        submit.mutate({
          intent: { kind: 'rename_journal_folder', id: folder.id, name },
          expectedVersion: folder.version,
        });
        return;
      }
      case 'entry': {
        const entry = model.entryById.get(item.id);
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
        return;
      }
      case 'libraryFolder': {
        const folder = model.libraryFolders.get(item.id);
        if (!folder || folder.name === name) return;
        submit.mutate({
          intent: { kind: 'rename_library_folder', id: folder.id, name },
          expectedVersion: folder.version,
        });
        return;
      }
      case 'asset': {
        const asset = model.assetById.get(item.id);
        if (!asset) return;
        submit.mutate({
          intent: { kind: 'rename_asset', id: asset.id, name },
          expectedVersion: asset.version,
        });
        return;
      }
      case 'playlist': {
        const playlist = model.playlistById.get(item.id);
        if (!playlist || playlist.name === name) return;
        submit.mutate({
          intent: {
            kind: 'update_playlist',
            id: playlist.id,
            name,
            tracks: playlist.tracks,
          },
          expectedVersion: playlist.version,
        });
        return;
      }
    }
  };

  const confirmationFor = (item: Item): Confirmation | null => {
    switch (item.kind) {
      case 'journalFolder':
      case 'entry':
        return { kind: 'journal', item };
      case 'libraryFolder': {
        const folder = model.libraryFolders.get(item.id);
        return folder ? { kind: 'libraryFolder', folder } : null;
      }
      case 'playlist': {
        const playlist = model.playlistById.get(item.id);
        return playlist ? { kind: 'playlist', playlist } : null;
      }
      case 'asset':
        return null;
    }
  };

  const remove = (target: Confirmation) => {
    setConfirm(null);
    treeRef.current?.focus();
    if (!isGm || busy) return;
    if (target.kind === 'libraryFolder') {
      submit.mutate({
        intent: { kind: 'delete_library_folder', id: target.folder.id },
        expectedVersion: target.folder.version,
      });
      if (selected?.kind === 'libraryFolder' && selected.id === target.folder.id) setSelected(null);
      return;
    }
    if (target.kind === 'playlist') {
      submit.mutate({
        intent: { kind: 'delete_playlist', id: target.playlist.id },
        expectedVersion: target.playlist.version,
      });
      if (selected?.kind === 'playlist' && selected.id === target.playlist.id) setSelected(null);
      return;
    }
    const item = target.item;
    if (item.kind === 'journalFolder') {
      const folder = model.journalFolders.get(item.id);
      if (!folder) return;
      submit.mutate({
        intent: { kind: 'delete_journal_folder', id: folder.id },
        expectedVersion: folder.version,
      });
    } else if (item.kind === 'entry') {
      const entry = model.entryById.get(item.id);
      if (!entry) return;
      submit.mutate({
        intent: { kind: 'delete_journal_entry', id: entry.id },
        expectedVersion: entry.version,
      });
    }
    if (sameItem(selected, item)) setSelected(null);
  };

  /** Moves the dragged item into `target` (`null`: the root of its own namespace). */
  const moveItem = (item: Item, target: FolderTarget | null) => {
    if (!isGm || busy) return;
    switch (item.kind) {
      case 'entry': {
        const entry = model.entryById.get(item.id);
        const folder = target?.kind === 'journal' ? target.id : null;
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
        expandJournalPath(folder);
        return;
      }
      case 'journalFolder': {
        const folder = model.journalFolders.get(item.id);
        const parent = target?.kind === 'journal' ? target.id : null;
        if (!folder || folder.parent === parent || model.isInsideJournal(parent, folder.id)) return;
        submit.mutate({
          intent: {
            kind: 'move_journal_folder',
            id: folder.id,
            parent,
            sort_order: model.journalSubfolders(parent).length,
          },
          expectedVersion: folder.version,
        });
        expandJournalPath(parent);
        return;
      }
      case 'libraryFolder': {
        const folder = model.libraryFolders.get(item.id);
        const parent = target?.kind === 'library' ? target.id : null;
        if (!folder || folder.parent === parent || model.isInsideLibrary(parent, folder.id)) return;
        if (parent !== null && model.libraryOf(parent) !== folder.library) return;
        submit.mutate({
          intent: { kind: 'move_library_folder', id: folder.id, parent },
          expectedVersion: folder.version,
        });
        expandLibraryPath(parent);
        return;
      }
      case 'asset': {
        const asset = model.assetById.get(item.id);
        const parent = target?.kind === 'library' ? target.id : null;
        if (!asset || (asset.folder_id ?? null) === parent) return;
        if (parent !== null && model.libraryOf(parent) !== assetLibrary(asset)) return;
        moveAsset(asset, parent);
        return;
      }
      case 'playlist':
        return;
    }
  };

  const moveAsset = (asset: AssetDto, folderId: string | null) => {
    submit.mutate({
      intent: { kind: 'move_asset', id: asset.id, folder_id: folderId },
      expectedVersion: asset.version,
    });
    if (selected?.kind === 'asset' && selected.id === asset.id) setSelected(null);
  };

  const canDrop = (item: Item, target: FolderTarget | null): boolean => {
    switch (item.kind) {
      case 'entry':
        return target === null || target.kind === 'journal';
      case 'journalFolder':
        return (
          target === null ||
          (target.kind === 'journal' && !model.isInsideJournal(target.id, item.id))
        );
      case 'libraryFolder': {
        if (target === null) return true;
        const folder = model.libraryFolders.get(item.id);
        return (
          target.kind === 'library' &&
          folder !== undefined &&
          model.libraryOf(target.id) === folder.library &&
          !model.isInsideLibrary(target.id, folder.id)
        );
      }
      case 'asset': {
        if (target === null) return true;
        const asset = model.assetById.get(item.id);
        return (
          target.kind === 'library' &&
          asset !== undefined &&
          model.libraryOf(target.id) === assetLibrary(asset)
        );
      }
      case 'playlist':
        return false;
    }
  };

  const dragged = useRef<Item | null>(null);
  const [dropKey, setDropKey] = useState<string | null>(null);
  const finishDrag = useCallback(() => {
    dragged.current = null;
    setDropKey(null);
  }, []);
  useEffect(() => {
    const end = () => {
      finishDrag();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') finishDrag();
    };
    window.addEventListener('dragend', end);
    window.addEventListener('drop', end);
    window.addEventListener('blur', end);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('dragend', end);
      window.removeEventListener('drop', end);
      window.removeEventListener('blur', end);
      window.removeEventListener('keydown', onKey);
      finishDrag();
    };
  }, [finishDrag]);

  const startDrag = (event: DragEvent, item: Item) => {
    finishDrag();
    dragged.current = item;
    event.dataTransfer.setData(DRAG_TYPE, `${item.kind}:${item.id}`);
    event.dataTransfer.effectAllowed = 'move';
  };
  const onRowDragOver = (event: DragEvent, target: FolderTarget | null, key: string) => {
    const item = dragged.current;
    if (!item) return;
    event.stopPropagation();
    if (!canDrop(item, target)) {
      setDropKey(null);
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    setDropKey(key);
  };
  const onRowDrop = (event: DragEvent, target: FolderTarget | null) => {
    const item = dragged.current;
    if (!item) return;
    event.preventDefault();
    event.stopPropagation();
    finishDrag();
    if (!canDrop(item, target)) return;
    moveItem(item, target);
  };
  const onDragLeave = (event: DragEvent, key: string) => {
    const next = event.relatedTarget;
    if (next instanceof Node && event.currentTarget.contains(next)) return;
    setDropKey((current) => (current === key ? null : current));
  };

  const canAddToPlaylist = (playlist: PlaylistDto): boolean => {
    const item = dragged.current;
    if (item?.kind !== 'asset') return false;
    const asset = model.assetById.get(item.id);
    return (
      asset !== undefined &&
      asset.usage === 'audio' &&
      playlist.tracks.length < MAX_TRACKS &&
      !playlist.tracks.some((track) => track.asset_id === asset.id)
    );
  };
  const onPlaylistDragOver = (event: DragEvent, playlist: PlaylistDto) => {
    if (!dragged.current) return;
    event.stopPropagation();
    if (!canAddToPlaylist(playlist)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
    setDropKey(`p:${playlist.id}`);
  };
  const onPlaylistDrop = (event: DragEvent, playlist: PlaylistDto) => {
    if (!dragged.current) return;
    event.preventDefault();
    event.stopPropagation();
    const allowed = canAddToPlaylist(playlist);
    const item = dragged.current;
    finishDrag();
    if (!allowed || item.kind !== 'asset') return;
    addToPlaylist(playlist, item.id);
  };
  /** Dropping an audio asset on a playlist appends it as a copied track. */
  const addToPlaylist = (playlist: PlaylistDto, assetId: string) => {
    const asset = model.assetById.get(assetId);
    if (!asset || asset.usage !== 'audio') return;
    if (playlist.tracks.length >= MAX_TRACKS) return;
    if (playlist.tracks.some((track) => track.asset_id === asset.id)) return;
    submit.mutate({
      intent: {
        kind: 'update_playlist',
        id: playlist.id,
        name: playlist.name,
        tracks: [
          ...playlist.tracks,
          {
            asset_id: asset.id,
            sha256: asset.sha256,
            mime: asset.mime,
            title: asset.original_name ?? asset.sha256.slice(0, 8),
          },
        ],
      },
      expectedVersion: playlist.version,
    });
  };

  const openItem = (item: Item) => {
    switch (item.kind) {
      case 'entry':
        openJournal(item.id);
        return;
      case 'libraryFolder': {
        const folder = model.libraryFolders.get(item.id);
        if (folder) openMedia(folder.library, folder.id);
        return;
      }
      case 'asset': {
        const asset = model.assetById.get(item.id);
        if (asset) openMedia(assetLibrary(asset), asset.folder_id ?? null, asset.id);
        return;
      }
      case 'playlist': {
        const playlist = model.playlistById.get(item.id);
        if (playlist) openMedia('audio', playlist.folder_id ?? null, `playlist:${playlist.id}`);
        return;
      }
      case 'journalFolder':
        return;
    }
  };

  const menuName = (item: Item): string => {
    switch (item.kind) {
      case 'journalFolder':
        return model.journalFolders.get(item.id)?.name ?? '';
      case 'libraryFolder':
        return model.libraryFolders.get(item.id)?.name ?? '';
      case 'entry':
        return model.entryById.get(item.id)?.title ?? '';
      case 'asset': {
        const asset = model.assetById.get(item.id);
        return asset ? assetLabel(asset) : '';
      }
      case 'playlist':
        return model.playlistById.get(item.id)?.name ?? '';
    }
  };

  const openMenu = (event: ReactMouseEvent, item: Item | null) => {
    if ((event.target as HTMLElement).closest('input, textarea, select')) return;
    event.preventDefault();
    event.stopPropagation();
    if (item) setSelected(item);
    setMenu({ x: event.clientX, y: event.clientY, item });
  };

  const updateAccess = (
    id: string,
    patch: { visibility?: 'gm' | 'players'; players_can_edit?: boolean },
  ) => {
    const entry = model.entryById.get(id);
    if (!entry || !isGm || busy) return;
    submit.mutate({
      intent: {
        kind: 'update_journal_entry',
        id,
        title: null,
        markdown: null,
        visibility: null,
        players_can_edit: null,
        ...patch,
      },
      expectedVersion: entry.version,
    });
  };

  const folderPath = (id: string): string => {
    const names: string[] = [];
    let folder = model.libraryFolders.get(id);
    while (folder && names.length < 32) {
      names.unshift(folder.name);
      folder = folder.parent ? model.libraryFolders.get(folder.parent) : undefined;
    }
    return names.join(' / ');
  };
  const libraryFoldersOf = (library: Library) =>
    [...model.libraryFolders.values()]
      .filter((folder) => folder.library === library)
      .sort((a, b) => folderPath(a.id).localeCompare(folderPath(b.id)));

  const pictureItems = (asset: AssetDto): (MenuItem | 'separator')[] => [
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
            content: { type: 'image', asset_id: asset.id },
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
        void navigator.clipboard.writeText(pictureMarkdown(asset)).then(() => {
          showToast(t('images.copied'));
        });
      },
    },
    'separator',
    {
      label: t('library.moveAssetRoot'),
      onSelect: () => {
        moveAsset(asset, null);
      },
    },
    ...libraryFoldersOf('images').map((folder) => ({
      label: t('images.moveToFolder', { name: folderPath(folder.id) }),
      checked: asset.folder_id === folder.id,
      onSelect: () => {
        moveAsset(asset, folder.id);
      },
    })),
  ];

  const soundItems = (asset: AssetDto): (MenuItem | 'separator')[] => [
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
    ...((sounds ?? []).some((sound) => sound.asset_id === asset.id)
      ? [
          {
            label: t('audio.stop'),
            onSelect: () => {
              submit.mutate({ intent: { kind: 'stop_sound', asset_id: asset.id } });
            },
          },
        ]
      : []),
    'separator',
    {
      label: t('library.moveAssetRoot'),
      onSelect: () => {
        moveAsset(asset, null);
      },
    },
    ...libraryFoldersOf('audio').map((folder) => ({
      label: t('images.moveToFolder', { name: folderPath(folder.id) }),
      checked: asset.folder_id === folder.id,
      onSelect: () => {
        moveAsset(asset, folder.id);
      },
    })),
  ];

  const createItems = (parent: string | null): MenuItem[] =>
    isGm
      ? [
          {
            label: t('journal.add'),
            disabled: busy,
            onSelect: () => {
              startCreate('entry', parent);
            },
          },
          {
            label: t('journal.addTable'),
            disabled: busy,
            onSelect: () => {
              startCreate('table', parent);
            },
          },
          {
            label: t('journal.addFolder'),
            disabled: busy,
            onSelect: () => {
              startCreate('journalFolder', parent);
            },
          },
        ]
      : [];

  const backgroundItems = (): (MenuItem | 'separator')[] => {
    const items: (MenuItem | 'separator')[] = [...createItems(null)];
    if (isGm) {
      items.push(
        'separator',
        {
          label: t('library.addPictureFolder'),
          disabled: busy,
          onSelect: () => {
            startLibraryFolder('images', null);
          },
        },
        {
          label: t('library.addAudioFolder'),
          disabled: busy,
          onSelect: () => {
            startLibraryFolder('audio', null);
          },
        },
      );
    }
    if (isGm) {
      items.push('separator', {
        label: t('media.openLibrary', { defaultValue: 'Open pictures' }),
        onSelect: () => {
          openMedia('images', null);
        },
      });
      items.push({
        label: t('audio.openLibrary', { defaultValue: 'Open music' }),
        onSelect: () => {
          openMedia('audio', null);
        },
      });
    }
    if (canImport) {
      items.push({
        label: t('images.import'),
        disabled: importing,
        onSelect: () => {
          importImages.mutate();
        },
      });
      items.push({
        label: t('audio.import'),
        disabled: importing,
        onSelect: () => {
          importAudio.mutate();
        },
      });
    }
    if (items.length > 0) items.push('separator');
    items.push(
      { label: t('context.expandAll'), onSelect: expandAll },
      { label: t('journal.collapseAll'), onSelect: collapseAll },
    );
    return items;
  };

  const menuItems = (item: Item | null): (MenuItem | 'separator')[] => {
    if (item === null) return backgroundItems();
    const items: (MenuItem | 'separator')[] = [];
    if (item.kind === 'journalFolder') {
      const folder = model.journalFolders.get(item.id);
      if (!folder) return backgroundItems();
      const key = journalKey(folder.id);
      items.push({
        label: expanded.has(key) ? t('context.collapse') : t('context.expand'),
        onSelect: () => {
          toggle(key);
        },
      });
      items.push(...createItems(folder.id));
      if (canRename(item)) {
        items.push({
          label: t('journal.rename'),
          disabled: busy,
          onSelect: () => {
            setRenaming(item);
          },
        });
      }
      if (isGm) {
        items.push(
          {
            label: t('context.moveTo'),
            disabled: busy,
            onSelect: () => {
              setMoving(item);
              setMoveFolder(model.journalFolders.get(item.id)?.parent ?? '');
            },
          },
          {
            label: t('journal.delete'),
            disabled: busy,
            onSelect: () => {
              setConfirm({ kind: 'journal', item });
            },
          },
        );
      }
    } else if (item.kind === 'entry') {
      const entry = model.entryById.get(item.id);
      if (!entry) return backgroundItems();
      items.push({
        label: t('context.open'),
        onSelect: () => {
          openJournal(entry.id);
        },
      });
      if (isGm) {
        items.push(
          {
            label:
              entry.visibility === 'players' ? t('context.hidePlayers') : t('context.showPlayers'),
            disabled: busy,
            onSelect: () => {
              updateAccess(entry.id, {
                visibility: entry.visibility === 'players' ? 'gm' : 'players',
              });
            },
          },
          {
            label: entry.players_can_edit
              ? t('journal.disallowPlayerEditing')
              : t('journal.playersCanEdit'),
            disabled: busy,
            onSelect: () => {
              updateAccess(entry.id, { players_can_edit: !entry.players_can_edit });
            },
          },
        );
      }
      items.push(...createItems(entry.folder_id));
      if (canRename(item)) {
        items.push({
          label: t('journal.rename'),
          disabled: busy,
          onSelect: () => {
            setRenaming(item);
          },
        });
      }
      if (isGm) {
        items.push(
          {
            label: t('context.moveTo'),
            disabled: busy,
            onSelect: () => {
              setMoving(item);
              setMoveFolder(entry.folder_id ?? '');
            },
          },
          {
            label: t('journal.delete'),
            disabled: busy,
            onSelect: () => {
              setConfirm({ kind: 'journal', item });
            },
          },
        );
      }
    } else if (item.kind === 'libraryFolder') {
      const folder = model.libraryFolders.get(item.id);
      if (!folder) return backgroundItems();
      items.push({
        label: t('media.open'),
        onSelect: () => {
          openItem(item);
        },
      });
      if (isGm) {
        items.push({
          label: t('library.addFolder'),
          disabled: busy,
          onSelect: () => {
            startLibraryFolder(folder.library, folder.id);
          },
        });
      }
      if (canImport) {
        items.push(
          {
            label: t('images.import'),
            disabled: importing,
            onSelect: () => {
              importImages.mutate();
            },
          },
          {
            label: t('audio.import'),
            disabled: importing,
            onSelect: () => {
              importAudio.mutate();
            },
          },
        );
      }
      if (isGm) {
        items.push(
          {
            label: t('library.renameFolder', { name: folder.name }),
            disabled: busy,
            onSelect: () => {
              setRenaming(item);
            },
          },
          {
            label: t('library.deleteFolder', { name: folder.name }),
            disabled: busy,
            onSelect: () => {
              setConfirm({ kind: 'libraryFolder', folder });
            },
          },
        );
      }
    } else if (item.kind === 'asset') {
      const asset = model.assetById.get(item.id);
      if (!asset) return backgroundItems();
      items.push({
        label: t('media.open'),
        onSelect: () => {
          openItem(item);
        },
      });
      if (canRename(item)) {
        items.push({
          label: t('library.renameAsset'),
          disabled: busy,
          onSelect: () => {
            setRenaming(item);
          },
        });
      }
      items.push(...(asset.usage === 'audio' ? soundItems(asset) : pictureItems(asset)));
    } else {
      const playlist = model.playlistById.get(item.id);
      if (!playlist) return backgroundItems();
      items.push({
        label: t('media.open'),
        onSelect: () => {
          openItem(item);
        },
      });
      if (isGm) {
        items.push(
          {
            label: t('playlists.playForAll'),
            disabled: playlist.tracks.length === 0,
            onSelect: () => {
              setPlayError(null);
              void playSharedPlaylist(playlist).catch((error: unknown) => {
                setPlayError(error);
              });
            },
          },
          {
            label: t('playlists.rename', { name: playlist.name }),
            disabled: busy,
            onSelect: () => {
              setRenaming(item);
            },
          },
          {
            label: t('playlists.delete', { name: playlist.name }),
            disabled: busy,
            onSelect: () => {
              setConfirm({ kind: 'playlist', playlist });
            },
          },
        );
      }
    }
    if (items.length > 0 && items.at(-1) !== 'separator') {
      items.push('separator', { label: t('context.expandAll'), onSelect: expandAll });
      items.push({ label: t('journal.collapseAll'), onSelect: collapseAll });
    }
    return items;
  };

  const confirmMessage = (target: Confirmation): string => {
    if (target.kind === 'libraryFolder') {
      return t('library.confirmDeleteFolder', {
        name: target.folder.name,
        count: model.libraryContents(target.folder.id).assets,
      });
    }
    if (target.kind === 'playlist') {
      return t('playlists.confirmDelete', { name: target.playlist.name });
    }
    const { item } = target;
    if (item.kind === 'entry') {
      return t('journal.confirmDeleteEntry', {
        title: model.entryById.get(item.id)?.title ?? '',
      });
    }
    const name = model.journalFolders.get(item.id)?.name ?? '';
    const { entries, folders } = model.journalContents(item.id);
    return entries === 0 && folders === 0
      ? t('journal.confirmDeleteEmptyFolder', { name })
      : t('journal.confirmDeleteFolder', { name, entries, folders });
  };

  const confirmDetails = (target: Confirmation): string | null => {
    if (target.kind !== 'libraryFolder') return null;
    const { folders } = model.libraryContents(target.folder.id);
    return folders > 0 ? t('library.confirmDeleteFolderDetails', { folders }) : null;
  };

  const toggleRow = (row: LibraryRow) => {
    if (row.kind === 'journalFolder') toggle(journalKey(row.folder.id));
    else if (row.kind === 'libraryFolder') toggle(libraryKey(row.folder.id));
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLUListElement>) => {
    // Navigation belongs to the tree itself (aria-activedescendant). A key pressed in an in-row
    // control — the actions menu, a rename input, a checkbox — is that control's to handle;
    // stealing Enter there opened the *selected* row and swallowed the button's click.
    if (event.target !== event.currentTarget) return;
    const row = rows[currentIndex];
    const go = (index: number) => {
      const target = rows[index];
      const item = target ? rowItem(target) : null;
      if (item) setSelected(item);
    };
    switch (event.key) {
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
        if (row?.kind !== 'journalFolder' && row?.kind !== 'libraryFolder') return;
        if (!row.expanded) toggleRow(row);
        else go(currentIndex + 1);
        break;
      case 'ArrowLeft': {
        if ((row?.kind === 'journalFolder' || row?.kind === 'libraryFolder') && row.expanded) {
          toggleRow(row);
          break;
        }
        if (row?.kind === 'journalFolder' && row.folder.parent) {
          setSelected({ kind: 'journalFolder', id: row.folder.parent });
        } else if (row?.kind === 'libraryFolder' && row.folder.parent) {
          setSelected({ kind: 'libraryFolder', id: row.folder.parent });
        } else if (row?.kind === 'entry' && row.entry.folder_id) {
          setSelected({ kind: 'journalFolder', id: row.entry.folder_id });
        } else if (row?.kind === 'asset') {
          const folder = model.assetById.get(row.asset.id)?.folder_id;
          if (folder) setSelected({ kind: 'libraryFolder', id: folder });
        }
        break;
      }
      case 'Enter':
      case ' ': {
        if (row?.kind === 'journalFolder' || row?.kind === 'libraryFolder') {
          setSelected(rowItem(row));
          toggleRow(row);
        } else {
          const item = row ? rowItem(row) : null;
          if (!item) return;
          setSelected(item);
          openItem(item);
        }
        break;
      }
      case 'F2':
        if (!current || currentIndex < 0 || !canRename(current) || busy) return;
        setRenaming(current);
        break;
      case 'Delete': {
        if (!isGm || busy || !current || currentIndex < 0) return;
        const target = confirmationFor(current);
        if (target) setConfirm(target);
        break;
      }
      default:
        return;
    }
    event.preventDefault();
  };

  const renderRow = (row: LibraryRow) => {
    const style = { '--depth': row.depth } as CSSProperties;
    if (row.kind === 'new') {
      const type = pending?.type ?? 'journalFolder';
      return (
        <li className="nav-row tree-row library-tree-row" role="none" style={style}>
          <span className="tree-chevron-space" />
          <Icon
            name={type === 'table' ? 'table' : type === 'playlist' ? 'playlist' : 'folder'}
            className="row-icon"
          />
          <NameInput
            initial=""
            label={
              type === 'playlist'
                ? t('playlists.new')
                : type === 'libraryFolder'
                  ? t('library.folderName')
                  : type === 'journalFolder'
                    ? t('journal.folderName')
                    : t('journal.entryName')
            }
            onDone={finishCreate}
          />
        </li>
      );
    }
    const item = rowItem(row);
    if (!item) return null;
    const key = itemKey(item);
    const isSelected = sameItem(item, current);
    const isRenaming = sameItem(item, renaming);
    if (row.kind === 'journalFolder' || row.kind === 'libraryFolder') {
      const folder = row.folder;
      const target: FolderTarget =
        row.kind === 'journalFolder'
          ? { kind: 'journal', id: folder.id }
          : { kind: 'library', id: folder.id };
      const dropKeyHere = folderKey(target);
      const draggable = isGm && !busy && !isRenaming;
      return (
        <li
          id={`${treeId}-${key}`}
          role="treeitem"
          aria-level={row.depth + 1}
          aria-expanded={row.expanded}
          aria-selected={isSelected}
          className="nav-row tree-row library-tree-row"
          style={style}
          data-drop={dropKey === dropKeyHere}
          draggable={draggable}
          onDragStart={(event) => {
            if (draggable) startDrag(event, item);
          }}
          onDragEnd={finishDrag}
          onDragOver={(event) => {
            onRowDragOver(event, target, dropKeyHere);
          }}
          onDragLeave={(event) => {
            onDragLeave(event, dropKeyHere);
          }}
          onDrop={(event) => {
            onRowDrop(event, target);
          }}
          onClick={() => {
            setSelected(item);
            toggleRow(row);
            if (row.kind === 'libraryFolder') openItem(item);
          }}
          onDoubleClick={() => {
            if (canRename(item) && !busy) setRenaming(item);
          }}
          onContextMenu={(event) => {
            openMenu(event, item);
          }}
        >
          <Icon name="chevron" className="tree-chevron" />
          <Icon name={row.expanded ? 'folderOpen' : 'folder'} className="row-icon" />
          {isRenaming ? (
            <NameInput
              initial={folder.name}
              label={t('journal.rename')}
              onDone={(name) => {
                finishRename(item, name);
              }}
            />
          ) : (
            <span className="row-name" title={folder.name}>
              {folder.name}
            </span>
          )}
        </li>
      );
    }
    if (row.kind === 'entry') {
      const entry = row.entry;
      const target: FolderTarget | null = entry.folder_id
        ? { kind: 'journal', id: entry.folder_id }
        : null;
      const draggable = isGm && !busy && !isRenaming;
      return (
        <li
          id={`${treeId}-${key}`}
          role="treeitem"
          aria-level={row.depth + 1}
          aria-selected={isSelected}
          aria-current={entry.id === openId}
          className="nav-row tree-row library-tree-row"
          style={style}
          data-drop={dropKey === key}
          draggable={draggable}
          onDragStart={(event) => {
            if (draggable) startDrag(event, item);
          }}
          onDragEnd={finishDrag}
          onDragOver={(event) => {
            onRowDragOver(event, target, key);
          }}
          onDragLeave={(event) => {
            onDragLeave(event, key);
          }}
          onDrop={(event) => {
            onRowDrop(event, target);
          }}
          onClick={() => {
            setSelected(item);
            openItem(item);
          }}
          onDoubleClick={() => {
            if (canRename(item) && !busy) setRenaming(item);
          }}
          onContextMenu={(event) => {
            openMenu(event, item);
          }}
        >
          <span className="tree-chevron-space" />
          <span title={t(`journal.visibility.${entry.visibility}`)} className="row-icon-box">
            <Icon
              name={entry.visibility === 'gm' ? 'lock' : entry.kind === 'table' ? 'table' : 'book'}
              className="row-icon"
            />
          </span>
          {isRenaming ? (
            <NameInput
              initial={entry.title}
              label={t('journal.rename')}
              onDone={(name) => {
                finishRename(item, name);
              }}
            />
          ) : (
            <span className="row-name" title={entry.title}>
              {entry.title}
            </span>
          )}
        </li>
      );
    }
    if (row.kind === 'asset') {
      const asset = row.asset;
      const target: FolderTarget | null = asset.folder_id
        ? { kind: 'library', id: asset.folder_id }
        : null;
      const draggable = isGm && !busy && !isRenaming;
      return (
        <li
          id={`${treeId}-${key}`}
          role="treeitem"
          aria-level={row.depth + 1}
          aria-selected={isSelected}
          className="nav-row tree-row library-tree-row"
          style={style}
          data-drop={dropKey === key}
          draggable={draggable}
          onDragStart={(event) => {
            if (draggable) startDrag(event, item);
          }}
          onDragEnd={finishDrag}
          onDragOver={(event) => {
            onRowDragOver(event, target, key);
          }}
          onDragLeave={(event) => {
            onDragLeave(event, key);
          }}
          onDrop={(event) => {
            onRowDrop(event, target);
          }}
          onContextMenu={(event) => {
            openMenu(event, item);
          }}
          onClick={() => {
            setSelected(item);
            openItem(item);
          }}
          onDoubleClick={() => {
            if (canRename(item) && !busy) setRenaming(item);
          }}
        >
          <span className="tree-chevron-space" />
          {asset.usage !== 'audio' ? (
            <img
              className="library-tree-thumb"
              src={assetUrl(asset.thumbnail_sha256)}
              alt=""
              onMouseEnter={(event) => {
                if (previewTimer.current) clearTimeout(previewTimer.current);
                const rect = event.currentTarget.getBoundingClientRect();
                const width = Math.min(320, window.innerWidth - 16);
                setHoverPreview({
                  asset,
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
          {isRenaming ? (
            <NameInput
              initial={asset.original_name ?? ''}
              label={t('library.renameAsset')}
              onDone={(name) => {
                finishRename(item, name);
              }}
            />
          ) : (
            <span className="row-name" title={asset.original_name ?? undefined}>
              {assetLabel(asset)}
            </span>
          )}
          <span
            className="library-row-actions"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <IconButton
              icon="more"
              label={t('library.assetActions', { name: assetLabel(asset) })}
              onClick={(event) => {
                const rect = event.currentTarget.getBoundingClientRect();
                setMenu({ x: rect.right, y: rect.bottom, item });
              }}
            />
          </span>
        </li>
      );
    }
    const playlist = row.playlist;
    return (
      <li
        id={`${treeId}-${key}`}
        role="treeitem"
        aria-level={row.depth + 1}
        aria-selected={isSelected}
        className="nav-row tree-row library-tree-row"
        style={style}
        data-drop={dropKey === key}
        onDragOver={(event) => {
          onPlaylistDragOver(event, playlist);
        }}
        onDragLeave={(event) => {
          onDragLeave(event, key);
        }}
        onDrop={(event) => {
          onPlaylistDrop(event, playlist);
        }}
        onClick={() => {
          setSelected(item);
          openItem(item);
        }}
        onDoubleClick={() => {
          if (canRename(item) && !busy) setRenaming(item);
        }}
        onContextMenu={(event) => {
          openMenu(event, item);
        }}
      >
        <span className="tree-chevron-space" />
        <Icon name="playlist" className="row-icon" />
        {isRenaming ? (
          <NameInput
            initial={playlist.name}
            label={t('playlists.rename', { name: playlist.name })}
            onDone={(name) => {
              finishRename(item, name);
            }}
          />
        ) : (
          <span className="row-name" title={playlist.name}>
            {playlist.name}
          </span>
        )}
        <span className="muted playlist-track-count">
          {t('playlists.tracksCount', { count: playlist.tracks.length })}
        </span>
      </li>
    );
  };

  const activeRow = currentIndex >= 0 ? rows[currentIndex] : undefined;
  const activeDescendant = activeRow ? `${treeId}-${rowKey(activeRow)}` : undefined;
  const details = confirm ? confirmDetails(confirm) : null;

  return (
    <section className="library-explorer" aria-label={t('library.tree')}>
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
        {isGm && (
          <>
            <IconButton
              icon="filePlus"
              label={t('journal.add')}
              onClick={() => {
                startCreate('entry', journalTarget);
              }}
              disabled={busy}
            />
            <IconButton
              icon="table"
              label={t('journal.addTable')}
              onClick={() => {
                startCreate('table', journalTarget);
              }}
              disabled={busy}
            />
            <IconButton
              icon="folderPlus"
              label={t('journal.addFolder')}
              onClick={() => {
                startCreate('journalFolder', journalTarget);
              }}
              disabled={busy}
            />
            <IconButton
              icon="folderPlus"
              label={t('library.addFolder')}
              onClick={() => {
                startLibraryFolder(libraryTarget.library, libraryTarget.parent);
              }}
              disabled={busy}
            />
            <IconButton
              icon="playlist"
              label={t('playlists.new')}
              onClick={startPlaylist}
              disabled={busy || pending !== null}
            />
          </>
        )}
        {canImport && (
          <>
            <IconButton
              icon="image"
              label={t('images.import')}
              onClick={() => {
                importImages.mutate();
              }}
              disabled={importing}
            />
            <IconButton
              icon="music"
              label={t('audio.import')}
              onClick={() => {
                importAudio.mutate();
              }}
              disabled={importing}
            />
          </>
        )}
        <IconButton icon="collapse" label={t('journal.collapseAll')} onClick={collapseAll} />
      </div>
      <div
        className="library-tree-root"
        data-drop={dropKey === 'root'}
        onDragOver={(event) => {
          onRowDragOver(event, null, 'root');
        }}
        onDragLeave={(event) => {
          onDragLeave(event, 'root');
        }}
        onDrop={(event) => {
          onRowDrop(event, null);
        }}
        onContextMenu={(event) => {
          if ((event.target as HTMLElement).closest('.nav-row')) return;
          openMenu(event, null);
        }}
      >
        <VirtualList<LibraryRow>
          items={rows}
          rowHeight={NAV_ROW_HEIGHT}
          getKey={rowKey}
          renderRow={renderRow}
          scrollToIndex={pendingIndex >= 0 ? pendingIndex : currentIndex}
          label={t('library.tree')}
          listProps={{
            ref: treeRef,
            role: 'tree',
            tabIndex: 0,
            'aria-activedescendant': activeDescendant,
            onKeyDown,
          }}
        />
        {rows.length === 0 && (
          <p className="nav-empty">{query.trim() ? t('picker.nothing') : t('library.empty')}</p>
        )}
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={menu.item ? menuName(menu.item) : t('library.tree')}
          items={menuItems(menu.item)}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
      {confirm && (
        <div
          className="confirm-bar"
          role="alertdialog"
          aria-label={t('journal.delete')}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              setConfirm(null);
              treeRef.current?.focus();
            }
          }}
        >
          <p>{confirmMessage(confirm)}</p>
          {details !== null && <p>{details}</p>}
          <div className="confirm-actions">
            <button
              type="button"
              className="confirm-delete"
              disabled={busy}
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
      {moving && (
        <form
          className="confirm-bar"
          role="dialog"
          aria-label={t('context.moveTo')}
          onKeyDown={(event) => {
            if (event.key === 'Escape') setMoving(null);
          }}
          onSubmit={(event) => {
            event.preventDefault();
            if (!busy) {
              const target: FolderTarget | null = moveFolder
                ? { kind: 'journal', id: moveFolder }
                : null;
              moveItem(moving, target);
              setMoving(null);
            }
          }}
        >
          <label>
            {t('journal.destination')}
            <select
              value={moveFolder}
              onChange={(event) => {
                setMoveFolder(event.target.value);
              }}
              disabled={busy}
            >
              <option value="">{t('journal.root')}</option>
              {[...model.journalFolders.values()]
                .filter(
                  (folder) =>
                    moving.kind !== 'journalFolder' || !model.isInsideJournal(folder.id, moving.id),
                )
                .map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {model
                      .journalPath(folder.id)
                      .map((id) => model.journalFolders.get(id)?.name)
                      .join(' / ')}
                  </option>
                ))}
            </select>
          </label>
          <button type="submit" disabled={busy}>
            {t('context.move')}
          </button>
          <button
            type="button"
            onClick={() => {
              setMoving(null);
            }}
          >
            {t('journal.cancel')}
          </button>
        </form>
      )}
      <ErrorText error={submit.error ?? importImages.error ?? importAudio.error ?? playError} />
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
    </section>
  );
}
