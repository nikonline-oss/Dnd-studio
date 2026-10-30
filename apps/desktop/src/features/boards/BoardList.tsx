import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import type { BoardDto, MapDto } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';
import { ContextMenu } from '@/shared/ui/ContextMenu';
import type { MenuItem } from '@/shared/ui/Menu';
import { NavSection } from '@/shared/ui/NavSection';
import './BoardList.css';

type Target = { kind: 'board'; item: BoardDto } | { kind: 'map'; item: MapDto };
export function BoardList() {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const boards = useSnapshotSelect((s) => s.boards) ?? [];
  const maps = useSnapshotSelect((s) => s.maps) ?? [];
  const nodes = useSnapshotSelect((s) => s.board_nodes) ?? [];
  const active = useSelection((s) => s.active);
  const openBoard = useSelection((s) => s.openBoard);
  const openMap = useSelection((s) => s.openMap);
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [menu, setMenu] = useState<{ x: number; y: number; target: Target | null } | null>(null);
  const [editing, setEditing] = useState<{ target: Target; name: string } | null>(null);
  const [deleting, setDeleting] = useState<Target | null>(null);
  const toggle = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  };
  const add = () => {
    const id = uuidv7();
    submit.mutate(
      { intent: { kind: 'create_board', id, name: `${t('boards.newName')} ${boards.length + 1}` } },
      {
        onSuccess: () => {
          openBoard(id);
        },
      },
    );
  };
  const open = (target: Target) => {
    if (target.kind === 'board') openBoard(target.item.id);
    else openMap(target.item.id);
  };
  const update = (target: Target, patch: { name?: string; visible_to_players?: boolean }) => {
    const common = {
      id: target.item.id,
      name: patch.name ?? null,
      visible_to_players: patch.visible_to_players ?? null,
    };
    submit.mutate({
      intent:
        target.kind === 'board'
          ? { kind: 'update_board', ...common }
          : { kind: 'update_map', ...common, grid: null },
      expectedVersion: target.item.version,
    });
  };
  const context = (e: MouseEvent, target: Target | null) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, target });
  };
  const commit = () => {
    if (!editing) return;
    const name = editing.name.trim();
    if (name && name !== editing.target.item.name) update(editing.target, { name });
    setEditing(null);
  };
  const items: MenuItem[] = [];
  if (menu?.target) {
    const target = menu.target;
    items.push({
      label: t('boards.open', { defaultValue: 'Open' }),
      onSelect: () => {
        open(target);
      },
    });
    if (target.kind === 'board')
      items.push({
        label: t(expanded.has(target.item.id) ? 'boards.collapseMaps' : 'boards.expandMaps', {
          name: target.item.name,
        }),
        onSelect: () => {
          toggle(target.item.id);
        },
      });
    if (isGm)
      items.push(
        {
          label: t(target.kind === 'board' ? 'boards.rename' : 'maps.rename'),
          disabled: submit.isPending,
          onSelect: () => {
            setEditing({ target, name: target.item.name });
          },
        },
        {
          label: t(target.item.visible_to_players ? 'boards.hidePlayers' : 'boards.showPlayers', {
            defaultValue: target.item.visible_to_players ? 'Hide from players' : 'Show to players',
          }),
          disabled: submit.isPending,
          onSelect: () => {
            update(target, { visible_to_players: !target.item.visible_to_players });
          },
        },
        {
          label: t(target.kind === 'board' ? 'boards.delete' : 'maps.delete'),
          disabled: submit.isPending,
          onSelect: () => {
            setDeleting(target);
          },
        },
      );
  }
  if (isGm) items.push({ label: t('boards.add'), disabled: submit.isPending, onSelect: add });
  const name = (target: Target) =>
    editing?.target.kind === target.kind && editing.target.item.id === target.item.id ? (
      <input
        autoFocus
        aria-label={t(target.kind === 'board' ? 'boards.rename' : 'maps.rename')}
        value={editing.name}
        maxLength={200}
        onClick={(e) => {
          e.stopPropagation();
        }}
        onChange={(e) => {
          setEditing({ ...editing, name: e.target.value });
        }}
        onBlur={commit}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') commit();
          if (e.key === 'Escape') setEditing(null);
        }}
      />
    ) : (
      <span
        onClick={() => {
          open(target);
        }}
        className="board-nav-name"
        onDoubleClick={() => {
          if (isGm) setEditing({ target, name: target.item.name });
        }}
      >
        {target.item.name}
      </span>
    );
  return (
    <div
      className="boards-navigator"
      onContextMenu={(e) => {
        context(e, null);
      }}
    >
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
              onClick={add}
            />
          )
        }
      >
        {boards.length === 0 && (
          <p className="nav-empty">{t(isGm ? 'boards.empty' : 'boards.emptyPlayer')}</p>
        )}
        <ul className="nav-list board-nav-list" aria-label={t('boards.title')}>
          {[...boards]
            .sort((a, b) => a.sort_order - b.sort_order)
            .map((board) => {
              const target: Target = { kind: 'board', item: board };
              const children = maps.filter((map) =>
                nodes.some(
                  (node) =>
                    node.board_id === board.id &&
                    node.content.type === 'link' &&
                    node.content.target.kind === 'map' &&
                    node.content.target.id === map.id,
                ),
              );
              return (
                <li
                  key={board.id}
                  className="board-tree-group"
                  onContextMenu={(e) => {
                    context(e, target);
                  }}
                >
                  <div
                    className="board-nav-row"
                    aria-current={active?.kind === 'board' && active.id === board.id}
                  >
                    <button
                      className="board-nav-expand"
                      type="button"
                      aria-expanded={expanded.has(board.id)}
                      aria-label={t(
                        expanded.has(board.id) ? 'boards.collapseMaps' : 'boards.expandMaps',
                        { name: board.name },
                      )}
                      onClick={() => {
                        toggle(board.id);
                      }}
                    >
                      <Icon name="chevron" />
                    </button>
                    <div className="board-nav-content">
                      <button
                        type="button"
                        className="board-nav-open"
                        aria-label={board.name}
                        onClick={() => {
                          openBoard(board.id);
                        }}
                      >
                        <Icon name="board" />
                      </button>
                      {name(target)}
                    </div>
                    <span className="board-nav-count">{children.length}</span>
                    <Icon
                      name={board.visible_to_players ? 'eye' : 'eyeOff'}
                      className="board-nav-visibility"
                    />
                    <button
                      type="button"
                      className="board-nav-more"
                      aria-label={t('boards.commands', {
                        defaultValue: 'Commands for {{name}}',
                        name: board.name,
                      })}
                      onClick={(e) => {
                        context(e, target);
                      }}
                    >
                      ⋯
                    </button>
                  </div>
                  {expanded.has(board.id) && (
                    <ul
                      className="board-map-list"
                      aria-label={t('boards.mapsInBoard', { name: board.name })}
                    >
                      {children.map((map) => {
                        const child: Target = { kind: 'map', item: map };
                        return (
                          <li
                            key={map.id}
                            className="board-nav-row board-nav-map"
                            aria-current={active?.kind === 'map' && active.id === map.id}
                            onContextMenu={(e) => {
                              context(e, child);
                            }}
                          >
                            <button
                              type="button"
                              className="board-nav-open"
                              aria-label={map.name}
                              onClick={() => {
                                openMap(map.id);
                              }}
                            >
                              <Icon name="map" />
                              {editing?.target.item.id !== map.id && (
                                <span className="board-nav-name">{map.name}</span>
                              )}
                            </button>
                            {editing?.target.item.id === map.id && name(child)}
                            <button
                              type="button"
                              className="board-nav-more"
                              aria-label={t('boards.commands', {
                                defaultValue: 'Commands for {{name}}',
                                name: map.name,
                              })}
                              onClick={(e) => {
                                context(e, child);
                              }}
                            >
                              ⋯
                            </button>
                          </li>
                        );
                      })}
                      {children.length === 0 && <li className="nav-empty">{t('boards.noMaps')}</li>}
                    </ul>
                  )}
                </li>
              );
            })}
        </ul>
        <ErrorText error={submit.error} />
      </NavSection>
      {menu && items.length > 0 && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t('boards.title')}
          items={items}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
      {deleting && (
        <div
          className="board-nav-confirm"
          role="alertdialog"
          aria-label={t(deleting.kind === 'board' ? 'boards.delete' : 'maps.delete')}
        >
          <p>
            {t(deleting.kind === 'map' ? 'boards.confirmDeleteMap' : 'boards.confirmDeleteBoard', {
              name: deleting.item.name,
              defaultValue:
                deleting.kind === 'map'
                  ? 'Delete map “{{name}}” and its contents from the campaign?'
                  : 'Delete board “{{name}}” and its contents?',
            })}
          </p>
          <button
            type="button"
            onClick={() => {
              setDeleting(null);
            }}
          >
            {t('boards.cancel', { defaultValue: 'Cancel' })}
          </button>
          <button
            type="button"
            disabled={submit.isPending}
            onClick={() => {
              submit.mutate({
                intent:
                  deleting.kind === 'board'
                    ? { kind: 'delete_board', id: deleting.item.id }
                    : { kind: 'delete_map', id: deleting.item.id },
                expectedVersion: deleting.item.version,
              });
              setDeleting(null);
            }}
          >
            {t('common.confirmDelete')}
          </button>
        </div>
      )}
    </div>
  );
}
