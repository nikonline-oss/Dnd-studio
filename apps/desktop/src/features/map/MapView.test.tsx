import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, createEvent, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GridForm } from './GridForm';
import { MapView } from './MapView';
import { ZoneList } from './ZoneList';
import { useZoneTool } from './zoneTool';
import type { FogTool, TokenMove, ZoneEditCallbacks } from './MapScene';
import type { TokenView } from './TokenLayer';
import type {
  AssetDto,
  CampaignInfo,
  CharacterDto,
  FogDto,
  FogOpKind,
  FogShape,
  IntentEnvelope,
  MapDto,
  Role,
  Snapshot,
  TokenDto,
  ZoneDto,
  ZoneShape,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

// jsdom has no WebGL: the host hands over a fake app, the scene records what it is shown.
const scene = vi.hoisted(() => ({
  shown: [] as {
    map: string | null;
    background: string | null;
    fogOps: number | null;
    gmView: boolean;
  }[],
  tools: [] as (FogTool | null)[],
  onFog: null as ((kind: FogOpKind, shapes: FogShape[]) => void) | null,
  onMove: null as ((move: TokenMove) => void) | null,
  onSelect: null as ((id: string | null) => void) | null,
  onZone: null as ((shape: ZoneShape) => void) | null,
  zones: [] as ZoneDto[][],
  /** The zone selected when zones were last shown. */
  selectedZone: null as string | null,
  editing: [] as boolean[],
  zoneEdit: null as ZoneEditCallbacks | null,
  clearZoneOverride: vi.fn(),
  tokens: [] as TokenView[],
  /** canvasToMap returns the canvas point as is (scale 1, no pan). */
  fit: vi.fn(),
  destroy: vi.fn(),
  loadError: null as ((error: unknown) => void) | null,
  rendererError: null as Error | null,
}));

vi.mock('./MapScene', () => ({
  MapScene: class {
    constructor(
      _app: unknown,
      onLoadError: (error: unknown) => void,
      onFog: (kind: FogOpKind, shapes: FogShape[]) => void,
      onMove: (move: TokenMove) => void,
      onSelect: (id: string | null) => void,
      onZone: (shape: ZoneShape) => void,
      zoneEdit: ZoneEditCallbacks,
    ) {
      scene.zoneEdit = zoneEdit;
      scene.onSelect = onSelect;
      scene.onZone = onZone;
      scene.loadError = onLoadError;
      scene.onFog = onFog;
      scene.onMove = onMove;
    }
    setTokens(views: TokenView[]) {
      scene.tokens = views;
    }
    canvasToMap(x: number, y: number) {
      return { x, y };
    }
    show(map: MapDto | null, background: AssetDto | null, fog: FogDto | null, gmView: boolean) {
      scene.shown.push({
        map: map?.id ?? null,
        background: background?.id ?? null,
        fogOps: fog ? fog.ops.length : null,
        gmView,
      });
    }
    setTool(tool: FogTool | null) {
      scene.tools.push(tool);
    }
    setZones(zones: ZoneDto[], selected: string | null) {
      scene.zones.push(zones);
      scene.selectedZone = selected;
    }
    setZoneEditing(on: boolean) {
      scene.editing.push(on);
    }
    zoneLabelOnCanvas() {
      return { x: 10, y: 20 };
    }
    clearZoneOverride = scene.clearZoneOverride;
    fit = scene.fit;
    destroy = scene.destroy;
  },
}));

vi.mock('@/shared/pixi/PixiHost', async () => {
  const { useEffect } = await import('react');
  return {
    PixiHost: ({
      onReady,
      onError,
    }: {
      onReady: (app: unknown) => () => void;
      onError?: (error: unknown) => void;
    }) => {
      useEffect(() => {
        if (scene.rendererError) {
          onError?.(scene.rendererError);
          return undefined;
        }
        return onReady({});
        // eslint-disable-next-line react-hooks/exhaustive-deps
      }, []);
      return null;
    },
  };
});

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString().padStart(12, '0')}`;

const map = (n: number, extra: Partial<MapDto> = {}): MapDto => ({
  id: id(n),
  name: `Map ${n}`,
  background_asset_id: null,
  width: 4000,
  height: 3000,
  grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
  visible_to_players: false,
  version: 2,
  ...extra,
});

const asset: AssetDto = {
  id: id(50),
  usage: 'map',
  sha256: 'aa'.repeat(32),
  mime: 'image/png',
  size_bytes: 1,
  original_name: null,
  width: 4000,
  height: 3000,
  display_sha256: null,
  thumbnail_sha256: 'bb'.repeat(32),
  tiles: [],
  version: 1,
};

function setup(
  node: ReactNode,
  maps: MapDto[],
  role: Role = 'gm',
  fog: FogDto[] = [],
  extra: Partial<Snapshot> = {},
) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps,
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
    assets: [asset],
    fog,
    combats: [],
    combatants: [],
    chat: [],
    ...extra,
  };
  const info: CampaignInfo = { name: 'C', path: '/c', user_id: id(99), role };
  const sent: IntentEnvelope[] = [];
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'log_frontend') return null;
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 2 };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  render(<QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>);
  return sent;
}

beforeEach(() => {
  scene.shown = [];
  scene.tools = [];
  scene.onFog = null;
  scene.onMove = null;
  scene.tokens = [];
  scene.zones = [];
  scene.editing = [];
  scene.selectedZone = null;
  scene.rendererError = null;
  scene.fit.mockClear();
  scene.destroy.mockClear();
});

afterEach(() => {
  useSelection.setState({ mapId: null, tokenId: null, inspect: 'map' });
});

describe('MapView', () => {
  it('shows the selected map with its background on the scene', async () => {
    useSelection.setState({ mapId: id(2) });
    setup(<MapView />, [map(1), map(2, { background_asset_id: asset.id })]);
    await waitFor(() => {
      expect(scene.shown.at(-1)).toMatchObject({ map: id(2), background: asset.id });
    });
    await userEvent.click(screen.getByRole('button', { name: 'Fit to window' }));
    expect(scene.fit).toHaveBeenCalled();
  });

  it('asks to select a map when none is selected', async () => {
    setup(<MapView />, [map(1)]);
    expect(await screen.findByText('Select or create a map')).toBeInTheDocument();
    expect(scene.shown.at(-1)).toMatchObject({ map: null, background: null });
  });

  it('shows players the first map shared with them', async () => {
    setup(<MapView />, [map(1), map(2, { visible_to_players: true })], 'player');
    await waitFor(() => {
      expect(scene.shown.at(-1)?.map).toBe(id(2));
    });
  });

  it('destroys the scene on unmount', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)]);
    await waitFor(() => {
      expect(scene.shown.at(-1)?.map).toBe(id(1));
    });
    const { cleanup } = await import('@testing-library/react');
    cleanup();
    expect(scene.destroy).toHaveBeenCalledTimes(1);
  });

  it('reports a renderer that cannot start', async () => {
    scene.rendererError = new Error('no webgl');
    setup(<MapView />, [map(1)]);
    expect(await screen.findByRole('alert')).toHaveTextContent('WebGL is unavailable');
  });

  it('reports map images that failed to load', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)]);
    await waitFor(() => {
      expect(scene.loadError).not.toBeNull();
    });
    scene.loadError?.(new Error('404'));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be loaded');
  });
});

describe('MapView fog (ADR-0019)', () => {
  const fog: FogDto = {
    map_id: id(1),
    covered: true,
    ops: [
      {
        id: id(70),
        kind: 'reveal',
        shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
      },
    ],
  };

  it('shows the fog translucent to the GM and opaque to players', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)], 'gm', [fog]);
    await waitFor(() => {
      expect(scene.shown.at(-1)).toMatchObject({ map: id(1), fogOps: 1, gmView: true });
    });
  });

  it('players get no fog tools and no tool on the scene', async () => {
    setup(<MapView />, [map(1, { visible_to_players: true })], 'player', [fog]);
    await waitFor(() => {
      expect(scene.shown.at(-1)).toMatchObject({ map: id(1), gmView: false });
    });
    expect(screen.queryByRole('toolbar', { name: 'Fog' })).not.toBeInTheDocument();
    expect(scene.tools.every((tool) => tool === null)).toBe(true);
  });

  it('draws zones for the GM; players never get them on the scene', async () => {
    useSelection.setState({ mapId: id(1) });
    const altar: ZoneDto = {
      id: id(70),
      map_id: id(1),
      name: 'Altar',
      shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
      version: 1,
    };
    const sent = setup(<MapView />, [map(1)], 'gm', [], { zones: [altar] });
    await userEvent.click(await screen.findByRole('radio', { name: 'Draw a zone' }));
    expect(scene.tools.at(-1)).toMatchObject({ shape: 'zone' });
    await waitFor(() => {
      expect(scene.zones.at(-1)).toEqual([altar]);
    });
    const shape: ZoneShape = { type: 'rect', x: 16, y: 32, width: 320, height: 160 };
    scene.onZone?.(shape);
    await waitFor(() => {
      expect(sent[0]?.intent).toMatchObject({
        kind: 'create_zone',
        map_id: id(1),
        name: 'Zone 2',
        shape,
      });
    });
  });

  it('the zone list redraws a zone, adds a part and merges ticked zones', async () => {
    useSelection.setState({ mapId: id(1) });
    useZoneTool.setState({ drawing: { mode: 'new' }, ticked: [] });
    const zone = (n: number, name: string): ZoneDto => ({
      id: id(n),
      map_id: id(1),
      name,
      shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
      version: 2,
    });
    const sent = setup(
      <>
        <MapView />
        <ZoneList mapId={id(1)} />
      </>,
      [map(1)],
      'gm',
      [],
      { zones: [zone(70, 'Altar'), zone(71, 'Crypt')] },
    );
    const shape: ZoneShape = { type: 'rect', x: 16, y: 32, width: 320, height: 160 };

    // Redraw: the zone tool is taken for one rectangle, which replaces the shape.
    const [redraw] = await screen.findAllByRole('button', { name: 'Redraw' });
    await userEvent.click(redraw as HTMLElement);
    await waitFor(() => {
      expect(scene.tools.at(-1)).toMatchObject({ shape: 'zone' });
    });
    scene.onZone?.(shape);
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 2,
        intent: { kind: 'update_zone', id: id(70), shape },
      });
    });
    expect(useZoneTool.getState().drawing).toEqual({ mode: 'new' });

    // Add a part: a union of the old shape and the new rectangle.
    const [, addPart] = screen.getAllByRole('button', { name: 'Add a part' });
    await userEvent.click(addPart as HTMLElement);
    scene.onZone?.(shape);
    await waitFor(() => {
      expect(sent[1]?.intent).toMatchObject({
        kind: 'update_zone',
        id: id(71),
        shape: { type: 'union', parts: [zone(71, 'Crypt').shape, shape] },
      });
    });

    // Merge: the first ticked zone stays.
    await userEvent.click(screen.getByRole('checkbox', { name: 'Tick Crypt' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'Tick Altar' }));
    await userEvent.click(screen.getByRole('button', { name: 'Merge into “Crypt”' }));
    await waitFor(() => {
      expect(sent[2]).toMatchObject({
        expected_version: 2,
        intent: { kind: 'merge_zones', into: id(71), from: [id(70)] },
      });
    });
  });

  it('zone edit mode: picks a zone from the list, sends moves and the name label, renames in place', async () => {
    useSelection.setState({ mapId: id(1) });
    useZoneTool.setState({ drawing: { mode: 'new' }, ticked: [], editing: false, selected: null });
    const altar: ZoneDto = {
      id: id(70),
      map_id: id(1),
      name: 'Altar',
      shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
      version: 3,
    };
    const sent = setup(
      <>
        <MapView />
        <ZoneList mapId={id(1)} />
      </>,
      [map(1)],
      'gm',
      [],
      { zones: [altar] },
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Edit “Altar” on the map' }));
    await waitFor(() => {
      expect(scene.editing.at(-1)).toBe(true);
    });
    expect(scene.selectedZone).toBe(id(70));
    expect(scene.tools.at(-1)).toBeNull();
    expect(screen.getByRole('radio', { name: 'Edit zones' })).toBeChecked();
    expect(screen.getByRole('status')).toHaveTextContent('Double-click the name');

    // A drag on the map: one update with the new shape and name position.
    const moved: ZoneShape = { type: 'rect', x: 320, y: 160, width: 160, height: 160 };
    act(() => {
      scene.zoneEdit?.onEdit(altar, { shape: moved, label: { x: 400, y: 200 } });
    });
    await waitFor(() => {
      expect(sent[0]).toMatchObject({
        expected_version: 3,
        intent: {
          kind: 'update_zone',
          id: id(70),
          name: null,
          shape: moved,
          label: { x: 400, y: 200 },
        },
      });
    });

    // A double click on the name: renamed in place; Escape leaves it as it was.
    act(() => {
      scene.zoneEdit?.onRename(id(70));
    });
    const field = await screen.findByRole('textbox', { name: 'Zone name' });
    await userEvent.clear(field);
    await userEvent.type(field, 'Old altar{Enter}');
    await waitFor(() => {
      expect(sent[1]?.intent).toMatchObject({ kind: 'update_zone', id: id(70), name: 'Old altar' });
    });
    expect(screen.queryByRole('textbox', { name: 'Zone name' })).not.toBeInTheDocument();
    act(() => {
      scene.zoneEdit?.onRename(id(70));
    });
    await userEvent.type(await screen.findByRole('textbox', { name: 'Zone name' }), 'X{Escape}');
    expect(screen.queryByRole('textbox', { name: 'Zone name' })).not.toBeInTheDocument();

    // Clicking the empty map deselects; another tool ends edit mode.
    act(() => {
      scene.zoneEdit?.onSelect(null);
    });
    await waitFor(() => {
      expect(scene.selectedZone).toBeNull();
    });
    await userEvent.click(screen.getByRole('radio', { name: 'Pan (no fog tool)' }));
    await waitFor(() => {
      expect(scene.editing.at(-1)).toBe(false);
    });
    expect(sent).toHaveLength(2);
  });

  it('players never get zones on the scene', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1, { visible_to_players: true })], 'player', [], {
      zones: [
        {
          id: id(70),
          map_id: id(1),
          name: 'Altar',
          shape: { type: 'rect', x: 0, y: 0, width: 160, height: 160 },
          version: 1,
        },
      ],
    });
    await waitFor(() => {
      expect(scene.zones.length).toBeGreaterThan(0);
    });
    expect(scene.zones.every((z) => z.length === 0)).toBe(true);
  });

  it('selects tools and sends each drawn shape as an apply_fog intent, in order', async () => {
    useSelection.setState({ mapId: id(1) });
    const sent = setup(<MapView />, [map(1)]);
    await userEvent.click(await screen.findByRole('radio', { name: 'Brush' }));
    await userEvent.click(screen.getByRole('radio', { name: 'Hide' }));
    expect(scene.tools.at(-1)).toEqual({ shape: 'brush', kind: 'hide', radius: 40 });

    const shapes: FogShape[] = [
      { type: 'brush', radius: 640, points: [0, 0, 100, 100] },
      { type: 'brush', radius: 640, points: [100, 100, 200, 200] },
    ];
    scene.onFog?.('hide', shapes);
    await waitFor(() => {
      expect(sent).toHaveLength(2);
    });
    expect(sent.map((e) => e.intent)).toMatchObject([
      { kind: 'apply_fog', map_id: id(1), op: { kind: 'hide', shape: shapes[0] } },
      { kind: 'apply_fog', map_id: id(1), op: { kind: 'hide', shape: shapes[1] } },
    ]);
    // Every operation gets its own UUIDv7.
    const ids = sent.map((e) => (e.intent.kind === 'apply_fog' ? e.intent.op.id : ''));
    expect(new Set(ids).size).toBe(2);
    expect(ids[0]).toMatch(/^[0-9a-f-]{14}7/);

    await userEvent.click(screen.getByRole('radio', { name: 'Pan (no fog tool)' }));
    expect(scene.tools.at(-1)).toBeNull();
  });

  it('resets the fog only after confirmation', async () => {
    useSelection.setState({ mapId: id(1) });
    const sent = setup(<MapView />, [map(1)], 'gm', [fog]);
    await userEvent.click(await screen.findByRole('button', { name: 'Remove all fog' }));
    expect(sent).toHaveLength(0);
    await userEvent.click(screen.getByRole('button', { name: 'Remove fog?' }));
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toEqual({ kind: 'reset_fog', map_id: id(1), covered: false });
  });

  it('cannot remove fog from a map that has none', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)]);
    expect(await screen.findByRole('button', { name: 'Remove all fog' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cover the whole map' })).toBeEnabled();
  });
});

describe('MapView tokens', () => {
  const knight: CharacterDto = {
    id: id(51),
    kind: 'npc',
    name: 'Restless Knight',
    owner: null,
    status: 'active',
    sheet_json: '{}',
    version: 1,
  };
  const token: TokenDto = {
    id: id(80),
    map_id: id(1),
    character_id: knight.id,
    asset_id: null,
    name: 'Restless Knight',
    x: 560,
    y: 560,
    rotation: 0,
    scale: 1,
    layer: 'tokens',
    visible: true,
    version: 1,
  };

  it('gives the scene the tokens of the shown map', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)], 'gm', [], {
      tokens: [token, { ...token, id: id(81), map_id: id(2) }],
      characters: [knight],
    });
    await waitFor(() => {
      expect(scene.tokens.map((v) => v.token.id)).toEqual([id(80)]);
    });
    expect(scene.tokens[0]).toMatchObject({ movable: true, colorKey: knight.id });
  });

  it('a clicked token is selected and drawn as selected; the empty map clears it', async () => {
    useSelection.setState({ mapId: id(1) });
    setup(<MapView />, [map(1)], 'gm', [], { tokens: [token] });
    await waitFor(() => {
      expect(scene.onSelect).not.toBeNull();
    });
    act(() => {
      scene.onSelect?.(token.id);
    });
    expect(useSelection.getState()).toMatchObject({ tokenId: token.id, inspect: 'token' });
    await waitFor(() => {
      expect(scene.tokens[0]?.selected).toBe(true);
    });
    act(() => {
      scene.onSelect?.(null);
    });
    expect(useSelection.getState()).toMatchObject({ tokenId: null, inspect: 'map' });
  });

  it('sends dragged positions as last-write-wins move_token intents', async () => {
    useSelection.setState({ mapId: id(1) });
    const sent = setup(<MapView />, [map(1)], 'gm', [], { tokens: [token] });
    await waitFor(() => {
      expect(scene.onMove).not.toBeNull();
    });
    scene.onMove?.({ id: token.id, x: 1120, y: 1680, final: true });
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: null,
      intent: { kind: 'move_token', id: token.id, x: 1120, y: 1680 },
    });
  });

  it('a character dropped on the map becomes a token on the grid cell', async () => {
    useSelection.setState({ mapId: id(1) });
    const sent = setup(<MapView />, [map(1)], 'gm', [], { characters: [knight] });
    const view = document.querySelector('.map-view');
    if (!(view instanceof HTMLElement)) throw new Error('no map view');
    await waitFor(() => {
      expect(scene.onMove).not.toBeNull();
    });
    // GM tools are up once the campaign (role) is loaded.
    await screen.findByRole('toolbar', { name: 'Fog' });
    // The fake scene maps canvas pixels 1:1 to map units; (100, 30) is in cell (1, 0) of 70.
    const data = new Map<string, string>([['application/x-dndstudio-character', knight.id]]);
    const dataTransfer = {
      types: [...data.keys()],
      getData: (type: string) => data.get(type) ?? '',
      dropEffect: 'none',
    };
    fireEvent.dragOver(view, { dataTransfer, clientX: 100, clientY: 30 });
    expect(await screen.findByText('Drop to place the token')).toBeInTheDocument();
    // jsdom has no DragEvent: the generic event it creates carries no coordinates.
    const drop = createEvent.drop(view, { dataTransfer });
    Object.defineProperty(drop, 'clientX', { value: 100 });
    Object.defineProperty(drop, 'clientY', { value: 30 });
    fireEvent(view, drop);
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]?.intent).toMatchObject({
      kind: 'create_token',
      map_id: id(1),
      character_id: knight.id,
      name: 'Restless Knight',
      x: 105 * 16,
      y: 35 * 16,
      layer: 'tokens',
    });
    expect(screen.queryByText('Drop to place the token')).not.toBeInTheDocument();
  });

  it('players cannot drop characters on the map', async () => {
    const sent = setup(<MapView />, [map(1, { visible_to_players: true })], 'player', [], {
      characters: [knight],
    });
    const view = document.querySelector('.map-view');
    if (!(view instanceof HTMLElement)) throw new Error('no map view');
    await waitFor(() => {
      expect(scene.shown.at(-1)?.map).toBe(id(1));
    });
    const dataTransfer = {
      types: ['application/x-dndstudio-character'],
      getData: () => knight.id,
      dropEffect: 'none',
    };
    fireEvent.drop(view, { dataTransfer, clientX: 10, clientY: 10 });
    expect(sent).toHaveLength(0);
  });
});

describe('GridForm', () => {
  it('sends the whole grid with the map version', async () => {
    const sent = setup(<GridForm map={map(1)} />, [map(1)]);
    const apply = screen.getByRole('button', { name: 'Apply grid' });
    expect(apply).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText('Grid'), 'hex_flat');
    const cell = screen.getByLabelText('Cell size');
    await userEvent.clear(cell);
    await userEvent.type(cell, '100');
    await userEvent.type(screen.getByLabelText('Offset X'), '5');
    await userEvent.click(apply);
    await waitFor(() => {
      expect(sent).toHaveLength(1);
    });
    expect(sent[0]).toMatchObject({
      expected_version: 2,
      intent: {
        kind: 'update_map',
        id: id(1),
        name: null,
        visible_to_players: null,
        grid: { kind: 'hex_flat', cell_size: 100, offset_x: 5, offset_y: 0 },
      },
    });
  });

  it.each(['4', '5000', ''])('does not send a cell size of "%s"', async (value) => {
    const sent = setup(<GridForm map={map(1)} />, [map(1)]);
    const cell = screen.getByLabelText('Cell size');
    await userEvent.clear(cell);
    if (value) await userEvent.type(cell, value);
    expect(cell).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('button', { name: 'Apply grid' })).toBeDisabled();
    expect(sent).toHaveLength(0);
  });
});
