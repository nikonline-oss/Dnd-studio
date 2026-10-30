import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MapList } from './MapList';
import { ZoneList } from './ZoneList';
import { useZoneTool } from './zoneTool';
import { TokenList } from '../tokens/TokenList';
import { useSelection } from '@/shared/state/selection';
const state = vi.hoisted(() => ({
  role: 'gm',
  mutate: vi.fn(),
  snapshot: {
    assets: [],
    maps: [
      { id: 'm', name: 'Map', version: 4, width: 100, height: 100, visible_to_players: false },
    ],
    tokens: [
      {
        id: 't',
        map_id: 'm',
        name: 'Token',
        version: 5,
        visible: true,
        character_id: 'c',
        x: 0,
        y: 0,
      },
    ],
    zones: [{ id: 'z', map_id: 'm', name: 'Zone', version: 6, color: 123 }],
  },
}));
vi.mock('@/shared/game/useGame', () => ({
  useCampaign: () => ({ data: { role: state.role } }),
  useSnapshotSelect: (select: (s: typeof state.snapshot) => unknown) => select(state.snapshot),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
vi.mock('@/shared/game/useRemote', () => ({ useRemote: () => null }));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
vi.mock('@/shared/ui/VirtualList', () => ({
  NAV_ROW_HEIGHT: 30,
  VirtualList: ({
    items,
    renderRow,
  }: {
    items: unknown[];
    renderRow: (item: unknown) => React.ReactNode;
  }) => <ul>{items.map(renderRow)}</ul>,
}));
beforeEach(() => {
  state.role = 'gm';
  state.mutate.mockClear();
  useSelection.setState({ mapId: 'm' });
});
function mount(component: React.ReactNode) {
  return render(<QueryClientProvider client={new QueryClient()}>{component}</QueryClientProvider>);
}
it('confirms map deletion and uses its version', () => {
  mount(<MapList />);
  fireEvent.contextMenu(screen.getByText('Map'));
  fireEvent.click(screen.getByRole('menuitem', { name: 'maps.delete' }));
  expect(state.mutate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'common.confirmDelete' }));
  expect(state.mutate).toHaveBeenCalledWith({
    intent: { kind: 'delete_map', id: 'm' },
    expectedVersion: 4,
  });
});
it('player token menu only selects or opens linked sheet', () => {
  state.role = 'player';
  mount(<TokenList />);
  fireEvent.contextMenu(screen.getByText('Token'));
  expect(screen.getAllByRole('menuitem')).toHaveLength(2);
  fireEvent.click(screen.getByRole('menuitem', { name: 'compendia.openCharacter' }));
  expect(
    useSelection.getState().tabs.some((tab) => tab.kind === 'character' && tab.id === 'c'),
  ).toBe(true);
  expect(state.mutate).not.toHaveBeenCalled();
});
it('renames token using a checked existing intent', () => {
  mount(<TokenList />);
  fireEvent.contextMenu(screen.getByText('Token'));
  fireEvent.click(screen.getByRole('menuitem', { name: 'tokens.rename' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'tokens.rename' }), {
    target: { value: 'Renamed' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'media.save' }));
  expect(state.mutate).toHaveBeenCalledWith({
    intent: {
      kind: 'update_token',
      id: 't',
      name: 'Renamed',
      rotation: null,
      scale: null,
      layer: null,
      visible: null,
    },
    expectedVersion: 5,
  });
});
it('shows background commands and confirms zone deletion', () => {
  const { container } = mount(<ZoneList mapId="m" />);
  fireEvent.contextMenu(container.firstElementChild ?? container);
  expect(screen.getByRole('menuitem', { name: 'zones.new' })).toBeInTheDocument();
  fireEvent.contextMenu(screen.getByText('Zone'));
  fireEvent.click(screen.getByRole('menuitem', { name: 'zones.delete' }));
  expect(state.mutate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'common.confirmDelete' }));
  expect(state.mutate).toHaveBeenCalledWith({
    intent: { kind: 'delete_zone', id: 'z' },
    expectedVersion: 6,
  });
});
it('hides zone management and map mutations for players', () => {
  state.role = 'player';
  const view = mount(<ZoneList mapId="m" />);
  expect(view.container).toBeEmptyDOMElement();
  view.unmount();
  mount(<MapList />);
  fireEvent.contextMenu(screen.getByText('Map'));
  expect(screen.getAllByRole('menuitem')).toHaveLength(1);
  expect(screen.queryByRole('button', { name: 'maps.add' })).not.toBeInTheDocument();
});

it('background creation enables drawing and Co-GM can manage tokens', () => {
  state.role = 'co_gm';
  const view = mount(<ZoneList mapId="m" />);
  fireEvent.contextMenu(view.container.firstElementChild ?? view.container);
  fireEvent.click(screen.getByRole('menuitem', { name: 'zones.new' }));
  expect(useZoneTool.getState().creating).toBe(true);
  view.unmount();
  const tokens = mount(<TokenList />);
  fireEvent.contextMenu(tokens.container.firstElementChild ?? tokens.container);
  fireEvent.click(screen.getByRole('menuitem', { name: 'tokens.add' }));
  expect(state.mutate.mock.calls[0]?.[0] as unknown).toMatchObject({
    intent: { kind: 'create_token', map_id: 'm' },
  });
});
