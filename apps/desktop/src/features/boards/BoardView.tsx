import { useEffect, useRef, useState, type PointerEvent, type WheelEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { BoardToolbar } from './BoardToolbar';
import { cardLine } from './geometry';
import { useCampaign, useSnapshotSelect, useSubmitIntent } from '@/shared/game/useGame';
import {
  assetUrl,
  type BoardEdgeDto,
  type BoardLink,
  type BoardNodeDto,
  type BoardVisibility,
  type TriggerDto,
} from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { ConfirmDelete, IconButton } from '@/shared/ui/IconButton';
import { InlineEdit } from '@/shared/ui/InlineEdit';
import { Markdown } from '@/shared/ui/Markdown';

/** Board coordinates are Units16 (ADR-0016); the canvas draws 1 unit = 1 CSS px at zoom 1. */
const U = 16;
/** A press that moves less than this (CSS px) is a click, not a drag. */
const DRAG_THRESHOLD = 4;
/** How long a dropped card keeps its local position while the move comes back (ms). */
const LANDING_MS = 3000;
const MIN_ZOOM = 0.2;
/** Smallest card a GM can resize to (CSS px; the core allows 16). */
const MIN_CARD = 48;
const MAX_ZOOM = 2.5;

type Selected = { kind: 'node'; id: string } | { kind: 'edge'; id: string } | null;

interface View {
  x: number;
  y: number;
  zoom: number;
}

/**
 * A board in a center tab (ADR-0024): cards (notes, links, pictures) and the lines between them
 * on a pannable, zoomable canvas. GM roles arrange it and decide what players see; players see
 * what is open to them, read-only. Every change is an intent.
 */
export function BoardView({ boardId }: { boardId: string }) {
  const { t } = useTranslation();
  const submit = useSubmitIntent();
  const { data: campaign } = useCampaign();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const board = useSnapshotSelect((s) => s.boards.find((b) => b.id === boardId));
  const nodes = useSnapshotSelect((s) => s.board_nodes.filter((n) => n.board_id === boardId));
  const edges = useSnapshotSelect((s) => s.board_edges.filter((e) => e.board_id === boardId));
  const triggers = useSnapshotSelect((s) => s.triggers);
  const [view, setView] = useState<View>({ x: 40, y: 40, zoom: 1 });
  const [selected, setSelected] = useState<Selected>(null);
  const [connecting, setConnecting] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  /** Cards dragged or just dropped here: shown at the local position (px). */
  const [local, setLocal] = useState<Record<string, { x: number; y: number }>>({});
  /** Cards being resized or just resized here: shown at the local size (px). */
  const [sizes, setSizes] = useState<Record<string, { w: number; h: number }>>({});
  const canvas = useRef<HTMLDivElement>(null);
  const gesture = useRef<
    | { kind: 'pan'; from: { x: number; y: number }; moved: boolean }
    | {
        kind: 'node';
        id: string;
        from: { x: number; y: number };
        start: { x: number; y: number };
        moved: boolean;
      }
    | {
        kind: 'resize';
        id: string;
        from: { x: number; y: number };
        start: { w: number; h: number };
        moved: boolean;
      }
    | null
  >(null);

  // A dropped card returns to the events' position once they bring the drop point.
  useEffect(() => {
    if (!nodes) return;
    if (gesture.current) return;
    setLocal((current) => {
      const landed = nodes.filter((node) => {
        const at = current[node.id];
        return at && node.x === Math.round(at.x * U) && node.y === Math.round(at.y * U);
      });
      return landed.reduce((acc, node) => without(acc, node.id), current);
    });
  }, [nodes]);

  if (!board || !nodes || !edges) {
    return <p className="muted">{t('boards.missing')}</p>;
  }

  const position = (node: BoardNodeDto) => local[node.id] ?? { x: node.x / U, y: node.y / U };
  const triggerLines = isGm ? linesOfTriggers(nodes, triggers ?? []) : [];
  const sizeOf = (node: BoardNodeDto) =>
    sizes[node.id] ?? { w: node.width / U, h: node.height / U };
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const selectedNode = selected?.kind === 'node' ? byId.get(selected.id) : undefined;
  const selectedEdge =
    selected?.kind === 'edge' ? edges.find((e) => e.id === selected.id) : undefined;

  /** Canvas point (CSS px, relative to the canvas) → board px. */
  const toBoard = (cx: number, cy: number) => ({
    x: (cx - view.x) / view.zoom,
    y: (cy - view.y) / view.zoom,
  });
  /** Where a new card goes: the middle of what is on screen. */
  const spawnPoint = () => {
    const rect = canvas.current?.getBoundingClientRect();
    const p = toBoard((rect?.width ?? 600) / 2, (rect?.height ?? 400) / 2);
    return { x: Math.round(p.x * U), y: Math.round(p.y * U) };
  };

  const createNode = (content: BoardNodeDto['content'], width: number, height: number) => {
    const at = spawnPoint();
    const id = uuidv7();
    submit.mutate(
      {
        intent: {
          kind: 'create_board_node',
          id,
          board_id: boardId,
          content,
          x: at.x - (width * U) / 2,
          y: at.y - (height * U) / 2,
          width: width * U,
          height: height * U,
          visibility: 'gm',
        },
      },
      {
        onSuccess: () => {
          setSelected({ kind: 'node', id });
        },
      },
    );
  };

  const connect = (from: string, to: string) => {
    submit.mutate({
      intent: {
        kind: 'create_board_edge',
        id: uuidv7(),
        board_id: boardId,
        from,
        to,
        label: '',
        directed: true,
        visibility: 'gm',
      },
    });
  };

  const onCanvasDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    gesture.current = { kind: 'pan', from: { x: e.clientX, y: e.clientY }, moved: false };
  };

  const onNodeDown = (node: BoardNodeDto, e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    if (editing === node.id) return;
    canvas.current?.setPointerCapture(e.pointerId);
    gesture.current = {
      kind: 'node',
      id: node.id,
      from: { x: e.clientX, y: e.clientY },
      start: position(node),
      moved: false,
    };
  };

  const onResizeDown = (node: BoardNodeDto, e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    canvas.current?.setPointerCapture(e.pointerId);
    gesture.current = {
      kind: 'resize',
      id: node.id,
      from: { x: e.clientX, y: e.clientY },
      start: sizeOf(node),
      moved: false,
    };
  };

  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    const g = gesture.current;
    if (!g) return;
    const dx = e.clientX - g.from.x;
    const dy = e.clientY - g.from.y;
    if (!g.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    g.moved = true;
    if (g.kind === 'pan') {
      g.from = { x: e.clientX, y: e.clientY };
      setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
    } else if (g.kind === 'resize') {
      setSizes((s) => ({
        ...s,
        [g.id]: {
          w: Math.max(MIN_CARD, g.start.w + dx / view.zoom),
          h: Math.max(MIN_CARD, g.start.h + dy / view.zoom),
        },
      }));
    } else if (isGm) {
      setLocal((l) => ({
        ...l,
        [g.id]: { x: g.start.x + dx / view.zoom, y: g.start.y + dy / view.zoom },
      }));
    }
  };

  const onUp = () => {
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    if (g.kind === 'pan') {
      if (!g.moved) {
        setSelected(null);
        setConnecting(false);
      }
      return;
    }
    if (g.kind === 'resize') {
      const size = sizes[g.id];
      const node = nodes.find((n) => n.id === g.id);
      if (!g.moved || !size || !node) return;
      submit.mutate({
        intent: {
          kind: 'update_board_node',
          id: g.id,
          content: null,
          width: Math.round(size.w * U),
          height: Math.round(size.h * U),
          visibility: null,
        },
        expectedVersion: node.version,
      });
      window.setTimeout(() => {
        setSizes((s) => without(s, g.id));
      }, LANDING_MS);
      return;
    }
    if (!g.moved || !isGm) {
      // A click: select, or finish a connection.
      if (connecting && selected?.kind === 'node' && selected.id !== g.id) {
        connect(selected.id, g.id);
        setConnecting(false);
      }
      setSelected({ kind: 'node', id: g.id });
      return;
    }
    const at = local[g.id];
    if (!at) return;
    submit.mutate({
      intent: {
        kind: 'move_board_node',
        id: g.id,
        x: Math.round(at.x * U),
        y: Math.round(at.y * U),
      },
    });
    window.setTimeout(() => {
      setLocal((l) => without(l, g.id));
    }, LANDING_MS);
  };

  const onWheel = (e: WheelEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    setView((v) => {
      const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom * Math.exp(-e.deltaY * 0.0015)));
      // Zoom at the cursor: the board point under it stays under it.
      const bx = (cx - v.x) / v.zoom;
      const by = (cy - v.y) / v.zoom;
      return { zoom, x: cx - bx * zoom, y: cy - by * zoom };
    });
  };

  const setNodeVisibility = (node: BoardNodeDto, visibility: BoardVisibility) => {
    submit.mutate({
      intent: {
        kind: 'update_board_node',
        id: node.id,
        content: null,
        width: null,
        height: null,
        visibility,
      },
      expectedVersion: node.version,
    });
  };

  const updateEdge = (
    edge: BoardEdgeDto,
    patch: { label?: string; directed?: boolean; visibility?: BoardVisibility },
  ) => {
    submit.mutate({
      intent: {
        kind: 'update_board_edge',
        id: edge.id,
        label: patch.label ?? null,
        directed: patch.directed ?? null,
        visibility: patch.visibility ?? null,
      },
      expectedVersion: edge.version,
    });
  };

  return (
    <section className="board-view" aria-label={board.name}>
      {isGm && (
        <BoardToolbar
          onNote={() => {
            createNode({ type: 'note', markdown: t('boards.newNote') }, 220, 120);
          }}
          onLink={(target) => {
            // A map link has room for the map's picture.
            if (target.kind === 'map') createNode({ type: 'link', target }, 220, 160);
            else createNode({ type: 'link', target }, 200, 64);
          }}
          onImage={(assetId) => {
            createNode({ type: 'image', asset_id: assetId }, 240, 180);
          }}
          onTrigger={(triggerId) => {
            if (triggerId !== null) {
              createNode({ type: 'trigger', id: triggerId }, 260, 150);
              return;
            }
            // A new trigger, placed at once; it is edited in its tab.
            const id = uuidv7();
            submit.mutate(
              {
                intent: {
                  kind: 'create_trigger',
                  id,
                  definition: {
                    name: t('triggers.newName'),
                    enabled: false,
                    once: false,
                    event: { type: 'manual' },
                    tokens: [],
                    condition: null,
                    steps: [
                      {
                        when: null,
                        action: { type: 'chat', text: t('triggers.newText'), gm_only: true },
                      },
                    ],
                  },
                },
              },
              {
                onSuccess: () => {
                  createNode({ type: 'trigger', id }, 260, 150);
                },
              },
            );
          }}
          connecting={connecting}
          canConnect={selectedNode !== undefined}
          onConnect={() => {
            setConnecting((c) => !c);
          }}
          onResetView={() => {
            setView({ x: 40, y: 40, zoom: 1 });
          }}
          disabled={submit.isPending}
        />
      )}

      <div
        ref={canvas}
        className={connecting ? 'board-canvas connecting' : 'board-canvas'}
        role="application"
        aria-label={t('boards.canvas')}
        onPointerDown={onCanvasDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
        onWheel={onWheel}
      >
        <div
          className="board-world"
          style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})` }}
        >
          <svg className="board-edges" aria-hidden={!isGm}>
            <defs>
              <marker
                id={`arrow-${boardId}`}
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="8"
                markerHeight="8"
                orient="auto-start-reverse"
              >
                <path d="M0 0L10 5L0 10z" className="board-arrow" />
              </marker>
            </defs>
            {edges.map((edge) => {
              const from = byId.get(edge.from);
              const to = byId.get(edge.to);
              if (!from || !to) return null;
              const { start: a, end } = cardLine(
                { at: position(from), size: sizeOf(from) },
                { at: position(to), size: sizeOf(to) },
              );
              const isSelected = selected?.kind === 'edge' && selected.id === edge.id;
              return (
                <g
                  key={edge.id}
                  className="board-edge"
                  data-hidden={edge.visibility === 'gm'}
                  data-selected={isSelected}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                  }}
                  onClick={() => {
                    if (isGm) setSelected({ kind: 'edge', id: edge.id });
                  }}
                >
                  <line x1={a.x} y1={a.y} x2={end.x} y2={end.y} className="board-edge-hit" />
                  <line
                    x1={a.x}
                    y1={a.y}
                    x2={end.x}
                    y2={end.y}
                    className="board-edge-line"
                    markerEnd={edge.directed ? `url(#arrow-${boardId})` : undefined}
                  />
                  {edge.label && (
                    <text
                      x={(a.x + end.x) / 2}
                      y={(a.y + end.y) / 2 - 6}
                      className="board-edge-label"
                    >
                      {edge.label}
                    </text>
                  )}
                </g>
              );
            })}
            {/* What triggers on this board open: drawn, not stored. */}
            {triggerLines.map(({ key, from, to }) => {
              const { start: a, end } = cardLine(
                { at: position(from), size: sizeOf(from) },
                { at: position(to), size: sizeOf(to) },
              );
              return (
                <line
                  key={key}
                  x1={a.x}
                  y1={a.y}
                  x2={end.x}
                  y2={end.y}
                  className="board-trigger-line"
                  markerEnd={`url(#arrow-${boardId})`}
                />
              );
            })}
          </svg>

          {nodes.map((node) => {
            const at = position(node);
            const size = sizeOf(node);
            return (
              <BoardCard
                key={node.id}
                node={node}
                x={at.x}
                y={at.y}
                width={size.w}
                height={size.h}
                onResizeDown={(e) => {
                  onResizeDown(node, e);
                }}
                selected={selected?.kind === 'node' && selected.id === node.id}
                isGm={isGm}
                editing={editing === node.id}
                onPointerDown={(e) => {
                  onNodeDown(node, e);
                }}
                onEdit={() => {
                  if (isGm && node.content.type === 'note') setEditing(node.id);
                }}
                onSave={(markdown) => {
                  setEditing(null);
                  if (node.content.type !== 'note' || markdown === node.content.markdown) return;
                  submit.mutate({
                    intent: {
                      kind: 'update_board_node',
                      id: node.id,
                      content: { type: 'note', markdown },
                      width: null,
                      height: null,
                      visibility: null,
                    },
                    expectedVersion: node.version,
                  });
                }}
              />
            );
          })}
        </div>
        {nodes.length === 0 && (
          <p className="board-empty muted">
            {isGm ? t('boards.emptyBoard') : t('boards.nothingYet')}
          </p>
        )}
      </div>

      {isGm && selectedNode && (
        <div className="board-selection" role="toolbar" aria-label={t('boards.card')}>
          <IconButton
            icon={selectedNode.visibility === 'players' ? 'eye' : 'eyeOff'}
            label={
              selectedNode.visibility === 'players'
                ? t('boards.shownToPlayers')
                : t('boards.gmOnly')
            }
            aria-pressed={selectedNode.visibility === 'players'}
            onClick={() => {
              setNodeVisibility(
                selectedNode,
                selectedNode.visibility === 'players' ? 'gm' : 'players',
              );
            }}
          >
            {selectedNode.visibility === 'players'
              ? t('boards.shownToPlayers')
              : t('boards.gmOnly')}
          </IconButton>
          {selectedNode.content.type === 'note' && (
            <IconButton
              icon="note"
              label={t('boards.editNote')}
              onClick={() => {
                setEditing(selectedNode.id);
              }}
            />
          )}
          <IconButton
            icon="link"
            label={t('boards.connect')}
            aria-pressed={connecting}
            onClick={() => {
              setConnecting((c) => !c);
            }}
          />
          <ConfirmDelete
            label={t('boards.deleteCard')}
            onConfirm={() => {
              setSelected(null);
              submit.mutate({
                intent: { kind: 'delete_board_node', id: selectedNode.id },
                expectedVersion: selectedNode.version,
              });
            }}
          />
        </div>
      )}
      {isGm && selectedEdge && (
        <div className="board-selection" role="toolbar" aria-label={t('boards.line')}>
          <InlineEdit
            value={selectedEdge.label || t('boards.noLabel')}
            label={t('boards.lineLabel')}
            onCommit={(label) => {
              updateEdge(selectedEdge, { label });
            }}
          />
          <label className="inline">
            <input
              type="checkbox"
              checked={selectedEdge.directed}
              onChange={(e) => {
                updateEdge(selectedEdge, { directed: e.target.checked });
              }}
            />
            {t('boards.arrow')}
          </label>
          <IconButton
            icon={selectedEdge.visibility === 'players' ? 'eye' : 'eyeOff'}
            label={
              selectedEdge.visibility === 'players'
                ? t('boards.shownToPlayers')
                : t('boards.gmOnly')
            }
            aria-pressed={selectedEdge.visibility === 'players'}
            onClick={() => {
              updateEdge(selectedEdge, {
                visibility: selectedEdge.visibility === 'players' ? 'gm' : 'players',
              });
            }}
          />
          <ConfirmDelete
            label={t('boards.deleteLine')}
            onConfirm={() => {
              setSelected(null);
              submit.mutate({
                intent: { kind: 'delete_board_edge', id: selectedEdge.id },
                expectedVersion: selectedEdge.version,
              });
            }}
          />
        </div>
      )}
      {connecting && <p className="board-hint">{t('boards.connectHint')}</p>}
      <ErrorText error={submit.error} />
    </section>
  );
}

