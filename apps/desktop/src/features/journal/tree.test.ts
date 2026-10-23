import { describe, expect, it } from 'vitest';
import { JournalTreeModel, playerView, type TreeRow } from './tree';
import type { JournalEntryMeta, JournalFolderDto } from '@/shared/ipc/client';

const folder = (id: string, name: string, parent: string | null = null): JournalFolderDto => ({
  id,
  parent,
  name,
  sort_order: 0,
  version: 1,
});

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

const folders = [
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

const names = (rows: TreeRow[]) =>
  rows.map((r) =>
    r.kind === 'folder'
      ? `${'  '.repeat(r.depth)}[${r.expanded ? '-' : '+'}] ${r.folder.name}`
      : r.kind === 'entry'
        ? `${'  '.repeat(r.depth)}${r.entry.title}`
        : `${'  '.repeat(r.depth)}<new ${r.type}>`,
  );

describe('JournalTreeModel', () => {
  const model = new JournalTreeModel(entries, folders);

  it('lists folders before entries, by name with numbers in order; collapsed hides children', () => {
    expect(names(model.rows(new Set()))).toEqual(['[+] Lore', '[+] Places', 'Root note']);
    expect(names(model.rows(new Set(['places', 'p2'])))).toEqual([
      '[+] Lore',
      '[-] Places',
      '  [-] Place 2',
      '    Chapel',
      '    Crypt',
      '  [+] Place 10',
      'Root note',
    ]);
  });

  it('puts the name input first inside its folder', () => {
    expect(names(model.rows(new Set(['lore']), { type: 'folder', parent: 'lore' }))).toEqual([
      '[-] Lore',
      '  <new folder>',
      '  Gods',
      '[+] Places',
      'Root note',
    ]);
  });

  it('counts everything inside a folder and knows the nesting', () => {
    expect(model.contents('places')).toEqual({ entries: 2, folders: 2 });
    expect(model.contents('p10')).toEqual({ entries: 0, folders: 0 });
    expect(model.isInside('p2', 'places')).toBe(true);
    expect(model.isInside('places', 'p2')).toBe(false);
    expect(model.isInside(null, 'places')).toBe(false);
    expect(model.path('p2')).toEqual(['places', 'p2']);
  });

  it('marks empty folders', () => {
    const rows = model.rows(new Set(['places']));
    const p10 = rows.find((r) => r.kind === 'folder' && r.folder.id === 'p10');
    expect(p10).toMatchObject({ empty: true });
  });
});

describe('playerView', () => {
  it('keeps shared entries and only the folders on their way to the root', () => {
    const view = playerView(entries, folders);
    expect(view.entries.map((e) => e.id)).toEqual(['a']);
    expect(view.folders.map((f) => f.id).sort()).toEqual(['p2', 'places']);
  });
});
