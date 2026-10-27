import type {
  Event,
  EventBatch,
  FogDto,
  JournalEntryDto,
  JournalEntryMeta,
  Snapshot,
} from '@/shared/ipc/client';

/** Chat messages kept in the snapshot (mirrors `CHAT_SNAPSHOT_LEN` in the protocol). */
export const CHAT_SNAPSHOT_LEN = 200;

/**
 * Applies one event to a snapshot. Mirrors `dnd_game_core::evolve`: the client only applies
 * facts decided by the core, it never decides anything itself (ADR-0001).
 */
export function applyEvent(snapshot: Snapshot, event: Event): Snapshot {
  switch (event.kind) {
    case 'campaign_meta_updated':
      return { ...snapshot, campaign: event.campaign };
    case 'map_created':
    case 'map_updated':
      return { ...snapshot, maps: upsert(snapshot.maps, event.map) };
    case 'map_deleted':
      return {
        ...snapshot,
        maps: snapshot.maps.filter((m) => m.id !== event.id),
        tokens: snapshot.tokens.filter((t) => t.map_id !== event.id),
        fog: snapshot.fog.filter((f) => f.map_id !== event.id),
        // The map's combat goes with it.
        ...endCombats(
          snapshot,
          snapshot.combats.filter((c) => c.map_id === event.id).map((c) => c.id),
        ),
      };
    case 'fog_applied': {
      // Fog of an unknown map is ignored; a replayed operation is drawn once (as in the core).
      if (!snapshot.maps.some((m) => m.id === event.map_id)) return snapshot;
      const current = snapshot.fog.find((f) => f.map_id === event.map_id) ?? {
        map_id: event.map_id,
        covered: false,
        ops: [],
      };
      if (current.ops.some((op) => op.id === event.op.id)) return snapshot;
      return {
        ...snapshot,
        fog: upsertFog(snapshot.fog, { ...current, ops: [...current.ops, event.op] }),
      };
    }
    case 'fog_reset':
      if (!snapshot.maps.some((m) => m.id === event.map_id)) return snapshot;
      return {
        ...snapshot,
        fog: upsertFog(snapshot.fog, { map_id: event.map_id, covered: event.covered, ops: [] }),
      };
    case 'token_created':
    case 'token_updated':
      return { ...snapshot, tokens: upsert(snapshot.tokens, event.token) };
    case 'token_moved':
      return {
        ...snapshot,
        tokens: snapshot.tokens.map((t) =>
          t.id === event.id ? { ...t, x: event.x, y: event.y } : t,
        ),
      };
    case 'token_deleted':
      return {
        ...snapshot,
        tokens: snapshot.tokens.filter((t) => t.id !== event.id),
        // Its combatant stays in the fight without a token.
        combatants: snapshot.combatants.map((c) =>
          c.token_id === event.id ? { ...c, token_id: null } : c,
        ),
      };
    case 'combat_started':
    case 'combat_updated':
      return { ...snapshot, combats: upsert(snapshot.combats, event.combat) };
    case 'combat_ended':
      return { ...snapshot, ...endCombats(snapshot, [event.id]) };
    case 'combatant_added':
    case 'combatant_updated':
      return { ...snapshot, combatants: upsert(snapshot.combatants, event.combatant) };
    case 'combatant_removed':
      return { ...snapshot, combatants: snapshot.combatants.filter((c) => c.id !== event.id) };
    case 'character_created':
    case 'character_updated':
      return { ...snapshot, characters: upsert(snapshot.characters, event.character) };
    case 'journal_entry_created':
    case 'journal_entry_updated':
      return { ...snapshot, journal: upsert(snapshot.journal, toMeta(event.entry)) };
    case 'journal_entry_deleted':
      return { ...snapshot, journal: snapshot.journal.filter((e) => e.id !== event.id) };
    case 'journal_folder_created':
    case 'journal_folder_updated':
      return { ...snapshot, journal_folders: upsert(snapshot.journal_folders, event.folder) };
    case 'journal_folder_deleted':
      // The core sends the deletion of everything inside first (ADR-0021).
      return {
        ...snapshot,
        journal_folders: snapshot.journal_folders.filter((f) => f.id !== event.id),
      };
    case 'asset_registered':
      return { ...snapshot, assets: upsert(snapshot.assets, event.asset) };
    case 'chat_message_posted':
      // Live messages have no stored time; show when they arrived.
      return {
        ...snapshot,
        chat: [
          ...snapshot.chat,
          { ...event.message, at_ms: event.message.at_ms ?? Date.now() },
        ].slice(-CHAT_SNAPSHOT_LEN),
      };
    case 'user_joined':
      return { ...snapshot, users: upsert(snapshot.users, event.user) };
    case 'plugin_installed':
      return {
        ...snapshot,
        plugins: [...snapshot.plugins.filter((p) => p.id !== event.plugin.id), event.plugin].sort(
          (a, b) => a.id.localeCompare(b.id),
        ),
      };
    case 'plugin_uninstalled':
      return { ...snapshot, plugins: snapshot.plugins.filter((p) => p.id !== event.id) };
    case 'system_plugin_set':
      return { ...snapshot, system_plugin: event.id };
    case 'board_created':
    case 'board_updated':
      return { ...snapshot, boards: upsert(snapshot.boards, event.board) };
    case 'board_deleted':
      // Its nodes and edges were deleted by earlier events; drop leftovers as the core does.
      return {
        ...snapshot,
        boards: snapshot.boards.filter((b) => b.id !== event.id),
        board_nodes: snapshot.board_nodes.filter((n) => n.board_id !== event.id),
        board_edges: snapshot.board_edges.filter((e) => e.board_id !== event.id),
      };
    case 'board_node_created':
    case 'board_node_updated':
      return { ...snapshot, board_nodes: upsert(snapshot.board_nodes, event.node) };
    case 'board_node_moved':
      return {
        ...snapshot,
        board_nodes: snapshot.board_nodes.map((n) =>
          n.id === event.id ? { ...n, x: event.x, y: event.y } : n,
        ),
      };
    case 'board_node_deleted':
      return {
        ...snapshot,
        board_nodes: snapshot.board_nodes.filter((n) => n.id !== event.id),
        board_edges: snapshot.board_edges.filter((e) => e.from !== event.id && e.to !== event.id),
      };
    case 'board_edge_created':
    case 'board_edge_updated':
      return { ...snapshot, board_edges: upsert(snapshot.board_edges, event.edge) };
    case 'board_edge_deleted':
      return { ...snapshot, board_edges: snapshot.board_edges.filter((e) => e.id !== event.id) };
    case 'redacted':
      // Something the server hides from this recipient: only its sequence number counts.
      return snapshot;
  }
}

