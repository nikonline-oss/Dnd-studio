import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { emit } from '@tauri-apps/api/event';
import { mockIPC } from '@tauri-apps/api/mocks';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from './AppShell';
import type { CampaignInfo, Snapshot } from '@/shared/ipc/client';
import { defaultSettings, useSettings } from '@/shared/settings/settings';
import { ErrorBoundary } from '@/shared/ui/ErrorBoundary';

const snapshot: Snapshot = {
  seq: 0,
  campaign: { name: 'Lost Mine', description: '', version: 1 },
  maps: [],
  tokens: [],
  characters: [],
};

const campaign = (role: CampaignInfo['role']): CampaignInfo => ({
  name: 'Lost Mine',
  path: '/games/Lost Mine',
  user_id: '0190a000-0000-7000-8000-000000000001',
  role,
});

function renderShell(role: CampaignInfo['role']) {
  mockIPC(
    (cmd) => {
      if (cmd === 'get_snapshot') return snapshot;
      if (cmd === 'app_info') return { version: '0.0.1' };
      if (cmd === 'logs_recent') return [];
      throw new Error(`unexpected command ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <AppShell campaign={campaign(role)} />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  useSettings.setState(defaultSettings);
  localStorage.clear();
});

describe('AppShell', () => {
  it('GM layout has navigator, inspector, breadcrumbs and status bar', async () => {
    renderShell('gm');
    expect(screen.getByRole('heading', { name: 'Maps' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Inspector' })).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Breadcrumbs' })).toHaveTextContent('Lost Mine');
    const status = screen.getByRole('contentinfo', { name: 'Status bar' });
    expect(status).toHaveTextContent('Local game');
    expect(await screen.findByText('v0.0.1')).toBeInTheDocument();
  });

  it('status bar follows autosave events', async () => {
    renderShell('gm');
    expect(await screen.findByText('Saved')).toBeInTheDocument();
    await act(() => emit('save-status-changed', { dirty: true, error: null }));
    expect(await screen.findByText('Unsaved changes')).toBeInTheDocument();
    await act(() => emit('save-status-changed', { dirty: true, error: 'disk full' }));
    expect(await screen.findByText('Save failed')).toHaveAttribute('title', 'disk full');
  });

  it('GM can switch the bottom panel to logs', async () => {
    renderShell('gm');
    await userEvent.click(screen.getByRole('tab', { name: 'Logs' }));
    expect(screen.getByRole('region', { name: 'Logs' })).toBeInTheDocument();
  });

  it('players always get the player preset (map + chat only)', () => {
    renderShell('player');
    expect(screen.queryByRole('heading', { name: 'Maps' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Inspector' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Chat' })).toBeInTheDocument();
  });

  it('View menu switches theme and language', async () => {
    renderShell('gm');
    await userEvent.click(screen.getByRole('button', { name: 'View' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: /Dark theme/ }));
    expect(useSettings.getState().theme).toBe('dark');

    await userEvent.click(screen.getByRole('button', { name: 'View' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: /Русский/ }));
    expect(useSettings.getState().language).toBe('ru');
  });
});

describe('ErrorBoundary', () => {
  it('contains a crashing feature and recovers on retry', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let crash = true;
    function Flaky() {
      if (crash) throw new Error('boom');
      return <p>recovered</p>;
    }
    render(
      <>
        <ErrorBoundary name="Maps">
          <Flaky />
        </ErrorBoundary>
        <p>sibling</p>
      </>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('“Maps” crashed');
    expect(screen.getByText('sibling')).toBeInTheDocument();
    crash = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(screen.getByText('recovered')).toBeInTheDocument();
  });
});
