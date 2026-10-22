import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GridForm } from './GridForm';
import { MapView } from './MapView';
import type {
  AssetDto,
  CampaignInfo,
  IntentEnvelope,
  MapDto,
  Role,
  Snapshot,
} from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

// jsdom has no WebGL: the host hands over a fake app, the scene records what it is shown.
const scene = vi.hoisted(() => ({
  shown: [] as { map: string | null; background: string | null }[],
  fit: vi.fn(),
  destroy: vi.fn(),
  loadError: null as ((error: unknown) => void) | null,
  rendererError: null as Error | null,
}));

vi.mock('./MapScene', () => ({
  MapScene: class {
    constructor(_app: unknown, onLoadError: (error: unknown) => void) {
      scene.loadError = onLoadError;
    }
    show(map: MapDto | null, background: AssetDto | null) {
      scene.shown.push({ map: map?.id ?? null, background: background?.id ?? null });
    }
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

function setup(node: ReactNode, maps: MapDto[], role: Role = 'gm') {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps,
    tokens: [],
    characters: [],
    users: [],
    journal: [],
    assets: [asset],
    fog: [],
    chat: [],
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
  scene.rendererError = null;
  scene.fit.mockClear();
  scene.destroy.mockClear();
});

afterEach(() => {
  useSelection.setState({ mapId: null });
});

describe('MapView', () => {
  it('shows the selected map with its background on the scene', async () => {
    useSelection.setState({ mapId: id(2) });
    setup(<MapView />, [map(1), map(2, { background_asset_id: asset.id })]);
    await waitFor(() => {
      expect(scene.shown.at(-1)).toEqual({ map: id(2), background: asset.id });
    });
    await userEvent.click(screen.getByRole('button', { name: 'Fit to window' }));
    expect(scene.fit).toHaveBeenCalled();
  });

  it('asks to select a map when none is selected', async () => {
    setup(<MapView />, [map(1)]);
    expect(await screen.findByText('Select or create a map')).toBeInTheDocument();
    expect(scene.shown.at(-1)).toEqual({ map: null, background: null });
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
