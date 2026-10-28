import type { JournalEntryMeta, JournalFolderDto } from '@/shared/ipc/client';

/** One visible line of the explorer: the tree flattened in display order (ADR-0021). */
export type TreeRow =
  | { kind: 'folder'; folder: JournalFolderDto; depth: number; expanded: boolean; empty: boolean }
  | { kind: 'entry'; entry: JournalEntryMeta; depth: number }
  /** The name input of an entry or folder being created. */
  | { kind: 'new'; type: Pending['type']; parent: string | null; depth: number };

export interface Pending {
  type: 'entry' | 'table' | 'folder';
  parent: string | null;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

/** Folders, then entries; each by name (as explorers do), then by `sort_order`. */
const byName = (a: string, b: string, x: number, y: number) => collator.compare(a, b) || x - y;

/** The journal as an explorer: children grouped by parent, sorted for display. */
export class JournalTreeModel {
  readonly folders = new Map<string, JournalFolderDto>();
  private readonly childFolders = new Map<string | null, JournalFolderDto[]>();
  private readonly childEntries = new Map<string | null, JournalEntryMeta[]>();

  constructor(entries: readonly JournalEntryMeta[], folders: readonly JournalFolderDto[]) {
    for (const f of folders) this.folders.set(f.id, f);
    for (const f of folders) {
      // A dangling parent (should not happen) shows the folder at the root.
      const parent = f.parent !== null && this.folders.has(f.parent) ? f.parent : null;
      push(this.childFolders, parent, f);
    }
    for (const e of entries) {
      const folder = e.folder_id !== null && this.folders.has(e.folder_id) ? e.folder_id : null;
      push(this.childEntries, folder, e);
    }
    for (const list of this.childFolders.values()) {
      list.sort((a, b) => byName(a.name, b.name, a.sort_order, b.sort_order));
    }
    for (const list of this.childEntries.values()) {
      list.sort((a, b) => byName(a.title, b.title, a.sort_order, b.sort_order));
    }
  }

  subfolders(parent: string | null): readonly JournalFolderDto[] {
    return this.childFolders.get(parent) ?? [];
  }

  entries(folder: string | null): readonly JournalEntryMeta[] {
    return this.childEntries.get(folder) ?? [];
  }

  /** Visible rows: children of expanded folders only, plus the pending name input. */
  rows(expanded: ReadonlySet<string>, pending: Pending | null = null): TreeRow[] {
    const rows: TreeRow[] = [];
    const walk = (parent: string | null, depth: number) => {
      if (pending && pending.parent === parent) {
        rows.push({ kind: 'new', type: pending.type, parent, depth });
      }
      for (const folder of this.subfolders(parent)) {
        const open = expanded.has(folder.id);
        const empty =
          this.subfolders(folder.id).length === 0 && this.entries(folder.id).length === 0;
        rows.push({ kind: 'folder', folder, depth, expanded: open, empty });
        if (open) walk(folder.id, depth + 1);
      }
      for (const entry of this.entries(parent)) rows.push({ kind: 'entry', entry, depth });
    };
    walk(null, 0);
    return rows;
  }

  /** Entries and folders inside `folder`, at any depth (for the delete confirmation). */
  contents(folder: string): { entries: number; folders: number } {
    let entries = this.entries(folder).length;
    let folders = 0;
    for (const sub of this.subfolders(folder)) {
      const inner = this.contents(sub.id);
      entries += inner.entries;
      folders += inner.folders + 1;
    }
    return { entries, folders };
  }

  /** Whether `folder` is `ancestor` or lies inside it. */
  isInside(folder: string | null, ancestor: string): boolean {
    for (let f = folder; f !== null; f = this.folders.get(f)?.parent ?? null) {
      if (f === ancestor) return true;
    }
    return false;
  }

  /** The folders from the root down to `folder` (inclusive). */
  path(folder: string | null): string[] {
    const path: string[] = [];
    for (let f = folder; f !== null; f = this.folders.get(f)?.parent ?? null) {
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
 * What a player is shown: entries open to players, and only the folders that hold such an entry
 * somewhere inside (ADR-0021). The server will filter the same way (stage 4).
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
