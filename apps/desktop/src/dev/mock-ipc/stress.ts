// Dev-only stress data for the mock (`?mock=gm&stress=1`): many tokens, characters, journal
// entries, maps and chat messages, to measure the UI with "a big campaign".
import type {
  ChatMessageDto,
  JournalEntryDto,
  JournalEntryMeta,
  Snapshot,
} from '@/shared/ipc/client';

export const STRESS = {
  tokens: 500,
  characters: 300,
  journal: 2000,
  folders: 200,
  maps: 50,
  chat: 200,
};

/** Deterministic v7-looking ids for generated entities (prefix keeps kinds apart). */
const id = (prefix: number, n: number) =>
  `0190b${prefix.toString(16)}00-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

/** Adds the stress data to `snapshot`; journal texts are pushed to `journal`. */
export function addStress(snapshot: Snapshot, journal: JournalEntryDto[]): Snapshot {
  const map = snapshot.maps[0];
  if (!map) return snapshot;
  const cell = map.grid.cell_size;
  const cols = Math.floor(map.width / cell);

  const characters = Array.from({ length: STRESS.characters }, (_, n) => ({
    id: id(1, n),
    kind: n % 3 === 0 ? ('pc' as const) : ('npc' as const),
    name: `${n % 3 === 0 ? 'Hero' : 'Goblin'} ${n + 1}`,
    owner: null,
    status: 'active' as const,
    sheet_json: `{"initiative": ${n % 7}}`,
    version: 1,
  }));
  const tokens = Array.from({ length: STRESS.tokens }, (_, n) => ({
    id: id(2, n),
    map_id: map.id,
    character_id: characters[n % characters.length]?.id ?? null,
    asset_id: null,
    name: characters[n % characters.length]?.name ?? `Token ${n}`,
    x: ((n % cols) * cell + cell / 2) * 16,
    y: (Math.floor(n / cols) * cell + cell / 2) * 16,
    rotation: 0,
    scale: 1,
    layer: 'tokens' as const,
    visible: true,
    version: 1,
  }));
  const entries: JournalEntryDto[] = Array.from({ length: STRESS.journal }, (_, n) => ({
    id: id(3, n),
    folder: `Region ${Math.floor(n / 100) + 1}/Place ${(n % STRESS.folders) + 1}`,
    title: `Note ${n + 1}`,
    markdown: `# Note ${n + 1}\n\nGenerated for the stress test.`,
    sort_order: n,
    visibility: n % 4 === 0 ? 'gm' : 'players',
    players_can_edit: false,
    author: null,
    version: 1,
  }));
  journal.push(...entries);
  const meta: JournalEntryMeta[] = entries.map((e) => ({
    id: e.id,
    folder: e.folder,
    title: e.title,
    sort_order: e.sort_order,
    visibility: e.visibility,
    players_can_edit: e.players_can_edit,
    author: e.author,
    version: e.version,
  }));
  const maps = Array.from({ length: STRESS.maps }, (_, n) => ({
    ...map,
    id: id(4, n),
    name: `Dungeon level ${n + 1}`,
    background_asset_id: null,
    visible_to_players: false,
  }));
  const now = Date.now();
  const chat: ChatMessageDto[] = Array.from({ length: STRESS.chat }, (_, n) => ({
    id: id(5, n),
    kind: 'text',
    sender: null,
    sender_name: `Player ${n % 5}`,
    body: `Message ${n + 1}: the party moves on.`,
    roll: null,
    system: null,
    visibility: 'all',
    at_ms: now - (STRESS.chat - n) * 1000,
  }));
  return {
    ...snapshot,
    characters: [...snapshot.characters, ...characters],
    tokens: [...snapshot.tokens, ...tokens],
    journal: [...snapshot.journal, ...meta],
    maps: [...snapshot.maps, ...maps],
    chat: [...snapshot.chat, ...chat].slice(-STRESS.chat),
  };
}
