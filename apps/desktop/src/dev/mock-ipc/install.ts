// Dev-only stand-in for the Rust side (`?mock=gm|player` in a plain browser), to look at and
// iterate on the UI without a Tauri build. Never shipped: loaded only under `import.meta.env.DEV`.
// It is not a game core: a few intents become events, everything else is accepted and ignored.
import { emit } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import { drawSampleMap } from './sampleMap';
import { turnOrder } from '@/features/initiative/turnOrder';
import { applyEvent } from '@/shared/game/evolve';
import { uuidv7 } from '@/shared/ipc/uuid';
import type {
  CampaignInfo,
  ChatMessageDto,
  Event,
  IntentEnvelope,
  IntentOutcome,
  JournalEntryDto,
  LogRecord,
  Role,
  Snapshot,
} from '@/shared/ipc/client';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
const GM = id(1);
const PLAYER = id(2);
const sha = (n: number) => n.toString(16).padStart(2, '0').repeat(32);

function sampleJournal(): JournalEntryDto[] {
  return [
    {
      id: id(30),
      folder: 'Lore',
      title: 'The Sunken Crypt',
      markdown:
        '# The Sunken Crypt\n\nBeneath the old chapel lies a crypt half-flooded by the river.\n\n- Entrance: collapsed stairs\n- **Guardian:** a restless knight\n',
      sort_order: 0,
      visibility: 'players',
      players_can_edit: false,
      author: GM,
      version: 1,
    },
    {
      id: id(31),
      folder: 'GM notes',
      title: 'Session 3 plan',
      markdown: 'The knight remembers the party if they bring the signet ring.',
      sort_order: 0,
      visibility: 'gm',
      players_can_edit: false,
      author: GM,
      version: 1,
    },
  ];
}

function sampleChat(): ChatMessageDto[] {
  const now = Date.now();
  return [
    {
      id: id(40),
      kind: 'text',
      sender: GM,
      sender_name: 'Game Master',
      body: 'Welcome to the Sunken Crypt. Roll for initiative when ready.',
      roll: null,
      system: null,
      visibility: 'all',
      at_ms: now - 120_000,
    },
    {
      id: id(41),
      kind: 'roll',
      sender: PLAYER,
      sender_name: 'Mira',
      body: '1d20+3',
      roll: {
        expression: '1d20 + 3',
        total: 17,
        terms: [
          { negative: false, notation: '1d20', subtotal: 14, dice: [{ value: 14, kept: true }] },
          { negative: false, notation: '3', subtotal: 3, dice: [] },
        ],
      },
      system: null,
      visibility: 'all',
      at_ms: now - 60_000,
    },
    {
      id: id(42),
      kind: 'system',
      sender: null,
      sender_name: '',
      body: '',
      roll: null,
      system: { code: 'character_submitted', name: 'Mira' },
      visibility: 'all',
      at_ms: now - 30_000,
    },
  ];
}

