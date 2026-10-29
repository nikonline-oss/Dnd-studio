import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { BoardDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { NavSection } from '@/shared/ui/NavSection';

/**
 * Navigator: the campaign's boards (ADR-0024). GM roles create, rename, show and delete them;
 * players see the boards shown to them (the server sends no others).
 */
export function BoardList() {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const boards = useSnapshotSelect((s) => s.boards) ?? [];
  const maps = useSnapshotSelect((s) => s.maps) ?? [];
  const nodes = useSnapshotSelect((s) => s.board_nodes) ?? [];
  const active = useSelection((s) => s.active);
  const openBoard = useSelection((s) => s.openBoard);
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const sorted = [...boards].sort((a, b) => a.sort_order - b.sort_order);
  const openMap = useSelection((s) => s.openMap);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const mapsInBoard = (boardId: string) =>
    nodes.flatMap((node) => {
      const content = node.content;
      if (node.board_id !== boardId || content.type !== 'link' || content.target.kind !== 'map')
        return [];
      const map = maps.find((candidate) => candidate.id === content.target.id);
      return map ? [map] : [];
    });

  const update = (board: BoardDto, patch: { name?: string; visible_to_players?: boolean }) => {
    submit.mutate({
      intent: {
        kind: 'update_board',
        id: board.id,
        name: patch.name ?? null,
        visible_to_players: patch.visible_to_players ?? null,
      },
      expectedVersion: board.version,
    });
  };

  return (
    <NavSection
      icon="board"
      title={t('boards.title')}
      count={boards.length}
      actions={
        isGm && (
          <IconButton
            icon="plus"
            label={t('boards.add')}
            disabled={submit.isPending}
            onClick={() => {
              const id = uuidv7();
              submit.mutate(
                {
                  intent: {
                    kind: 'create_board',
                    id,
                    name: `${t('boards.newName')} ${boards.length + 1}`,
                  },
                },
                {
                  onSuccess: () => {
                    openBoard(id);
                  },
                },
              );
            }}
          />
        )
      }
    >
      {boards.length === 0 && (
        <p className="nav-empty">{isGm ? t('boards.empty') : t('boards.emptyPlayer')}</p>
      )}
      <ul className="nav-list" aria-label={t('boards.title')}>
        {sorted.map((board) => (
          <li key={board.id} className="board-tree-group">
            <div
              className="nav-row"
              aria-current={active?.kind === 'board' && active.id === board.id}
            >
              <button
                type="button"
                className="board-expand"
                aria-label={t(
                  expanded.has(board.id) ? 'boards.collapseMaps' : 'boards.expandMaps',
                  { name: board.name },
                )}
                aria-expanded={expanded.has(board.id)}
                onClick={() => {
                  setExpanded((current) => {
                    const next = new Set(current);
                    if (!next.delete(board.id)) next.add(board.id);
                    return next;
                  });
                }}
              >
                <Icon name="chevron" />
              </button>
              <button
                type="button"
                className="board-open"
                onClick={() => {
                  openBoard(board.id);
                }}
              >
                <Icon name="board" className="row-icon" />
                {isGm ? (
                  <InlineEdit
                    value={board.name}
                    label={t('boards.rename')}
                    onCommit={(name) => {
                      update(board, { name });
                    }}
                  />
                ) : (
                  <span className="nav-row-name">{board.name}</span>
                )}
              </button>
              {isGm && (
                <>
                  <label
                    className="eye-toggle"
                    data-on={board.visible_to_players}
                    title={board.visible_to_players ? t('boards.visible') : t('boards.hidden')}
                    onClick={(e) => {
                      e.stopPropagation();
                    }}
                  >
                    <input
                      type="checkbox"
                      className="visually-hidden"
                      aria-label={t('boards.visibleHint')}
                      checked={board.visible_to_players}
                      onChange={(e) => {
                        update(board, { visible_to_players: e.target.checked });
                      }}
                    />
                    <Icon name={board.visible_to_players ? 'eye' : 'eyeOff'} />
                  </label>
                  <span className="row-actions">
                    <ConfirmDelete
                      label={t('boards.delete')}
                      onConfirm={() => {
                        submit.mutate({
                          intent: { kind: 'delete_board', id: board.id },
                          expectedVersion: board.version,
                        });
                      }}
                    />
                  </span>
                </>
              )}
            </div>
            {expanded.has(board.id) && (
              <ul
                className="board-map-list"
                aria-label={t('boards.mapsInBoard', { name: board.name })}
              >
                {mapsInBoard(board.id).map((map) => (
                  <li key={map.id}>
                    <button
                      type="button"
                      className="nav-row board-map-row"
                      onClick={() => {
                        openMap(map.id);
                      }}
                    >
                      <Icon name="map" className="row-icon" />
                      <span className="nav-row-name">{map.name}</span>
                    </button>
                  </li>
                ))}
                {mapsInBoard(board.id).length === 0 && (
                  <li className="nav-empty">{t('boards.noMaps')}</li>
                )}
              </ul>
            )}
          </li>
        ))}
      </ul>
      <ErrorText error={submit.error} />
    </NavSection>
  );
}
