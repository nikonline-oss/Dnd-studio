// Boards and zones in the browser mock (ADR-0024): a rough stand-in for the core, enough to look at the
// canvas. Dev only, never shipped.
import { uuidv7 } from '@/shared/ipc/uuid';
import type {
  BoardDto,
  BoardEdgeDto,
  BoardNodeDto,
  Event,
  Intent,
  Snapshot,
} from '@/shared/ipc/client';

/** Events for a board intent, `null` for any other intent. */
export function boardEvents(snapshot: Snapshot, intent: Intent): Event[] | null {
  const bump = <T extends { version: number }>(item: T, patch: Partial<T>): T => ({
    ...item,
    ...patch,
    version: item.version + 1,
  });
  switch (intent.kind) {
    case 'create_board': {
      const board: BoardDto = {
        id: intent.id,
        name: intent.name,
        sort_order: snapshot.boards.length,
        visible_to_players: false,
        version: 1,
      };
      return [{ kind: 'board_created', board }];
    }
    case 'update_board': {
      const board = snapshot.boards.find((b) => b.id === intent.id);
      if (!board) return [];
      return [
        {
          kind: 'board_updated',
          board: bump(board, {
            ...(intent.name === null ? {} : { name: intent.name }),
            ...(intent.visible_to_players === null
              ? {}
              : { visible_to_players: intent.visible_to_players }),
          }),
        },
      ];
    }
    case 'delete_board':
      return [
        ...snapshot.board_edges
          .filter((e) => e.board_id === intent.id)
          .map((e): Event => ({ kind: 'board_edge_deleted', id: e.id })),
        ...snapshot.board_nodes
          .filter((n) => n.board_id === intent.id)
          .map((n): Event => ({ kind: 'board_node_deleted', id: n.id })),
        { kind: 'board_deleted', id: intent.id },
      ];
    case 'create_board_node': {
      const node: BoardNodeDto = {
        id: intent.id,
        board_id: intent.board_id,
        content: intent.content,
        x: intent.x,
        y: intent.y,
        width: intent.width,
        height: intent.height,
        visibility: intent.visibility,
        version: 1,
      };
      return [{ kind: 'board_node_created', node }];
    }
    case 'update_board_node': {
      const node = snapshot.board_nodes.find((n) => n.id === intent.id);
      if (!node) return [];
      return [
        {
          kind: 'board_node_updated',
          node: bump(node, {
            ...(intent.content === null ? {} : { content: intent.content }),
            ...(intent.width === null ? {} : { width: intent.width }),
            ...(intent.height === null ? {} : { height: intent.height }),
            ...(intent.visibility === null ? {} : { visibility: intent.visibility }),
          }),
        },
      ];
    }
    case 'move_board_node':
      return [{ kind: 'board_node_moved', id: intent.id, x: intent.x, y: intent.y }];
    case 'delete_board_node':
      return [
        ...snapshot.board_edges
          .filter((e) => e.from === intent.id || e.to === intent.id)
          .map((e): Event => ({ kind: 'board_edge_deleted', id: e.id })),
        { kind: 'board_node_deleted', id: intent.id },
      ];
    case 'create_board_edge': {
      const edge: BoardEdgeDto = {
        id: intent.id,
        board_id: intent.board_id,
        from: intent.from,
        to: intent.to,
        label: intent.label,
        directed: intent.directed,
        visibility: intent.visibility,
        version: 1,
      };
      return [{ kind: 'board_edge_created', edge }];
    }
    case 'update_board_edge': {
      const edge = snapshot.board_edges.find((e) => e.id === intent.id);
      if (!edge) return [];
      return [
        {
          kind: 'board_edge_updated',
          edge: bump(edge, {
            ...(intent.label === null ? {} : { label: intent.label }),
            ...(intent.directed === null ? {} : { directed: intent.directed }),
            ...(intent.visibility === null ? {} : { visibility: intent.visibility }),
          }),
        },
      ];
    }
    case 'delete_board_edge':
      return [{ kind: 'board_edge_deleted', id: intent.id }];
    case 'create_zone':
      return [
        {
          kind: 'zone_created',
          zone: {
            id: intent.id,
            map_id: intent.map_id,
            name: intent.name,
            shape: intent.shape,
            version: 1,
          },
        },
      ];
    case 'update_zone': {
      const zone = snapshot.zones.find((z) => z.id === intent.id);
      if (!zone) return [];
      return [
        {
          kind: 'zone_updated',
          zone: bump(zone, {
            ...(intent.name === null ? {} : { name: intent.name }),
            ...(intent.shape === null ? {} : { shape: intent.shape }),
          }),
        },
      ];
    }
    case 'delete_zone':
      return [{ kind: 'zone_deleted', id: intent.id }];
    case 'merge_zones': {
      const into = snapshot.zones.find((z) => z.id === intent.into);
      if (!into) return [];
      const parts = [into, ...snapshot.zones.filter((z) => intent.from.includes(z.id))].flatMap(
        (z) => (z.shape.type === 'union' ? z.shape.parts : [z.shape]),
      );
      return [
        { kind: 'zone_updated', zone: bump(into, { shape: { type: 'union', parts } }) },
        ...intent.from.map((id): Event => ({ kind: 'zone_deleted', id })),
      ];
    }
    case 'create_trigger':
      return [
        {
          kind: 'trigger_created',
          trigger: { id: intent.id, definition: intent.definition, fired: false, version: 1 },
        },
      ];
    case 'update_trigger': {
      const trigger = snapshot.triggers.find((tr) => tr.id === intent.id);
      if (!trigger) return [];
      return [
        {
          kind: 'trigger_updated',
          trigger: bump(trigger, { definition: intent.definition, fired: false }),
        },
      ];
    }
    case 'delete_trigger':
      return [{ kind: 'trigger_deleted', id: intent.id }];
    case 'fire_trigger': {
      // A rough stand-in for the core: reveals and chat lines only.
      const trigger = snapshot.triggers.find((tr) => tr.id === intent.id);
      if (!trigger) return [];
      const events: Event[] = [
        {
          kind: 'trigger_fired',
          fired: {
            trigger: trigger.id,
            name: trigger.definition.name,
            character: null,
            token: null,
          },
        },
      ];
      for (const { action } of trigger.definition.steps) {
        if (action.type === 'reveal' && action.target.kind === 'node') {
          const node = snapshot.board_nodes.find((n) => n.id === action.target.id);
          if (node) {
            events.push({
              kind: 'board_node_updated',
              node: bump(node, { visibility: 'players' }),
            });
          }
        }
        if (action.type === 'chat') {
          events.push({
            kind: 'chat_message_posted',
            message: {
              id: uuidv7(),
              kind: 'system',
              sender: null,
              sender_name: '',
              body: '',
              roll: null,
              system: { code: 'narration', text: action.text },
              visibility: action.gm_only ? 'gm' : 'all',
              at_ms: null,
            },
          });
        }
      }
      return events;
    }
    default:
      return null;
  }
}

