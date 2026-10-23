import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { CenterTabs } from './CenterTabs';
import type { MapDto, Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const map = (id: string, name: string): MapDto => ({
  id,
  name,
  background_asset_id: null,
  width: 100,
  height: 100,
  grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
  visible_to_players: false,
  version: 1,
});

function renderTabs(maps: MapDto[]) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps,
    tokens: [],
    characters: [],
    users: [],
    journal: [
      {
        id: 'j1',
        folder_id: null,
        title: 'Notes',
        sort_order: 0,
        visibility: 'gm',
        players_can_edit: false,
        author: null,
        version: 1,
      },
    ],
    journal_folders: [],
    assets: [],
    fog: [],
    combats: [],
    combatants: [],
    chat: [],
  };
  mockIPC((cmd) => {
    if (cmd === 'get_snapshot') return snapshot;
    throw new Error(`unexpected command ${cmd}`);
  });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CenterTabs />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('CenterTabs', () => {
  it('keeps every opened tab; clicking one does not close the others', async () => {
    useSelection.getState().openMap('m1');
    useSelection.getState().openJournal('j1');
    renderTabs([map('m1', 'Crypt')]);
    await userEvent.click(await screen.findByRole('tab', { name: /Crypt/ }));
    expect(screen.getByRole('tab', { name: /Notes/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Crypt/ })).toHaveAttribute('aria-selected', 'true');
  });

  it('closes a tab with its × button or a middle click', async () => {
    useSelection.getState().openMap('m1');
    useSelection.getState().openJournal('j1');
    renderTabs([map('m1', 'Crypt')]);
    await userEvent.click(await screen.findByRole('button', { name: 'Close Notes' }));
    expect(screen.queryByRole('tab', { name: /Notes/ })).not.toBeInTheDocument();
    await userEvent.pointer({
      keys: '[MouseMiddle]',
      target: screen.getByRole('tab', { name: /Crypt/ }),
    });
    expect(useSelection.getState().tabs).toEqual([]);
  });

  it('drops the tab of a map that no longer exists', async () => {
    useSelection.getState().openMap('gone');
    useSelection.getState().openMap('m1');
    renderTabs([map('m1', 'Crypt')]);
    await waitFor(() => {
      expect(useSelection.getState().tabs).toEqual([{ kind: 'map', id: 'm1' }]);
    });
  });

  it('shows a plain map tab when nothing is open', async () => {
    renderTabs([]);
    expect(await screen.findByRole('tab', { name: 'Map' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
  });
});