/** `record` without `key` (the same object when it has none). */
function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  if (!(key in record)) return record;
  return Object.fromEntries(Object.entries(record).filter(([k]) => k !== key));
}

function BoardCard({
  node,
  x,
  y,
  width,
  height,
  selected,
  isGm,
  editing,
  onPointerDown,
  onResizeDown,
  onEdit,
  onSave,
}: {
  node: BoardNodeDto;
  x: number;
  y: number;
  width: number;
  height: number;
  onResizeDown: (e: PointerEvent<HTMLDivElement>) => void;
  selected: boolean;
  isGm: boolean;
  editing: boolean;
  onPointerDown: (e: PointerEvent<HTMLDivElement>) => void;
  onEdit: () => void;
  onSave: (markdown: string) => void;
}) {
  const { t } = useTranslation();
  const content = node.content;
  return (
    <div
      className={`board-card board-card-${content.type}`}
      data-selected={selected}
      data-hidden={node.visibility === 'gm'}
      role="group"
      aria-label={cardTitle(node, t)}
      style={{ left: x, top: y, width, height }}
      onPointerDown={onPointerDown}
      onDoubleClick={onEdit}
    >
      {isGm && (
        // A strip to grab the card by: links and pictures are clickable all over.
        <div className="board-card-handle" title={t('boards.dragHandle')} aria-hidden="true">
          <Icon name="grip" size={14} />
          {node.visibility === 'gm' && (
            <span className="board-card-badge" title={t('boards.gmOnly')}>
              <Icon name="eyeOff" size={12} />
            </span>
          )}
        </div>
      )}
      {content.type === 'note' ? (
        editing ? (
          <NoteEditor initial={content.markdown} onSave={onSave} />
        ) : (
          <Markdown source={content.markdown} />
        )
      ) : content.type === 'link' ? (
        <LinkCard target={content.target} />
      ) : content.type === 'image' ? (
        <ImageCard assetId={content.asset_id} />
      ) : (
        <TriggerCard triggerId={content.id} />
      )}
      {isGm && (
        <div
          className="board-card-resize"
          title={t('boards.resize')}
          aria-hidden="true"
          onPointerDown={onResizeDown}
        />
      )}
    </div>
  );
}

