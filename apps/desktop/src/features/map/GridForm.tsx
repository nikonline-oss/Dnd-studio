import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MAX_CELL_SIZE, MIN_CELL_SIZE } from './gridPattern';
import { useSubmitIntent } from '@/shared/game/useGame';
import type { GridKind, MapDto } from '@/shared/ipc/client';
import { ErrorText } from '@/shared/ui/ErrorText';

const KINDS: GridKind[] = ['none', 'square', 'hex_flat', 'hex_pointy'];

/**
 * Grid settings of a map (GM roles). A draft until "Apply": one `UpdateMap` with the map version
 * (ADR-0003). Mount with `key={map.version}` so a newer version replaces the draft.
 */
export function GridForm({ map }: { map: MapDto }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const [kind, setKind] = useState<GridKind>(map.grid.kind);
  const [cell, setCell] = useState(String(map.grid.cell_size));
  const [offsetX, setOffsetX] = useState(String(map.grid.offset_x));
  const [offsetY, setOffsetY] = useState(String(map.grid.offset_y));

  const grid = {
    kind,
    cell_size: Number(cell),
    offset_x: Number(offsetX),
    offset_y: Number(offsetY),
  };
  const valid =
    cell.trim() !== '' &&
    grid.cell_size >= MIN_CELL_SIZE &&
    grid.cell_size <= MAX_CELL_SIZE &&
    Number.isFinite(grid.offset_x) &&
    Number.isFinite(grid.offset_y);
  const changed =
    grid.kind !== map.grid.kind ||
    grid.cell_size !== map.grid.cell_size ||
    grid.offset_x !== map.grid.offset_x ||
    grid.offset_y !== map.grid.offset_y;

  return (
    <form
      className="grid-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        submit.mutate({
          intent: { kind: 'update_map', id: map.id, name: null, grid, visible_to_players: null },
          expectedVersion: map.version,
        });
      }}
    >
      <label htmlFor="grid-kind">{t('inspector.grid')}</label>
      <select
        id="grid-kind"
        value={kind}
        onChange={(e) => {
          setKind(e.target.value as GridKind);
        }}
      >
        {KINDS.map((k) => (
          <option key={k} value={k}>
            {t(`inspector.gridKind.${k}`)}
          </option>
        ))}
      </select>
      <label htmlFor="grid-cell">{t('inspector.gridCell')}</label>
      <input
        id="grid-cell"
        type="number"
        min={MIN_CELL_SIZE}
        max={MAX_CELL_SIZE}
        step="any"
        value={cell}
        aria-invalid={!valid}
        onChange={(e) => {
          setCell(e.target.value);
        }}
      />
      <label htmlFor="grid-offset-x">{t('inspector.gridOffsetX')}</label>
      <input
        id="grid-offset-x"
        type="number"
        step="any"
        value={offsetX}
        onChange={(e) => {
          setOffsetX(e.target.value);
        }}
      />
      <label htmlFor="grid-offset-y">{t('inspector.gridOffsetY')}</label>
      <input
        id="grid-offset-y"
        type="number"
        step="any"
        value={offsetY}
        onChange={(e) => {
          setOffsetY(e.target.value);
        }}
      />
      <button type="submit" disabled={!valid || !changed || submit.isPending}>
        {t('inspector.gridApply')}
      </button>
      <ErrorText error={submit.error} />
    </form>
  );
}
