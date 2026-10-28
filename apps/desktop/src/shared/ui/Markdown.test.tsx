import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import { elementLink, linkMarkdown, Markdown } from './Markdown';
import type { Snapshot } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

const id = (n: number) => `0190a000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;

function setup(source: string) {
  const snapshot: Snapshot = {
    seq: 1,
    campaign: { name: 'C', description: '', version: 1 },
    maps: [],
    tokens: [],
    characters: [],
    users: [],
    journal: [
      {
        id: id(1),
        folder_id: null,
        title: 'Tavern',
        sort_order: 0,
        visibility: 'players',
        players_can_edit: false,
        author: null,
        version: 1,
      },
    ],
    journal_folders: [],
    plugins: [],
    system_plugin: null,
    boards: [],
    board_nodes: [],
    board_edges: [],
    zones: [],
    triggers: [],
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
      <Markdown source={source} />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  useSelection.getState().reset();
});

describe('Links in texts', () => {
  it('are written and read back', () => {
    expect(linkMarkdown('journal', id(1), 'The [old] tavern')).toBe(
      `[The old tavern](journal:${id(1)})`,
    );
    expect(elementLink(`map:${id(2)}`)).toEqual({ kind: 'map', id: id(2) });
    expect(elementLink('https://example.com')).toBeNull();
    expect(elementLink(`token:${id(2)}`)).toBeNull();
  });

  it('open another journal entry; hidden or deleted ones stay plain text', async () => {
    setup(`See [the tavern](journal:${id(1)}) and [the secret](journal:${id(9)}).`);
    await userEvent.click(await screen.findByRole('link', { name: 'the tavern' }));
    expect(useSelection.getState().active).toEqual({ kind: 'journal', id: id(1) });
    expect(screen.queryByRole('link', { name: 'the secret' })).not.toBeInTheDocument();
    expect(screen.getByText('the secret')).toBeInTheDocument();
  });
});