/** A sample board: an investigation with a hidden secret. */
export function sampleBoard(
  id: (n: number) => string,
): Pick<Snapshot, 'boards' | 'board_nodes' | 'board_edges' | 'zones' | 'triggers'> {
  const board = id(70);
  const node = (
    n: number,
    markdown: string,
    x: number,
    y: number,
    visibility: BoardNodeDto['visibility'],
  ): BoardNodeDto => ({
    id: id(n),
    board_id: board,
    content: { type: 'note', markdown },
    x: x * 16,
    y: y * 16,
    width: 220 * 16,
    height: 110 * 16,
    visibility,
    version: 1,
  });
  return {
    boards: [
      { id: board, name: 'Investigation', sort_order: 0, visible_to_players: true, version: 1 },
    ],
    board_nodes: [
      node(71, '**The missing mayor**\n\nLast seen at the old chapel.', 40, 40, 'players'),
      node(72, '**Cult of the Drowned**\n\nMeets under the crypt.', 360, 40, 'players'),
      node(73, '**Secret passage**\n\nBehind the altar, opened by a hidden lever.', 200, 240, 'gm'),
    ],
    zones: [],
    triggers: [],
    board_edges: [
      {
        id: id(74),
        board_id: board,
        from: id(71),
        to: id(72),
        label: 'kidnapped by?',
        directed: true,
        visibility: 'players',
        version: 1,
      },
      {
        id: id(75),
        board_id: board,
        from: id(72),
        to: id(73),
        label: 'uses',
        directed: true,
        visibility: 'gm',
        version: 1,
      },
    ],
  };
}