function cardTitle(node: BoardNodeDto, t: (key: string) => string): string {
  switch (node.content.type) {
    case 'note':
      return t('boards.note');
    case 'link':
      return t('boards.link');
    case 'image':
      return t('boards.image');
    case 'trigger':
      return t('boards.trigger');
  }
}

/** Lines from each trigger block to the cards of this board its steps open or hide. */
function linesOfTriggers(
  nodes: BoardNodeDto[],
  triggers: TriggerDto[],
): { key: string; from: BoardNodeDto; to: BoardNodeDto }[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return nodes.flatMap((block) => {
    if (block.content.type !== 'trigger') return [];
    const triggerId = block.content.id;
    const trigger = triggers.find((tr) => tr.id === triggerId);
    if (!trigger) return [];
    return trigger.definition.steps.flatMap(({ action }, i) => {
      const target =
        action.type === 'reveal' && action.target.kind === 'node'
          ? action.target.id
          : action.type === 'set_node_visibility'
            ? action.node
            : null;
      const to = target === null ? undefined : byId.get(target);
      return to ? [{ key: `${block.id}:${i}`, from: block, to }] : [];
    });
  });
}

/** A trigger as a block (GM roles only): what sets it off, what it does, on/off and Run. */
function TriggerCard({ triggerId }: { triggerId: string }) {
  const { t } = useTranslation();
  const trigger = useSnapshotSelect((s) => s.triggers.find((tr) => tr.id === triggerId));
  const submit = useSubmitIntent();
  const openTrigger = useSelection((s) => s.openTrigger);
  if (!trigger) return <p className="muted">{t('boards.unavailable')}</p>;
  const def = trigger.definition;
  const stop = (e: PointerEvent) => {
    e.stopPropagation();
  };
  return (
    <div
      className="board-trigger"
      data-enabled={def.enabled}
      onDoubleClick={(e) => {
        e.stopPropagation();
        openTrigger(trigger.id);
      }}
    >
      <strong className="board-trigger-name">
        <Icon name="bolt" size={14} />
        {def.name}
      </strong>
      <span className="board-trigger-when">{t(`triggers.events.${def.event.type}`)}</span>
      <ol className="board-trigger-steps">
        {def.steps.slice(0, 4).map((step, i) => (
          <li key={i}>{t(`triggers.actions.${step.action.type}`)}</li>
        ))}
        {def.steps.length > 4 && <li>…</li>}
      </ol>
      <span className="board-trigger-actions" onPointerDown={stop}>
        <label className="inline">
          <input
            type="checkbox"
            checked={def.enabled}
            onChange={(e) => {
              submit.mutate({
                intent: {
                  kind: 'update_trigger',
                  id: trigger.id,
                  definition: { ...def, enabled: e.target.checked },
                },
                expectedVersion: trigger.version,
              });
            }}
          />
          {t('triggers.enabled')}
        </label>
        {def.event.type === 'manual' && (
          <button
            type="button"
            disabled={trigger.fired || submit.isPending}
            onClick={() => {
              submit.mutate({ intent: { kind: 'fire_trigger', id: trigger.id, token: null } });
            }}
          >
            {t('triggers.run')}
          </button>
        )}
        <button
          type="button"
          onClick={() => {
            openTrigger(trigger.id);
          }}
        >
          {t('boards.openTrigger')}
        </button>
      </span>
    </div>
  );
}