/**
 * Applies a batch if it directly follows the snapshot. Returns the snapshot unchanged for an
 * already-applied batch and `null` on a gap (the caller must refetch the snapshot).
 */
export function applyBatch(snapshot: Snapshot, batch: EventBatch): Snapshot | null {
  const last = batch.seq + batch.events.length - 1;
  if (last <= snapshot.seq) return snapshot;
  if (batch.seq !== snapshot.seq + 1) return null;
  const next = batch.events.reduce(applyEvent, snapshot);
  return { ...next, seq: last };
}

/** Combats `ids` removed with all their combatants. */
function endCombats(snapshot: Snapshot, ids: string[]): Pick<Snapshot, 'combats' | 'combatants'> {
  return {
    combats: snapshot.combats.filter((c) => !ids.includes(c.id)),
    combatants: snapshot.combatants.filter((c) => !ids.includes(c.combat_id)),
  };
}

function upsertFog(items: FogDto[], fog: FogDto): FogDto[] {
  const rest = items.filter((f) => f.map_id !== fog.map_id);
  return [...rest, fog].sort((a, b) => a.map_id.localeCompare(b.map_id));
}

function upsert<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex((i) => i.id === item.id);
  if (index === -1) return [...items, item].sort((a, b) => a.id.localeCompare(b.id));
  return items.map((i, n) => (n === index ? item : i));
}

/** The snapshot keeps only the journal index; texts are fetched per entry. */
function toMeta(entry: JournalEntryDto): JournalEntryMeta {
  return {
    id: entry.id,
    folder_id: entry.folder_id,
    title: entry.title,
    sort_order: entry.sort_order,
    visibility: entry.visibility,
    players_can_edit: entry.players_can_edit,
    author: entry.author,
    version: entry.version,
  };
}
