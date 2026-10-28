import { csvToTable, tableToCsv } from './table';
import type {
  Intent,
  JournalEntryDto,
  JournalFile,
  JournalFolderDto,
  JournalImport,
  JournalTable,
} from '@/shared/ipc/client';

/** Longest entry title or folder name the core accepts. */
const MAX_NAME = 200;

// ---------- Export ----------

/** A title as a file name valid on every system (ADR-0026): no separators or reserved names. */
export function fileSafe(name: string): string {
  let safe = name
    // eslint-disable-next-line no-control-regex -- control characters are exactly what is removed
    .replace(/[\u0000-\u001f\u007f/\\:*?"<>|]/g, '_')
    .trim()
    .replace(/[. ]+$/, '');
  if (/^(con|prn|aux|nul|com\d|lpt\d)(\..*)?$/i.test(safe)) safe = `_${safe}`;
  return Array.from(safe).slice(0, 150).join('') || '_';
}

/** `name` unless taken (case-insensitively) in `taken`; then `name (2)`, `name (3)`, … */
function unique(name: string, ext: string, taken: Set<string>): string {
  for (let n = 1; ; n++) {
    const candidate = n === 1 ? `${name}${ext}` : `${name} (${n})${ext}`;
    if (!taken.has(candidate.toLowerCase())) {
      taken.add(candidate.toLowerCase());
      return candidate;
    }
  }
}

/** The path from the folder `from` to the file `to`, as a Markdown link target. */
export function relativeLink(from: readonly string[], to: readonly string[]): string {
  let common = 0;
  while (common < from.length && common < to.length - 1 && from[common] === to[common]) common++;
  const up = from.slice(common).map(() => '..');
  return [...up, ...to.slice(common)].map(encodeURIComponent).join('/');
}

const JOURNAL_LINK = /\]\(journal:([0-9a-f-]{36})\)/g;

/**
 * The journal as files: folders as folders, texts as `.md`, tables as `.csv`; links to other
 * entries become relative paths, as Obsidian and other Markdown tools write them.
 */
export function planExport(
  folders: readonly JournalFolderDto[],
  entries: readonly JournalEntryDto[],
): JournalFile[] {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const folderPaths = new Map<string | null, string[]>([[null, []]]);
  const takenIn = new Map<string, Set<string>>();
  const taken = (path: readonly string[]) => {
    const key = path.join('/');
    let set = takenIn.get(key);
    if (!set) {
      set = new Set();
      takenIn.set(key, set);
    }
    return set;
  };
  const pathOf = (id: string | null, depth = 0): string[] => {
    const known = folderPaths.get(id);
    if (known) return known;
    const folder = id === null ? undefined : byId.get(id);
    if (!folder || depth > 32) return [];
    const parent = pathOf(folder.parent, depth + 1);
    const path = [...parent, unique(fileSafe(folder.name), '', taken(parent))];
    folderPaths.set(id, path);
    return path;
  };
  // Folders and entries in a stable order, so the same journal gives the same names.
  for (const f of [...folders].sort((a, b) => a.sort_order - b.sort_order)) pathOf(f.id);
  const sorted = [...entries].sort((a, b) => a.sort_order - b.sort_order);
  const filePaths = new Map<string, string[]>();
  for (const e of sorted) {
    const dir = pathOf(e.folder_id);
    const name = unique(fileSafe(e.title), e.table ? '.csv' : '.md', taken(dir));
    filePaths.set(e.id, [...dir, name]);
  }
  return sorted.map((e) => {
    const path = filePaths.get(e.id) ?? [];
    if (e.table) return { path, text: tableToCsv(e.table) };
    const dir = path.slice(0, -1);
    const text = e.markdown.replace(JOURNAL_LINK, (whole, id: string) => {
      const target = filePaths.get(id);
      return target ? `](${relativeLink(dir, target)})` : whole;
    });
    return { path, text };
  });
}

// ---------- Import ----------

/** What an import becomes: intents in an order the core accepts (parents first). */
export interface ImportPlan {
  intents: Intent[];
  folders: number;
  entries: number;
}

const title = (name: string) =>
  Array.from(name.replace(/\.(md|markdown|csv)$/i, '').trim())
    .slice(0, MAX_NAME)
    .join('') || '—';

/** `a/./b/../c` → `a/c`; `null` if it climbs out of the imported folder. */
function normalize(parts: readonly string[]): string[] | null {
  const out: string[] = [];
  for (const p of parts) {
    if (p === '' || p === '.') continue;
    if (p === '..') {
      if (out.length === 0) return null;
      out.pop();
    } else out.push(p);
  }
  return out;
}

function decode(part: string): string {
  try {
    return decodeURIComponent(part);
  } catch {
    return part;
  }
}

/**
 * Plans an import (ADR-0026): the picked folder becomes a journal folder named `imported.name`
 * (`(2)`… if the root already has one), its subfolders nested folders, `.md` files texts and `.csv`
 * files tables, all GM-only. Relative links to imported files and wiki links `[[Title]]`,
 * `[[Title|text]]` become `journal:` links.
 */
export function planImport(
  imported: JournalImport,
  rootNames: readonly string[],
  newId: () => string,
): ImportPlan {
  const intents: Intent[] = [];
  const takenRoot = new Set(rootNames.map((n) => n.toLowerCase()));
  const topName = unique(title(imported.name).replace(/\s+\(\d+\)$/, ''), '', takenRoot);
  const topId = newId();
  intents.push({ kind: 'create_journal_folder', id: topId, parent: null, name: topName });
  const folderIds = new Map<string, string>([['', topId]]);
  // Sibling folders differ in more than case (the core's rule, ADR-0021).
  const takenNames = new Map<string, Set<string>>();
  const folderOf = (dirs: readonly string[]): string => {
    const key = dirs.join('/');
    const known = folderIds.get(key);
    if (known) return known;
    const parent = folderOf(dirs.slice(0, -1));
    const siblings = takenNames.get(parent) ?? new Set<string>();
    takenNames.set(parent, siblings);
    const id = newId();
    folderIds.set(key, id);
    intents.push({
      kind: 'create_journal_folder',
      id,
      parent,
      name: unique(title(dirs[dirs.length - 1] ?? ''), '', siblings),
    });
    return id;
  };

  const files = imported.files.map((f) => ({
    ...f,
    id: newId(),
    title: title(f.path[f.path.length - 1] ?? ''),
    key: f.path.join('/').toLowerCase(),
  }));
  const byPath = new Map(files.map((f) => [f.key, f]));
  const byTitle = new Map<string, (typeof files)[number]>();
  for (const f of files) {
    const k = f.title.toLowerCase();
    if (!byTitle.has(k)) byTitle.set(k, f);
  }

  for (const f of files) {
    const dirs = f.path.slice(0, -1);
    const folder = folderOf(dirs);
    const isTable = /\.csv$/i.test(f.path[f.path.length - 1] ?? '');
    let table: JournalTable | null = null;
    let markdown = '';
    if (isTable) table = csvToTable(f.text);
    else {
      markdown = f.text
        // [[Title]], [[Title|text]], [[Title#heading]]
        .replace(
          /\[\[([^\]|#\n]+)(?:#[^\]|\n]*)?(?:\|([^\]\n]+))?\]\]/g,
          (_, name: string, text?: string) => {
            const target = byTitle.get(name.trim().toLowerCase());
            const shown = (text ?? name).trim().replace(/[[\]\\]/g, '');
            return target ? `[${shown}](journal:${target.id})` : shown;
          },
        )
        // [text](relative/path.md)
        .replace(/\]\(([^()\s]+\.(?:md|markdown|csv))(#[^()\s]*)?\)/gi, (whole, href: string) => {
          if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return whole;
          const parts = normalize([...dirs, ...href.split('/').map(decode)]);
          const target = parts && byPath.get(parts.join('/').toLowerCase());
          return target ? `](journal:${target.id})` : whole;
        });
    }
    intents.push({
      kind: 'create_journal_entry',
      id: f.id,
      folder_id: folder,
      title: f.title,
      markdown,
      visibility: 'gm',
      players_can_edit: false,
      table,
    });
  }
  return { intents, folders: folderIds.size, entries: files.length };
}
