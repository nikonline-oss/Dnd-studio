import { describe, expect, it } from 'vitest';
import { fileSafe, planExport, planImport, relativeLink } from './exchange';
import type { Intent, JournalEntryDto, JournalFolderDto } from '@/shared/ipc/client';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

const folder = (n: number, name: string, parent: number | null): JournalFolderDto => ({
  id: id(n),
  parent: parent === null ? null : id(parent),
  name,
  sort_order: n,
  version: 1,
});

const entry = (
  n: number,
  title: string,
  folderN: number | null,
  markdown = '',
): JournalEntryDto => ({
  id: id(n),
  folder_id: folderN === null ? null : id(folderN),
  title,
  markdown,
  sort_order: n,
  visibility: 'gm',
  players_can_edit: false,
  author: null,
  table: null,
  version: 1,
});

describe('journal export', () => {
  it('makes names safe for every system', () => {
    expect(fileSafe('Who? / What: "x"')).toBe('Who_ _ What_ _x_');
    expect(fileSafe('con')).toBe('_con');
    expect(fileSafe('dots...')).toBe('dots');
    expect(fileSafe('   ')).toBe('_');
  });

  it('links between files with relative paths', () => {
    expect(relativeLink(['Lore'], ['Lore', 'Gods.md'])).toBe('Gods.md');
    expect(relativeLink(['A', 'B'], ['C', 'My file.md'])).toBe('../../C/My%20file.md');
    expect(relativeLink([], ['A', 'x.md'])).toBe('A/x.md');
  });

  it('writes folders, texts and tables, with links rewritten', () => {
    const files = planExport(
      [folder(1, 'Lore', null), folder(2, 'Gods', 1)],
      [
        entry(10, 'Start', null, `See [Zeus](journal:${id(11)}) and [gone](journal:${id(99)}).`),
        entry(11, 'Zeus', 2),
        entry(12, 'Zeus', 2),
        {
          ...entry(13, 'Prices', 1),
          table: { columns: [{ name: 'Item', width: 140 }], rows: [['Rope']] },
        },
      ],
    );
    expect(files.map((f) => f.path.join('/'))).toEqual([
      'Start.md',
      'Lore/Gods/Zeus.md',
      'Lore/Gods/Zeus (2).md',
      'Lore/Prices.csv',
    ]);
    expect(files[0]?.text).toBe(`See [Zeus](Lore/Gods/Zeus.md) and [gone](journal:${id(99)}).`);
    expect(files[3]?.text).toBe('﻿Item\r\nRope\r\n');
  });
});

describe('journal import', () => {
  it('creates folders and entries and resolves relative and wiki links', () => {
    let n = 100;
    const plan = planImport(
      {
        name: 'Vault',
        skipped: 0,
        files: [
          {
            path: ['Index.md'],
            text: 'Go to [[Zeus]], [[zeus|the god]], [[Nobody]], [rules](Rules/Combat%20rules.md).',
          },
          {
            path: ['Gods', 'Zeus.md'],
            text: 'Back to [index](../Index.md); [web](https://x.org/a.md)',
          },
          { path: ['Rules', 'Combat rules.md'], text: '' },
          { path: ['Rules', 'Loot.csv'], text: 'Item;Price\r\nRope;1 gp\r\n' },
        ],
      },
      ['Vault', 'vault (2)'],
      () => id(n++),
    );
    const kinds = plan.intents.map((i) => i.kind);
    expect(kinds.filter((k) => k === 'create_journal_folder')).toHaveLength(3);
    expect(plan).toMatchObject({ folders: 3, entries: 4 });
    // Parents come before what is inside them.
    const created = (i: Intent) => ('id' in i ? i.id : '');
    const top = plan.intents[0];
    expect(top).toMatchObject({ kind: 'create_journal_folder', parent: null, name: 'Vault (3)' });
    const entries = plan.intents.filter(
      (i): i is Extract<Intent, { kind: 'create_journal_entry' }> =>
        i.kind === 'create_journal_entry',
    );
    const byTitle = new Map(entries.map((e) => [e.title, e]));
    const zeus = byTitle.get('Zeus');
    const rules = byTitle.get('Combat rules');
    expect(byTitle.get('Index')?.markdown).toBe(
      `Go to [Zeus](journal:${created(zeus as Intent)}), [the god](journal:${created(zeus as Intent)}), Nobody, [rules](journal:${created(rules as Intent)}).`,
    );
    expect(zeus?.markdown).toBe(
      `Back to [index](journal:${created(byTitle.get('Index') as Intent)}); [web](https://x.org/a.md)`,
    );
    expect(byTitle.get('Loot')).toMatchObject({
      markdown: '',
      table: { columns: [{ name: 'Item' }, { name: 'Price' }], rows: [['Rope', '1 gp']] },
    });
    expect(entries.every((e) => e.visibility === 'gm')).toBe(true);
    for (const e of entries) {
      const parentAt = plan.intents.findIndex((i) => created(i) === e.folder_id);
      expect(parentAt).toBeGreaterThanOrEqual(0);
      expect(parentAt).toBeLessThan(plan.intents.indexOf(e));
    }
  });
});
