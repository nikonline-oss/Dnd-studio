import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it } from 'vitest';
import type { PluginDto, Snapshot } from '@/shared/ipc/client';
import { queryKeys } from '@/shared/game/useGame';
import { defaultSettings, useSettings } from '@/shared/settings/settings';
import { CampaignThemeSync } from './CampaignThemeSync';
import { ThemePluginMenu } from './ThemePluginMenu';

const palette = { light: { '--color-bg': '#abc' }, dark: { '--color-bg': '#123' } };
const plugin: PluginDto & { theme: typeof palette } = {
  id: 'forest',
  name: 'Forest',
  version: '1.0.0',
  kind: 'theme',
  license: 'MIT',
  archive_sha256: 'a'.repeat(64),
  attribution: null,
  system: null,
  theme: palette,
};
const snapshot: Snapshot = {
  seq: 1,
  campaign: { name: 'C', description: '', version: 1 },
  plugins: [plugin],
  system_plugin: null,
  maps: [],
  tokens: [],
  characters: [],
  users: [],
  journal: [],
  journal_folders: [],
  assets: [],
  fog: [],
  combats: [],
  combatants: [],
  chat: [],
  boards: [],
  board_nodes: [],
  board_edges: [],
  zones: [],
  triggers: [],
};
afterEach(() => {
  useSettings.setState(defaultSettings);
  document.documentElement.removeAttribute('style');
});
function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
  client.setQueryData(queryKeys.snapshot, snapshot);
  const rendered = render(
    <QueryClientProvider client={client}>
      <CampaignThemeSync />
      <ThemePluginMenu />
    </QueryClientProvider>,
  );
  return { ...rendered, client };
}
describe('campaign theme selection', () => {
  it('selects and resets a local palette without a game command', async () => {
    useSettings.setState({ theme: 'light' });
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Palette' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: 'Forest' }));
    expect(useSettings.getState().themePluginId).toBe('forest');
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#abc');
    await userEvent.click(screen.getByRole('button', { name: 'Palette' }));
    await userEvent.click(screen.getByRole('menuitemradio', { name: 'Default palette' }));
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
  });
  it('cleans the palette after uninstall and campaign unmount', async () => {
    useSettings.setState({ theme: 'dark', themePluginId: 'forest' });
    const { client, unmount } = setup();
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#123');
    act(() => {
      client.setQueryData(queryKeys.snapshot, { ...snapshot, plugins: [] });
    });
    await waitFor(() => {
      expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
    });
    act(() => {
      client.setQueryData(queryKeys.snapshot, snapshot);
    });
    await waitFor(() => {
      expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('#123');
    });
    unmount();
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
  });
  it('does not apply a remembered selection before campaign data arrives', () => {
    useSettings.setState({ themePluginId: 'forest' });
    const client = new QueryClient({ defaultOptions: { queries: { enabled: false } } });
    render(
      <QueryClientProvider client={client}>
        <CampaignThemeSync />
      </QueryClientProvider>,
    );
    expect(document.documentElement.style.getPropertyValue('--color-bg')).toBe('');
  });
});