function NoteEditor({ initial, onSave }: { initial: string; onSave: (markdown: string) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState(initial);
  return (
    <textarea
      className="board-note-editor"
      aria-label={t('boards.noteText')}
      autoFocus
      value={text}
      onPointerDown={(e) => {
        e.stopPropagation();
      }}
      onChange={(e) => {
        setText(e.target.value);
      }}
      onBlur={() => {
        onSave(text);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onSave(initial);
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) onSave(text);
      }}
    />
  );
}

/** A card that opens a map, a journal entry or a sheet; players may not see the target. */
function LinkCard({ target }: { target: BoardLink }) {
  const { t } = useTranslation();
  const name = useSnapshotSelect((s) => {
    switch (target.kind) {
      case 'map':
        return s.maps.find((m) => m.id === target.id)?.name;
      case 'journal':
        return s.journal.find((e) => e.id === target.id)?.title;
      case 'character':
        return s.characters.find((c) => c.id === target.id)?.name;
    }
  });
  // A map link shows the map's picture.
  const picture = useSnapshotSelect((s) => {
    if (target.kind !== 'map') return null;
    const map = s.maps.find((m) => m.id === target.id);
    const asset = s.assets.find((a) => a.id === map?.background_asset_id);
    return asset ? assetUrl(asset.thumbnail_sha256) : null;
  });
  const openMap = useSelection((s) => s.openMap);
  const openJournal = useSelection((s) => s.openJournal);
  const openCharacter = useSelection((s) => s.openCharacter);
  const icon = { map: 'map', journal: 'book', character: 'user' } as const;
  const open = () => {
    if (target.kind === 'map') openMap(target.id);
    else if (target.kind === 'journal') openJournal(target.id);
    else openCharacter(target.id);
  };
  return (
    <button
      type="button"
      className="board-link"
      disabled={name === undefined}
      onPointerDown={(e) => {
        e.stopPropagation();
      }}
      onClick={open}
    >
      {picture && <img className="board-link-picture" src={picture} alt="" draggable={false} />}
      <span className="board-link-caption">
        <Icon name={icon[target.kind]} />
        <span>{name ?? t('boards.unavailable')}</span>
      </span>
    </button>
  );
}

function ImageCard({ assetId }: { assetId: string }) {
  const { t } = useTranslation();
  const asset = useSnapshotSelect((s) => s.assets.find((a) => a.id === assetId));
  if (!asset) return <p className="muted">{t('boards.unavailable')}</p>;
  return (
    <img
      className="board-image"
      src={assetUrl(asset.display_sha256 ?? asset.thumbnail_sha256)}
      alt={asset.original_name ?? ''}
      draggable={false}
    />
  );
}
