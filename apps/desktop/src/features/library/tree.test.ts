import { describe, expect, it } from 'vitest';
import { assetLabel, folderKey, LibraryTreeModel, playerView, type LibraryRow } from './tree';
import type {
  AssetDto,
  JournalEntryMeta,
  JournalFolderDto,
  LibraryFolderDto,
  PlaylistDto,
} from '@/shared/ipc/client';

const folder = (id: string, name: string, parent: string | null = null): JournalFolderDto => ({
  id,
  parent,
  name,
  sort_order: 0,
  version: 1,
});

const libraryFolder = (
  id: string,
  name: string,
  parent: string | null = null,
  library: 'images' | 'audio' = 'images',
): LibraryFolderDto => ({ id, library, parent, name, version: 1 });

const entry = (
  id: string,
  title: string,
  folder_id: string | null,
  visibility: 'gm' | 'players' = 'gm',
): JournalEntryMeta => ({
  id,
  folder_id,
  title,
  sort_order: 0,
  visibility,
  players_can_edit: false,
  author: null,
  version: 1,
});

const asset = (
  id: string,
  name: string | null,
  folder_id: string | null = null,
  usage: AssetDto['usage'] = 'token',
): AssetDto => ({
  id,
  usage,
  sha256: (id[0] ?? 'a').repeat(64),
  mime: usage === 'audio' ? 'audio/mpeg' : 'image/webp',
  size_bytes: 10,
  original_name: name,
  width: 0,
  height: 0,
  display_sha256: null,
  thumbnail_sha256: (id[0] ?? 'a').repeat(64),
  tiles: [],
  folder_id,
  version: 1,
});

const playlist = (id: string, name: string): PlaylistDto => ({
  id,
  name,
  folder_id: null,
  tracks: [],
  version: 1,
});

const journalFolders = [
  folder('places', 'Places'),
  folder('lore', 'Lore'),
  folder('p10', 'Place 10', 'places'),
  folder('p2', 'Place 2', 'places'),
];
const entries = [
  entry('root', 'Root note', null),
  entry('a', 'Crypt', 'p2', 'players'),
  entry('b', 'Chapel', 'p2'),
  entry('c', 'Gods', 'lore'),
];

const names = (rows: LibraryRow[]) =>
  rows.map((row) => {
    const pad = '  '.repeat(row.depth);
    switch (row.kind) {
      case 'journalFolder':
        return `${pad}[${row.expanded ? '-' : '+'}] ${row.folder.name}`;
      case 'libraryFolder':
        return `${pad}[${row.expanded ? '-' : '+'}] ${row.folder.name}`;
      case 'entry':
        return `${pad}${row.entry.title}`;
      case 'asset':
        return `${pad}${assetLabel(row.asset)}`;
      case 'playlist':
        return `${pad}[pl] ${row.playlist.name}`;
      case 'new':
        return `${pad}<new>`;
    }
  });

