import { useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
  columnLetter,
  deleteColumn,
  deleteRow,
  insertColumn,
  insertRow,
  MAX_COLUMNS,
  MAX_ROWS,
  parseClipboard,
  pasteBlock,
  renameColumn,
  resizeColumn,
  setCell,
} from './table';
import type { JournalTable } from '@/shared/ipc/client';

interface Cell {
  row: number;
  col: number;
}

/** A cell editor opens focused with the caret after what is already typed. */
function focusAtEnd(input: HTMLTextAreaElement | null) {
  if (!input || document.activeElement === input) return;
  input.focus();
  input.setSelectionRange(input.value.length, input.value.length);
}

interface Props {
  table: JournalTable;
  editable: boolean;
  onChange: (table: JournalTable) => void;
}

/**
 * A journal table as a spreadsheet grid (ADR-0026): click a cell to select it, type or press Enter
 * (or double-click) to edit; Enter/Tab move on, Escape cancels, arrows move the selection, Delete
 * clears, and a block copied from Excel pastes in at the selection. Column names are edited in the
 * header; its right edge resizes the column. Cells are stored as typed (formulas come later).
 */
export function TableGrid({ table, editable, onChange }: Props) {
  const { t } = useTranslation();
  const [active, setActive] = useState<Cell>({ row: 0, col: 0 });
  /** The text being typed into the active cell; `null` while not editing. */
  const [editing, setEditingState] = useState<string | null>(null);
  /** The same, readable in handlers of an older render (blur runs after Escape or Enter). */
  const editingRef = useRef<string | null>(null);
  const setEditing = (value: string | null) => {
    editingRef.current = value;
    setEditingState(value);
  };
  const grid = useRef<HTMLDivElement>(null);
  const resizing = useRef<{ col: number; startX: number; startWidth: number } | null>(null);
  const [liveWidth, setLiveWidth] = useState<{ col: number; width: number } | null>(null);

  const rows = table.rows.length;
  const cols = table.columns.length;
  const at = (cell: Cell) => table.rows[cell.row]?.[cell.col] ?? '';
  const clamp = (cell: Cell): Cell => ({
    row: Math.max(0, Math.min(rows - 1, cell.row)),
    col: Math.max(0, Math.min(cols - 1, cell.col)),
  });
  const move = (dRow: number, dCol: number) => {
    setActive((a) => clamp({ row: a.row + dRow, col: a.col + dCol }));
  };
  const focusGrid = () => {
    grid.current?.focus();
  };

  const commit = (dRow: number, dCol: number) => {
    const value = editingRef.current;
    if (value === null) return;
    if (value !== at(active)) onChange(setCell(table, active.row, active.col, value));
    setEditing(null);
    move(dRow, dCol);
    focusGrid();
  };

  const onGridKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (editing !== null || rows === 0) return;
    const step: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
      Tab: [0, e.shiftKey ? -1 : 1],
    };
    const delta = step[e.key];
    if (delta) {
      e.preventDefault();
      move(...delta);
      return;
    }
    if (!editable) return;
    if (e.key === 'Enter' || e.key === 'F2') {
      e.preventDefault();
      setEditing(at(active));
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      if (at(active) !== '') onChange(setCell(table, active.row, active.col, ''));
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      // Typing over a cell replaces it, as in a spreadsheet.
      e.preventDefault();
      setEditing(e.key);
    }
  };

  const onEditKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      commit(1, 0);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      commit(0, e.shiftKey ? -1 : 1);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setEditing(null);
      focusGrid();
    }
  };

  const startResize = (e: PointerEvent<HTMLSpanElement>, col: number) => {
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    resizing.current = { col, startX: e.clientX, startWidth: table.columns[col]?.width ?? 0 };
  };
  const onResizeMove = (e: PointerEvent<HTMLSpanElement>) => {
    const r = resizing.current;
    if (!r) return;
    setLiveWidth({ col: r.col, width: Math.max(40, r.startWidth + e.clientX - r.startX) });
  };
  const endResize = () => {
    const r = resizing.current;
    resizing.current = null;
    if (r && liveWidth) onChange(resizeColumn(table, r.col, liveWidth.width));
    setLiveWidth(null);
  };
  const widthOf = (col: number) =>
    liveWidth?.col === col ? liveWidth.width : (table.columns[col]?.width ?? 0);

  return (
    <div className="table-entry">
      {editable && (
        <div className="table-toolbar" role="toolbar" aria-label={t('table.toolbar')}>
          <button
            type="button"
            disabled={rows >= MAX_ROWS}
            onClick={() => {
              onChange(insertRow(table, rows === 0 ? 0 : active.row + 1));
              if (rows > 0) move(1, 0);
            }}
          >
            {t('table.addRow')}
          </button>
          <button
            type="button"
            disabled={cols >= MAX_COLUMNS}
            onClick={() => {
              onChange(insertColumn(table, active.col + 1));
              move(0, 1);
            }}
          >
            {t('table.addColumn')}
          </button>
          <button
            type="button"
            disabled={rows === 0}
            onClick={() => {
              onChange(deleteRow(table, active.row));
              setActive((a) => ({ row: Math.max(0, Math.min(rows - 2, a.row)), col: a.col }));
            }}
          >
            {t('table.deleteRow')}
          </button>
          <button
            type="button"
            disabled={cols <= 1}
            onClick={() => {
              onChange(deleteColumn(table, active.col));
              setActive((a) => ({ row: a.row, col: Math.max(0, Math.min(cols - 2, a.col)) }));
            }}
          >
            {t('table.deleteColumn')}
          </button>
        </div>
      )}
      <div
        ref={grid}
        className="table-grid"
        role="grid"
        aria-label={t('table.grid')}
        aria-rowcount={rows + 1}
        aria-colcount={cols}
        tabIndex={0}
        onKeyDown={onGridKey}
        onPaste={(e) => {
          if (!editable || editing !== null) return;
          const text = e.clipboardData.getData('text/plain');
          if (!text) return;
          e.preventDefault();
          onChange(pasteBlock(table, active.row, active.col, parseClipboard(text)));
        }}
        onCopy={(e) => {
          if (editing !== null) return;
          e.preventDefault();
          e.clipboardData.setData('text/plain', at(active));
        }}
      >
        <table>
          <colgroup>
            <col className="table-row-head" />
            {table.columns.map((_, i) => (
              <col key={i} style={{ width: widthOf(i) }} />
            ))}
          </colgroup>
          <thead>
            <tr role="row">
              <th className="table-corner" aria-hidden />
              {table.columns.map((c, i) => (
                <th key={i} role="columnheader" scope="col" aria-colindex={i + 1}>
                  <span className="table-letter">{columnLetter(i)}</span>
                  {editable ? (
                    <input
                      className="table-column-name"
                      aria-label={t('table.columnName', { letter: columnLetter(i) })}
                      value={c.name}
                      maxLength={200}
                      onChange={(e) => {
                        onChange(renameColumn(table, i, e.target.value));
                      }}
                    />
                  ) : (
                    <span className="table-column-name">{c.name}</span>
                  )}
                  {editable && (
                    <span
                      className="table-resize"
                      role="separator"
                      aria-orientation="vertical"
                      aria-label={t('table.resize', { letter: columnLetter(i) })}
                      onPointerDown={(e) => {
                        startResize(e, i);
                      }}
                      onPointerMove={onResizeMove}
                      onPointerUp={endResize}
                      onPointerCancel={endResize}
                    />
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {table.rows.map((row, r) => (
              <tr key={r} role="row" aria-rowindex={r + 2}>
                <th scope="row" className="table-row-head">
                  {r + 1}
                </th>
                {row.map((value, c) => {
                  const isActive = active.row === r && active.col === c;
                  return (
                    <td
                      key={c}
                      role="gridcell"
                      aria-colindex={c + 1}
                      aria-selected={isActive}
                      className={value.startsWith('=') ? 'table-formula' : undefined}
                      onClick={() => {
                        if (editing !== null && !isActive) commit(0, 0);
                        setActive({ row: r, col: c });
                      }}
                      onDoubleClick={() => {
                        if (!editable) return;
                        setActive({ row: r, col: c });
                        setEditing(value);
                      }}
                    >
                      {isActive && editing !== null ? (
                        <textarea
                          className="table-cell-input"
                          aria-label={`${columnLetter(c)}${r + 1}`}
                          ref={focusAtEnd}
                          value={editing}
                          onChange={(e) => {
                            setEditing(e.target.value);
                          }}
                          onKeyDown={onEditKey}
                          onBlur={() => {
                            commit(0, 0);
                          }}
                        />
                      ) : (
                        value
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {rows === 0 && <p className="muted">{t('table.empty')}</p>}
      </div>
    </div>
  );
}
