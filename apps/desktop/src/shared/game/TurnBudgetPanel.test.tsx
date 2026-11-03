import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';
import { TurnBudgetPanel, TurnBudgetSummary } from './TurnBudgetPanel';
import type { Snapshot, TurnEconomyView } from '@/shared/ipc/client';

const economy: TurnEconomyView = {
  in_combat: true,
  own_turn: true,
  turn_started: true,
  budgets: [
    {
      resource: 'action',
      label: 'Actions',
      refresh: 'own_turn',
      remaining: 2,
      reserved: 0,
      limit: 2,
    },
    {
      resource: 'slot',
      label: 'Spell-slot expenditure',
      refresh: 'every_turn',
      remaining: 0,
      reserved: 1,
      limit: 1,
    },
  ],
};
it('shows available balances, reservation and distinct refresh scopes', () => {
  render(<TurnBudgetSummary economy={economy} />);
  expect(screen.getByText('Your turn')).toBeInTheDocument();
  expect(screen.getByText('At the start of your turn')).toBeInTheDocument();
  expect(screen.getByText('At the start of every turn')).toBeInTheDocument();
  expect(screen.getByText('Reserved: 1')).toBeInTheDocument();
  expect(screen.getByText('2')).toBeInTheDocument();
});
it('explains that turn limits do not apply outside combat', () => {
  render(<TurnBudgetSummary economy={{ ...economy, in_combat: false }} />);
  expect(screen.getByText('Turn limits do not apply outside combat')).toBeInTheDocument();
  expect(screen.queryByText('Reserved: 1')).not.toBeInTheDocument();
});
it('refreshes the authoritative view when only runtime reservations change', async () => {
  let calls = 0;
  const snapshot = {
    sounds: [],
    characters: [{ id: 'hero', sheet_json: '{}', conditions: [] }],
    plugins: [],
    system_plugin: null,
    combats: [],
    effects: [],
    plugin_runtime: { action_stacks: [] },
  } as unknown as Snapshot;
  mockIPC((cmd) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'sheet_view') {
      calls++;
      return {
        economy: {
          ...economy,
          budgets: [
            { ...economy.budgets[1], reserved: calls > 1 ? 1 : 0, remaining: calls > 1 ? 0 : 1 },
          ],
        },
      };
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <TurnBudgetPanel characterId="hero" />
    </QueryClientProvider>,
  );
  await screen.findByText('Spell-slot expenditure');
  expect(screen.queryByText('Reserved: 1')).not.toBeInTheDocument();
  act(() => {
    client.setQueryData(['snapshot'], {
      ...snapshot,
      plugin_runtime: { action_stacks: [{ id: 'reservation' }] },
    });
  });
  await waitFor(() => {
    expect(screen.getByText('Reserved: 1')).toBeInTheDocument();
  });
  expect(calls).toBeGreaterThan(1);
});
