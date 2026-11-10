import type {
  AssetDto,
  JournalEntryMeta,
  JournalFolderDto,
  Library,
  LibraryFolderDto,
  PlaylistDto,
} from '@/shared/ipc/client';

/**
 * A folder in one of the two namespaces the navigator shows: journal folders and library folders
 * live in separate tables, so they are keyed by both kind and id.
 */
export type FolderTarget = { kind: 'journal'; id: string } | { kind: 'library'; id: string };

/** A name input shown inside a folder while an entry, table or folder is being created. */
export type PendingCreate =
  | { type: 'entry' | 'table' | 'journalFolder'; parent: string | null }
  | { type: 'libraryFolder'; parent: string | null; library: Library }
  | { type: 'playlist' };

/** One visible line of the navigator: the merged journal and media tree in display order. */
export type LibraryRow =
  | {
      kind: 'journalFolder';
      folder: JournalFolderDto;
      depth: number;
      expanded: boolean;
      empty: boolean;
    }
  | {
      kind: 'libraryFolder';
      folder: LibraryFolderDto;
      depth: number;
      expanded: boolean;
      empty: boolean;
    }
  | { kind: 'entry'; entry: JournalEntryMeta; depth: number }
  | { kind: 'asset'; asset: AssetDto; depth: number }
  | { kind: 'playlist'; playlist: PlaylistDto; depth: number }
  /** The name input of the item being created. */
  | { kind: 'new'; depth: number };

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
const byName = (a: string, b: string) => collator.compare(a, b);

/** Key of a folder's expansion state (both namespaces share one set). */
export const folderKey = (target: FolderTarget) => `${target.kind}:${target.id}`;

/** Which library an asset belongs to: audio by usage, everything else is a picture. */
export const assetLibrary = (asset: AssetDto): Library =>
  asset.usage === 'audio' ? 'audio' : 'images';

/** The name a row shows: the original file name, or a short hash when the file was unnamed. */
export const assetLabel = (asset: AssetDto) => asset.original_name ?? asset.sha256.slice(0, 8);

const sortOrder = (folder: JournalFolderDto | LibraryFolderDto) =>
  'sort_order' in folder ? folder.sort_order : 0;

/**
 * The campaign navigator as one explorer: journal folders and entries, library folders, pictures,
 * sounds and playlists. Children are grouped per parent and sorted folders-first, then files by
 * name (numbers in numeric order, as explorers do). The model is pure: the component supplies the
 * expansion set, the pending create row and the search query.
 */
export class LibraryTreeModel {
  readonly journalFolders = new Map<string, JournalFolderDto>();
  readonly libraryFolders = new Map<string, LibraryFolderDto>();
  readonly entryById = new Map<string, JournalEntryMeta>();
  readonly assetById = new Map<string, AssetDto>();
  readonly playlistById = new Map<string, PlaylistDto>();
  readonly playlists: readonly PlaylistDto[];
  private readonly journalChildren = new Map<string | null, JournalFolderDto[]>();
  private readonly entriesByFolder = new Map<string | null, JournalEntryMeta[]>();
  private readonly libraryChildren = new Map<string | null, LibraryFolderDto[]>();
  private readonly assetsByFolder = new Map<string | null, AssetDto[]>();

  constructor(view: {
    entries: readonly JournalEntryMeta[];
    journalFolders: readonly JournalFolderDto[];
    libraryFolders: readonly LibraryFolderDto[];
    assets: readonly AssetDto[];
    playlists: readonly PlaylistDto[];
  }) {
    for (const folder of view.journalFolders) this.journalFolders.set(folder.id, folder);
    for (const folder of view.journalFolders) {
      // A dangling parent (should not happen) shows the folder at the root.
      const parent =
        folder.parent !== null && this.journalFolders.has(folder.parent) ? folder.parent : null;
      push(this.journalChildren, parent, folder);
    }
    for (const folder of view.libraryFolders) this.libraryFolders.set(folder.id, folder);
    for (const folder of view.libraryFolders) {
      const parent =
        folder.parent !== null && this.libraryFolders.has(folder.parent) ? folder.parent : null;
      push(this.libraryChildren, parent, folder);
    }
    for (const entry of view.entries) {
      this.entryById.set(entry.id, entry);
      const folder =
        entry.folder_id !== null && this.journalFolders.has(entry.folder_id)
          ? entry.folder_id
          : null;
      push(this.entriesByFolder, folder, entry);
    }
    for (const asset of view.assets) {
      this.assetById.set(asset.id, asset);
      const parent = asset.folder_id ?? null;
      const folder = parent !== null && this.libraryFolders.has(parent) ? parent : null;
      push(this.assetsByFolder, folder, asset);
    }
    for (const playlist of view.playlists) this.playlistById.set(playlist.id, playlist);
    this.playlists = [...view.playlists].sort((a, b) => byName(a.name, b.name));

    for (const list of this.journalChildren.values()) {
      list.sort((a, b) => byName(a.name, b.name) || a.sort_order - b.sort_order);
    }
    for (const list of this.entriesByFolder.values()) {
      list.sort((a, b) => byName(a.title, b.title) || a.sort_order - b.sort_order);
    }
    for (const list of this.libraryChildren.values()) {
      list.sort((a, b) => byName(a.name, b.name));
    }
    for (const list of this.assetsByFolder.values()) {
      list.sort((a, b) => byName(assetLabel(a), assetLabel(b)));
    }
  }

