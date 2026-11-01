import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { RollStatistics } from './RollStatistics';

function mount(result: () => unknown) {
  mockIPC((command) => {
    if (command === 'campaign_current')
      return { name: 'C', path: '/c', user_id: 'me', role: 'player' };
    if (command === 'roll_statistics') return result();
    throw new Error(`Unexpected command ${command}`);
  });
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <RollStatistics />
    </QueryClientProvider>,
  );
}

describe('roll statistics', () => {
  it('shows authorized totals and refreshes on demand', async () => {
    let rolls = 3;
    mount(() => [
      {
        user_id: 'me',
        name: 'Aria',
        rolls,
        average: 12.5,
        minimum: 1,
        maximum: 25,
        natural_ones: 1,
        natural_twenties: 2,
      },
    ]);
    const row = await screen.findByRole('row', { name: /Aria/ });
    expect(within(row).getByText('12.5')).toBeInTheDocument();
    expect(within(row).getByText('3')).toBeInTheDocument();
    rolls = 4;
    await userEvent.click(screen.getByRole('button', { name: 'Refresh statistics' }));
    expect(await within(row).findByText('4')).toBeInTheDocument();
    expect(screen.getByText(/Kept d20 dice/)).toBeInTheDocument();
  });
  it('explains an empty history', async () => {
    mount(() => []);
    expect(await screen.findByText('No visible rolls yet.')).toBeInTheDocument();
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
  });
  it('shows an error and allows retrying', async () => {
    let failed = true;
    mount(() => {
      if (failed) throw new Error('Offline');
      return [];
    });
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    failed = false;
    await userEvent.click(screen.getByRole('button', { name: 'Refresh statistics' }));
    expect(await screen.findByText('No visible rolls yet.')).toBeInTheDocument();
  });
});