describe('LibraryTreeModel rows', () => {
  it('merges both namespaces: folders first by name, then files by name', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [libraryFolder('lf', 'Pictures')],
      assets: [asset('aa', 'Cover.png')],
      playlists: [playlist('pl', 'Themes')],
    });
    expect(names(model.rows(new Set()))).toEqual([
      '[+] Lore',
      '[+] Pictures',
      '[+] Places',
      'Cover.png',
      'Root note',
      '[pl] Themes',
    ]);
  });

  it('expands library folders to their own subfolders and files, never journal rows', () => {
    const model = new LibraryTreeModel({
      entries: [],
      journalFolders,
      libraryFolders: [
        libraryFolder('lf', 'Pictures'),
        libraryFolder('rooms', 'Rooms', 'lf'),
        libraryFolder('sounds', 'Sounds', null, 'audio'),
      ],
      assets: [
        asset('ia', 'Cover.png', 'lf'),
        asset('aa', 'Cavern.mp3', 'sounds', 'audio'),
        asset('ra', 'Outside.png'),
      ],
      playlists: [],
    });
    const expanded = new Set([
      folderKey({ kind: 'library', id: 'lf' }),
      folderKey({ kind: 'library', id: 'sounds' }),
    ]);
    expect(names(model.rows(expanded))).toEqual([
      '[+] Lore',
      '[-] Pictures',
      '  [+] Rooms',
      '  Cover.png',
      '[+] Places',
      '[-] Sounds',
      '  Cavern.mp3',
      'Outside.png',
    ]);
  });

  it('sorts journal folders before entries with numbers in numeric order', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [],
      assets: [],
      playlists: [],
    });
    expect(
      names(
        model.rows(
          new Set([
            folderKey({ kind: 'journal', id: 'places' }),
            folderKey({ kind: 'journal', id: 'p2' }),
          ]),
        ),
      ),
    ).toEqual([
      '[+] Lore',
      '[-] Places',
      '  [-] Place 2',
      '    Chapel',
      '    Crypt',
      '  [+] Place 10',
      'Root note',
    ]);
  });

  it('puts the name input first inside its folder and at the root', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [libraryFolder('lf', 'Pictures')],
      assets: [],
      playlists: [],
    });
    expect(
      names(
        model.rows(new Set([folderKey({ kind: 'journal', id: 'lore' })]), {
          type: 'journalFolder',
          parent: 'lore',
        }),
      ),
    ).toEqual(['[-] Lore', '  <new>', '  Gods', '[+] Pictures', '[+] Places', 'Root note']);
    expect(
      names(model.rows(new Set(), { type: 'libraryFolder', parent: null, library: 'audio' })),
    ).toEqual(['<new>', '[+] Lore', '[+] Pictures', '[+] Places', 'Root note']);
  });

  it('filters by name, keeps the folders on the way and expands them', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [libraryFolder('lf', 'Pictures')],
      assets: [asset('ia', 'Cover.png', 'lf')],
      playlists: [],
    });
    expect(names(model.rows(new Set(), null, 'place'))).toEqual([
      '[-] Places',
      '  [-] Place 2',
      '  [-] Place 10',
    ]);
    expect(names(model.rows(new Set(), null, 'cover'))).toEqual(['[-] Pictures', '  Cover.png']);
    expect(names(model.rows(new Set(), null, 'missing'))).toEqual([]);
  });

  it('counts everything inside a folder and knows the nesting', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [libraryFolder('lf', 'Pictures'), libraryFolder('rooms', 'Rooms', 'lf')],
      assets: [asset('ia', 'Cover.png', 'rooms'), asset('root', 'Outside.png')],
      playlists: [],
    });
    expect(model.journalContents('places')).toEqual({ entries: 2, folders: 2 });
    expect(model.journalContents('p10')).toEqual({ entries: 0, folders: 0 });
    expect(model.libraryContents('lf')).toEqual({ assets: 1, folders: 1 });
    expect(model.isInsideJournal('p2', 'places')).toBe(true);
    expect(model.isInsideJournal('places', 'p2')).toBe(false);
    expect(model.isInsideJournal(null, 'places')).toBe(false);
    expect(model.journalPath('p2')).toEqual(['places', 'p2']);
    expect(model.isInsideLibrary('rooms', 'lf')).toBe(true);
    expect(model.isInsideLibrary('lf', 'rooms')).toBe(false);
    expect(model.libraryPath('rooms')).toEqual(['lf', 'rooms']);
    expect(model.libraryOf('rooms')).toBe('images');
    expect(model.libraryOf('missing')).toBeNull();
  });

  it('marks empty folders', () => {
    const model = new LibraryTreeModel({
      entries,
      journalFolders,
      libraryFolders: [libraryFolder('lf', 'Pictures'), libraryFolder('empty', 'Empty', 'lf')],
      assets: [],
      playlists: [],
    });
    const rows = model.rows(new Set([folderKey({ kind: 'library', id: 'lf' })]));
    const empty = rows.find((row) => row.kind === 'libraryFolder' && row.folder.id === 'empty');
    expect(empty).toMatchObject({ empty: true });
    const pictures = rows.find((row) => row.kind === 'libraryFolder' && row.folder.id === 'lf');
    expect(pictures).toMatchObject({ empty: false });
  });

  it('drops a dangling folder.asset parent to the root', () => {
    const model = new LibraryTreeModel({
      entries: [entry('lost', 'Lost entry', 'gone')],
      journalFolders,
      libraryFolders: [],
      assets: [asset('lost-asset', 'Lost.png', 'gone')],
      playlists: [],
    });
    expect(names(model.rows(new Set()))).toEqual([
      '[+] Lore',
      '[+] Places',
      'Lost entry',
      'Lost.png',
    ]);
  });
});

describe('playerView', () => {
  it('keeps shared entries and only the folders on their way to the root', () => {
    const view = playerView(entries, journalFolders);
    expect(view.entries.map((e) => e.id)).toEqual(['a']);
    expect(view.folders.map((f) => f.id).sort()).toEqual(['p2', 'places']);
  });
});