  journalSubfolders(parent: string | null = null): readonly JournalFolderDto[] {
    return this.journalChildren.get(parent) ?? [];
  }

  entries(folder: string | null = null): readonly JournalEntryMeta[] {
    return this.entriesByFolder.get(folder) ?? [];
  }

  librarySubfolders(parent: string | null = null): readonly LibraryFolderDto[] {
    return this.libraryChildren.get(parent) ?? [];
  }

  assets(folder: string | null = null): readonly AssetDto[] {
    return this.assetsByFolder.get(folder) ?? [];
  }

  /** The library a library folder belongs to, or `null` when the folder is gone. */
  libraryOf(folder: string): Library | null {
    return this.libraryFolders.get(folder)?.library ?? null;
  }

  /**
   * Visible rows: the root's folders then files, children of expanded folders, and the pending
   * create row. A non-empty query keeps only matching files and the folders holding them.
   */
  rows(
    expanded: ReadonlySet<string>,
    pending: PendingCreate | null = null,
    query = '',
  ): LibraryRow[] {
    const search = query.trim().toLocaleLowerCase();
    const matches = (name: string) =>
      search.length === 0 || name.toLocaleLowerCase().includes(search);
    const journalMatches = new Map<string, boolean>();
    const libraryMatches = new Map<string, boolean>();

    const journalFolderMatches = (id: string): boolean => {
      const cached = journalMatches.get(id);
      if (cached !== undefined) return cached;
      journalMatches.set(id, false); // cycle guard for corrupt parent chains
      const folder = this.journalFolders.get(id);
      const result =
        folder !== undefined &&
        (matches(folder.name) ||
          this.journalSubfolders(id).some((child) => journalFolderMatches(child.id)) ||
          this.entries(id).some((entry) => matches(entry.title)));
      journalMatches.set(id, result);
      return result;
    };
    const libraryFolderMatches = (id: string): boolean => {
      const cached = libraryMatches.get(id);
      if (cached !== undefined) return cached;
      libraryMatches.set(id, false);
      const folder = this.libraryFolders.get(id);
      const result =
        folder !== undefined &&
        (matches(folder.name) ||
          this.librarySubfolders(id).some((child) => libraryFolderMatches(child.id)) ||
          this.assets(id).some((asset) => matches(assetLabel(asset))));
      libraryMatches.set(id, result);
      return result;
    };

    const rows: LibraryRow[] = [];
    const pushFiles = (parent: string | null, depth: number) => {
      const files: { name: string; row: LibraryRow }[] = [];
      for (const entry of this.entries(parent)) {
        if (!matches(entry.title)) continue;
        files.push({ name: entry.title, row: { kind: 'entry', entry, depth } });
      }
      for (const asset of this.assets(parent)) {
        if (!matches(assetLabel(asset))) continue;
        files.push({ name: assetLabel(asset), row: { kind: 'asset', asset, depth } });
      }
      // Playlists have no navigator nesting today: they stay at the root.
      if (parent === null) {
        for (const playlist of this.playlists) {
          if (!matches(playlist.name)) continue;
          files.push({ name: playlist.name, row: { kind: 'playlist', playlist, depth } });
        }
      }
      files.sort((a, b) => byName(a.name, b.name));
      for (const file of files) rows.push(file.row);
    };

    const walkJournal = (folder: JournalFolderDto, depth: number) => {
      const open = search.length > 0 || expanded.has(folderKey({ kind: 'journal', id: folder.id }));
      const empty =
        this.journalSubfolders(folder.id).length === 0 && this.entries(folder.id).length === 0;
      rows.push({ kind: 'journalFolder', folder, depth, expanded: open, empty });
      if (!open) return;
      if (
        pending !== null &&
        pending.type !== 'libraryFolder' &&
        pending.type !== 'playlist' &&
        pending.parent === folder.id
      ) {
        rows.push({ kind: 'new', depth: depth + 1 });
      }
      for (const child of this.journalSubfolders(folder.id)) {
        if (search.length > 0 && !journalFolderMatches(child.id)) continue;
        walkJournal(child, depth + 1);
      }
      pushFiles(folder.id, depth + 1);
    };

    const walkLibrary = (folder: LibraryFolderDto, depth: number) => {
      const open = search.length > 0 || expanded.has(folderKey({ kind: 'library', id: folder.id }));
      const empty =
        this.librarySubfolders(folder.id).length === 0 && this.assets(folder.id).length === 0;
      rows.push({ kind: 'libraryFolder', folder, depth, expanded: open, empty });
      if (!open) return;
      if (pending !== null && pending.type === 'libraryFolder' && pending.parent === folder.id) {
        rows.push({ kind: 'new', depth: depth + 1 });
      }
      for (const child of this.librarySubfolders(folder.id)) {
        if (search.length > 0 && !libraryFolderMatches(child.id)) continue;
        walkLibrary(child, depth + 1);
      }
      pushFiles(folder.id, depth + 1);
    };

    if (pending !== null && (pending.type === 'playlist' || pending.parent === null)) {
      rows.push({ kind: 'new', depth: 0 });
    }
    const rootFolders: (
      { kind: 'journal'; folder: JournalFolderDto } | { kind: 'library'; folder: LibraryFolderDto }
    )[] = [
      ...this.journalSubfolders().map((folder) => ({ kind: 'journal' as const, folder })),
      ...this.librarySubfolders().map((folder) => ({ kind: 'library' as const, folder })),
    ].sort(
      (a, b) => byName(a.folder.name, b.folder.name) || sortOrder(a.folder) - sortOrder(b.folder),
    );
    for (const item of rootFolders) {
      if (search.length > 0) {
        const visible =
          item.kind === 'journal'
            ? journalFolderMatches(item.folder.id)
            : libraryFolderMatches(item.folder.id);
        if (!visible) continue;
      }
      if (item.kind === 'journal') walkJournal(item.folder, 0);
      else walkLibrary(item.folder, 0);
    }
    pushFiles(null, 0);
    return rows;
  }

