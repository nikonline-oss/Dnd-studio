import type { JournalTable } from '@/shared/ipc/client';

/** Limits of the core (`dnd-protocol`, ADR-0026). */
export const MAX_COLUMNS = 64;
export const MAX_ROWS = 5000;
export const MIN_WIDTH = 40;
export const MAX_WIDTH = 2000;
export const DEFAULT_WIDTH = 140;
export const MAX_CELL = 4000;

/** A fresh table: three named columns, three empty rows. */
export function newTable(): JournalTable {
  return {
    columns: ['A', 'B', 'C'].map((name) => ({ name, width: DEFAULT_WIDTH })),
    rows: Array.from({ length: 3 }, () => ['', '', '']),
  };
}

/** Spreadsheet column letters: A … Z, AA … (cell addresses for formulas later). */
export function columnLetter(index: number): string {
  let n = index + 1;
  let out = '';
  while (n > 0) {
    const rem = (n - 1) % 26;
    out = String.fromCharCode(65 + rem) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}

/** What the core accepts in a cell: no control characters but line breaks and tabs, 4000 at most. */
export function cleanCell(value: string): string {
  // Code points, as the core counts characters.
  // eslint-disable-next-line no-control-regex -- control characters are exactly what is removed
  return Array.from(value.replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g, ''))
    .slice(0, MAX_CELL)
    .join('');
}

const clampWidth = (w: number) => Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(w)));

export function setCell(
  table: JournalTable,
  row: number,
  col: number,
  value: string,
): JournalTable {
  return {
    ...table,
    rows: table.rows.map((r, i) =>
      i === row ? r.map((c, j) => (j === col ? cleanCell(value) : c)) : r,
    ),
  };
}

export function insertRow(table: JournalTable, at: number): JournalTable {
  if (table.rows.length >= MAX_ROWS) return table;
  const rows = [...table.rows];
  rows.splice(
    at,
    0,
    table.columns.map(() => ''),
  );
  return { ...table, rows };
}

export function deleteRow(table: JournalTable, at: number): JournalTable {
  return { ...table, rows: table.rows.filter((_, i) => i !== at) };
}

export function insertColumn(table: JournalTable, at: number): JournalTable {
  if (table.columns.length >= MAX_COLUMNS) return table;
  const columns = [...table.columns];
  columns.splice(at, 0, { name: columnLetter(table.columns.length), width: DEFAULT_WIDTH });
  return {
    columns,
    rows: table.rows.map((r) => {
      const row = [...r];
      row.splice(at, 0, '');
      return row;
    }),
  };
}

/** The last column stays: a table has at least one. */
export function deleteColumn(table: JournalTable, at: number): JournalTable {
  if (table.columns.length <= 1) return table;
  return {
    columns: table.columns.filter((_, i) => i !== at),
    rows: table.rows.map((r) => r.filter((_, i) => i !== at)),
  };
}

export function renameColumn(table: JournalTable, at: number, name: string): JournalTable {
  return {
    ...table,
    columns: table.columns.map((c, i) => (i === at ? { ...c, name } : c)),
  };
}

export function resizeColumn(table: JournalTable, at: number, width: number): JournalTable {
  return {
    ...table,
    columns: table.columns.map((c, i) => (i === at ? { ...c, width: clampWidth(width) } : c)),
  };
}

/**
 * Pastes a block of cells (as Excel and other spreadsheets copy them: tab-separated lines) with
 * its top-left corner at `row`/`col`; the table grows to fit, within the limits.
 */
export function pasteBlock(
  table: JournalTable,
  row: number,
  col: number,
  block: string[][],
): JournalTable {
  let next = table;
  const width = Math.max(0, ...block.map((r) => r.length));
  while (next.columns.length < Math.min(MAX_COLUMNS, col + width)) {
    next = insertColumn(next, next.columns.length);
  }
  while (next.rows.length < Math.min(MAX_ROWS, row + block.length)) {
    next = insertRow(next, next.rows.length);
  }
  const rows = next.rows.map((r, i) => {
    const line = block[i - row];
    if (!line) return r;
    return r.map((c, j) => (j >= col && j - col < line.length ? cleanCell(line[j - col] ?? c) : c));
  });
  return { ...next, rows };
}

/** Clipboard text → cells: tab-separated lines (a trailing line break is not a row). */
export function parseClipboard(text: string): string[][] {
  const lines = text.replace(/\r\n?/g, '\n').replace(/\n$/, '').split('\n');
  return lines.map((line) => line.split('\t'));
}

// ---------- CSV (RFC 4180), for import and export ----------

/** Excel opens UTF-8 CSV correctly only with a byte-order mark. */
const BOM = '﻿';

function csvField(value: string): string {
  return /[",\r\n]/.test(value) || /^\s|\s$/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** The table as CSV: a header line with the column names, then the rows. */
export function tableToCsv(table: JournalTable): string {
  const lines = [table.columns.map((c) => c.name), ...table.rows].map((r) =>
    r.map(csvField).join(','),
  );
  return `${BOM}${lines.join('\r\n')}\r\n`;
}

/** Parses CSV: quoted fields with `""`, line breaks inside quotes; `,` or `;` (Excel in some locales). */
export function parseCsv(text: string): string[][] {
  const src = text.startsWith(BOM) ? text.slice(1) : text;
  const firstLine = src.slice(0, src.search(/\r?\n|$/));
  const sep = firstLine.split(';').length > firstLine.split(',').length ? ';' : ',';
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < src.length; i++) {
    const ch = src[i] ?? '';
    if (quoted) {
      if (ch === '"') {
        if (src[i + 1] === '"') {
          field += '"';
          i++;
        } else quoted = false;
      } else if (ch !== '\r') field += ch;
    } else if (ch === '"' && field === '') quoted = true;
    else if (ch === sep) {
      row.push(field);
      field = '';
    } else if (ch === '\n' || ch === '\r') {
      if (ch === '\r' && src[i + 1] === '\n') i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else field += ch;
  }
  if (field !== '' || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

/**
 * CSV → a table: the first line names the columns; short rows are padded, the table is cut to the
 * limits. An empty file gives one empty column.
 */
export function csvToTable(text: string): JournalTable {
  const [header = [], ...body] = parseCsv(text);
  const width = Math.min(MAX_COLUMNS, Math.max(1, header.length, ...body.map((r) => r.length)));
  const columns = Array.from({ length: width }, (_, i) => ({
    name: cleanCell(header[i] ?? columnLetter(i)),
    width: DEFAULT_WIDTH,
  }));
  const rows = body
    .slice(0, MAX_ROWS)
    .map((r) => Array.from({ length: width }, (_, i) => cleanCell(r[i] ?? '')));
  return { columns, rows };
}