/** `open: false` starts on the start screen (no campaign open). */
export async function installMockIpc(role: Role, open = true): Promise<void> {
  const image = await drawSampleMap();
  const files = new Map<string, string>([
    [sha(0xa1), image.tile],
    [sha(0xee), image.thumbnail],
  ]);
  const journal = sampleJournal();
  let seq = 10;
  let snapshot: Snapshot = {
    seq,
    campaign: { name: 'Curse of the Lost Mine', description: '', version: 1 },
    maps: [
      {
        id: id(10),
        name: 'Sunken Crypt',
        background_asset_id: id(20),
        width: image.width,
        height: image.height,
        grid: { kind: 'square', cell_size: 64, offset_x: 0, offset_y: 0 },
        visible_to_players: true,
        version: 1,
      },
      {
        id: id(11),
        name: 'Old Chapel',
        background_asset_id: null,
        width: 4000,
        height: 3000,
        grid: { kind: 'hex_flat', cell_size: 80, offset_x: 0, offset_y: 0 },
        visible_to_players: false,
        version: 1,
      },
    ],
    // The restless knight waits in the flooded hall (Units16, cell centre on a 64-unit grid).
    tokens: [
      {
        id: id(80),
        map_id: id(10),
        character_id: id(51),
        asset_id: null,
        name: 'Restless Knight',
        x: (17 * 64 + 32) * 16,
        y: (15 * 64 + 32) * 16,
        rotation: 0,
        scale: 1,
        layer: 'tokens',
        visible: true,
        version: 1,
      },
    ],
    characters: [
      {
        id: id(50),
        kind: 'pc',
        name: 'Mira',
        owner: PLAYER,
        status: 'pending_approval',
        sheet_json: '{"hp":{"max":24,"value":24},"str_mod":3}',
        version: 1,
      },
      {
        id: id(51),
        kind: 'npc',
        name: 'Restless Knight',
        owner: null,
        status: 'active',
        sheet_json: '{"hp":{"max":45,"value":45}}',
        version: 1,
      },
    ],
    users: [
      { id: GM, name: 'Game Master', role: 'gm' },
      { id: PLAYER, name: 'Mira', role: 'player' },
    ],
    journal: journal.map((e) => ({
      id: e.id,
      folder: e.folder,
      title: e.title,
      sort_order: e.sort_order,
      visibility: e.visibility,
      players_can_edit: e.players_can_edit,
      author: e.author,
      version: e.version,
    })),
    assets: [
      {
        id: id(20),
        usage: 'map',
        sha256: sha(0x01),
        mime: 'image/png',
        size_bytes: 123_456,
        original_name: 'sunken-crypt.png',
        width: image.width,
        height: image.height,
        display_sha256: null,
        thumbnail_sha256: sha(0xee),
        tiles: [{ x: 0, y: 0, width: image.width, height: image.height, sha256: sha(0xa1) }],
        version: 1,
      },
    ],
    // The crypt starts covered with its entrance hall revealed (Units16).
    fog: [
      {
        map_id: id(10),
        covered: true,
        ops: [
          {
            id: id(60),
            kind: 'reveal',
            shape: { type: 'rect', x: 160 * 16, y: 160 * 16, width: 512 * 16, height: 384 * 16 },
          },
        ],
      },
    ],
    combats: [],
    combatants: [],
    chat: sampleChat(),
  };
  const info: CampaignInfo = {
    name: snapshot.campaign.name,
    path: 'C:\\Campaigns\\curse-of-the-lost-mine',
    user_id: role === 'player' ? PLAYER : GM,
    role,
  };
  const logs: LogRecord[] = [
    { ts_ms: Date.now(), level: 'info', target: 'mock', message: 'mock IPC installed' },
  ];

  const commit = async (events: Event[], cause: string): Promise<IntentOutcome> => {
    seq += 1;
    for (const event of events) snapshot = applyEvent(snapshot, event);
    snapshot = { ...snapshot, seq: seq + events.length - 1 };
    await emit('game-events-emitted', { seq, caused_by: cause, events });
    seq += events.length - 1;
    return { status: 'accepted', seq };
  };

  const submit = async ({ intent, intent_id }: IntentEnvelope): Promise<IntentOutcome> => {
    switch (intent.kind) {
      case 'create_map':
        return commit(
          [
            {
              kind: 'map_created',
              map: {
                id: intent.id,
                name: intent.name,
                background_asset_id: intent.background_asset_id,
                width: intent.width,
                height: intent.height,
                grid: intent.grid,
                visible_to_players: intent.visible_to_players,
                version: 1,
              },
            },
          ],
          intent_id,
        );
      case 'update_map': {
        const map = snapshot.maps.find((m) => m.id === intent.id);
        if (!map) return { status: 'rejected', reason: { code: 'not_found' } };
        const updated = {
          ...map,
          name: intent.name ?? map.name,
          grid: intent.grid ?? map.grid,
          visible_to_players: intent.visible_to_players ?? map.visible_to_players,
          version: map.version + 1,
        };
        return commit([{ kind: 'map_updated', map: updated }], intent_id);
      }
      case 'create_token':
        return commit(
          [
            {
              kind: 'token_created',
              token: {
                id: intent.id,
                map_id: intent.map_id,
                character_id: intent.character_id,
                asset_id: intent.asset_id,
                name: intent.name,
                x: intent.x,
                y: intent.y,
                rotation: intent.rotation,
                scale: intent.scale,
                layer: intent.layer,
                visible: intent.visible,
                version: 1,
              },
            },
          ],
          intent_id,
        );
      case 'move_token':
        return commit(
          [{ kind: 'token_moved', id: intent.id, x: intent.x, y: intent.y }],
          intent_id,
        );
      // Combat: a rough stand-in for the core (ADR-0020), enough to look at the tracker.
      case 'start_combat':
        return commit(
          [
            {
              kind: 'combat_started',
              combat: { id: intent.id, map_id: intent.map_id, round: 1, turn: null, version: 1 },
            },
          ],
          intent_id,
        );
      case 'add_combatant': {
        const order = snapshot.combatants.filter((c) => c.combat_id === intent.combat_id).length;
        return commit(
          [
            {
              kind: 'combatant_added',
              combatant: {
                id: intent.id,
                combat_id: intent.combat_id,
                token_id: intent.token_id,
                character_id: intent.character_id,
                name: intent.name,
                initiative: null,
                sort_order: order,
                version: 1,
              },
            },
          ],
          intent_id,
        );
      }
      case 'roll_initiative':
      case 'set_initiative': {
        const c = snapshot.combatants.find((x) => x.id === intent.id);
        if (!c) return { status: 'rejected', reason: { code: 'not_found' } };
        const die = 1 + Math.floor(Math.random() * 20);
        const value = intent.kind === 'set_initiative' ? intent.initiative : die;
        const events: Event[] = [
          {
            kind: 'combatant_updated',
            combatant: { ...c, initiative: value, version: c.version + 1 },
          },
        ];
        if (intent.kind === 'roll_initiative') {
          events.push({
            kind: 'chat_message_posted',
            message: {
              id: uuidv7(),
              kind: 'roll',
              sender: info.user_id,
              sender_name: c.name,
              body: '1d20',
              roll: {
                expression: '1d20',
                total: die,
                terms: [
                  {
                    negative: false,
                    notation: '1d20',
                    subtotal: die,
                    dice: [{ value: die, kept: true }],
                  },
                ],
              },
              system: null,
              visibility: 'all',
              at_ms: Date.now(),
            },
          });
        }
        return commit(events, intent_id);
      }
      case 'next_turn': {
        const combat = snapshot.combats.find((x) => x.id === intent.combat_id);
        if (!combat) return { status: 'rejected', reason: { code: 'not_found' } };
        const order = turnOrder(snapshot.combatants, combat.id);
        const first = order[0];
        if (!first) return { status: 'rejected', reason: { code: 'invalid', field: 'combatants' } };
        const pos = order.findIndex((c) => c.id === combat.turn);
        const next = pos === -1 ? first : (order[pos + 1] ?? first);
        const round = pos !== -1 && pos === order.length - 1 ? combat.round + 1 : combat.round;
        return commit(
          [
            {
              kind: 'combat_updated',
              combat: { ...combat, turn: next.id, round, version: combat.version + 1 },
            },
          ],
          intent_id,
        );
      }
      case 'remove_combatant':
        return commit([{ kind: 'combatant_removed', id: intent.id }], intent_id);
      case 'end_combat':
        return commit([{ kind: 'combat_ended', id: intent.id }], intent_id);
      case 'update_token': {
        const tok = snapshot.tokens.find((x) => x.id === intent.id);
        if (!tok) return { status: 'rejected', reason: { code: 'not_found' } };
        return commit(
          [
            {
              kind: 'token_updated',
              token: {
                ...tok,
                name: intent.name ?? tok.name,
                rotation: intent.rotation ?? tok.rotation,
                scale: intent.scale ?? tok.scale,
                layer: intent.layer ?? tok.layer,
                visible: intent.visible ?? tok.visible,
                version: tok.version + 1,
              },
            },
          ],
          intent_id,
        );
      }
      case 'delete_token':
        return commit([{ kind: 'token_deleted', id: intent.id }], intent_id);
      case 'apply_fog':
        return commit([{ kind: 'fog_applied', map_id: intent.map_id, op: intent.op }], intent_id);
      case 'reset_fog':
        return commit(
          [{ kind: 'fog_reset', map_id: intent.map_id, covered: intent.covered }],
          intent_id,
        );
      case 'delete_map':
        return commit([{ kind: 'map_deleted', id: intent.id }], intent_id);
      case 'send_message':
        return commit(
          [
            {
              kind: 'chat_message_posted',
              message: {
                id: intent.id,
                kind: 'text',
                sender: info.user_id,
                sender_name: role === 'player' ? 'Mira' : 'Game Master',
                body: intent.body,
                roll: null,
                system: null,
                visibility: 'all',
                at_ms: Date.now(),
              },
            },
          ],
          intent_id,
        );
      default:
        return { status: 'accepted', seq };
    }
  };

  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case 'app_info':
          return { version: '0.0.1-mock' };
        case 'campaign_current':
          return open ? info : null;
        case 'get_snapshot':
          return snapshot;
        case 'submit_intent':
          return submit(payload.envelope as IntentEnvelope);
        case 'journal_entry':
          return journal.find((e) => e.id === payload.id);
        case 'chat_history':
          return [];
        case 'logs_recent':
          return logs;
        case 'log_frontend':
          console.info('[log_frontend]', payload.level, payload.message);
          return null;
        case 'campaign_checkpoint':
        case 'update_check':
          return null;
        case 'asset_import_map':
          // The real command opens a native file dialog; here it behaves as if cancelled.
          return null;
        default:
          throw new Error(`mock IPC: ${cmd} is not mocked`);
      }
    },
    { shouldMockEvents: true },
  );

  // `convertFileSrc` serves derived images by hash; here they are generated data URLs.
  const internals = (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> })
    .__TAURI_INTERNALS__;
  internals.convertFileSrc = (hash: string) => files.get(hash) ?? '';
}