  /** Entries and folders inside a journal folder, at any depth (for the delete confirmation). */
  journalContents(folder: string): { entries: number; folders: number } {
    let entries = this.entries(folder).length;
    let folders = 0;
    for (const sub of this.journalSubfolders(folder)) {
      const inner = this.journalContents(sub.id);
      entries += inner.entries;
      folders += inner.folders + 1;
    }
    return { entries, folders };
  }

  /** Files and folders inside a library folder, at any depth (for the delete confirmation). */
  libraryContents(folder: string): { assets: number; folders: number } {
    let assets = this.assets(folder).length;
    let folders = 0;
    for (const sub of this.librarySubfolders(folder)) {
      const inner = this.libraryContents(sub.id);
      assets += inner.assets;
      folders += inner.folders + 1;
    }
    return { assets, folders };
  }

  /** Whether `folder` is `ancestor` or lies inside it (journal folders). */
  isInsideJournal(folder: string | null, ancestor: string): boolean {
    for (let f = folder; f !== null; f = this.journalFolders.get(f)?.parent ?? null) {
      if (f === ancestor) return true;
    }
    return false;
  }

  /** Whether `folder` is `ancestor` or lies inside it (library folders). */
  isInsideLibrary(folder: string | null, ancestor: string): boolean {
    for (let f = folder; f !== null; f = this.libraryFolders.get(f)?.parent ?? null) {
      if (f === ancestor) return true;
    }
    return false;
  }

  /** The folders from the root down to `folder` (inclusive), to reveal a row opened elsewhere. */
  journalPath(folder: string | null): string[] {
    const path: string[] = [];
    for (let f = folder; f !== null; f = this.journalFolders.get(f)?.parent ?? null) {
      if (path.includes(f)) break;
      path.unshift(f);
    }
    return path;
  }

  /** The folders from the root down to `folder` (inclusive), for library folders. */
  libraryPath(folder: string | null): string[] {
    const path: string[] = [];
    for (let f = folder; f !== null; f = this.libraryFolders.get(f)?.parent ?? null) {
      if (path.includes(f)) break;
      path.unshift(f);
    }
    return path;
  }
}

function push<K, V>(map: Map<K, V[]>, key: K, value: V) {
  const list = map.get(key);
  if (list) list.push(value);
  else map.set(key, [value]);
}

/**
 * What a player is shown (ADR-0021): entries open to players, and only the folders that hold such
 * an entry somewhere inside. The server filters the same way.
 */
export function playerView(
  entries: readonly JournalEntryMeta[],
  folders: readonly JournalFolderDto[],
): { entries: JournalEntryMeta[]; folders: JournalFolderDto[] } {
  const visible = entries.filter((e) => e.visibility === 'players');
  const byId = new Map(folders.map((f) => [f.id, f]));
  const keep = new Set<string>();
  for (const e of visible) {
    for (let f = e.folder_id; f !== null && !keep.has(f); f = byId.get(f)?.parent ?? null) {
      keep.add(f);
    }
  }
  return { entries: visible, folders: folders.filter((f) => keep.has(f.id)) };
}
