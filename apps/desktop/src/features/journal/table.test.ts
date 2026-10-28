import { describe, expect, it } from 'vitest';
import {
  cleanCell,
  columnLetter,
  csvToTable,
  deleteColumn,
  insertColumn,
  MAX_CELL,
  parseCsv,
  tableToCsv,
} from './table';

describe('journal tables', () => {
  it('names columns like a spreadsheet', () => {
    expect([0, 1, 25, 26, 27, 51, 52, 701, 702].map(columnLetter)).toEqual([
      'A',
      'B',
      'Z',
      'AA',
      'AB',
      'AZ',
      'BA',
      'ZZ',
      'AAA',
    ]);
  });

  it('round-trips CSV, quotes and line breaks included', () => {
    const table = {
      columns: [
        { name: 'Name', width: 140 },
        { name: 'Notes, misc', width: 200 },
      ],
      rows: [
        ['Sword', 'says "hi"'],
        ['Rope', 'two\nlines'],
        ['=A1', ''],
      ],
    };
    const csv = tableToCsv(table);
    // A byte-order mark: Excel then reads UTF-8.
    expect(csv.startsWith('﻿')).toBe(true);
    expect(csv).toContain('"says ""hi"""');
    const back = csvToTable(csv);
    expect(back.columns.map((c) => c.name)).toEqual(['Name', 'Notes, misc']);
    expect(back.rows).toEqual(table.rows);
  });

  it('reads Excel’s semicolon CSV and pads short rows', () => {
    expect(parseCsv('a;b;c\r\n1;2\r\n')).toEqual([
      ['a', 'b', 'c'],
      ['1', '2'],
    ]);
    expect(csvToTable('a;b;c\r\n1;2\r\n').rows).toEqual([['1', '2', '']]);
    expect(csvToTable('').columns).toHaveLength(1);
  });

  it('keeps cells within what the core accepts', () => {
    expect(cleanCell('a\u0007b\tc\nd')).toBe('ab\tc\nd');
    expect(Array.from(cleanCell('я'.repeat(MAX_CELL + 5)))).toHaveLength(MAX_CELL);
    const one = { columns: [{ name: 'A', width: 140 }], rows: [['x']] };
    expect(deleteColumn(one, 0)).toBe(one);
    expect(insertColumn(one, 0).rows).toEqual([['', 'x']]);
  });
});
