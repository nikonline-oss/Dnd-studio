import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { PluginSettingsView } from './PluginSettingsView';
import { PluginsView } from './PluginsView';
import { useSelection } from '@/shared/state/selection';
import type { CampaignInfo, IntentEnvelope, PluginDto, Role, Snapshot } from '@/shared/ipc/client';

function setup(role: Role, navigator = false) {
  const plugin = {
    id: 'example',
    version: '1.0.0',
    name: 'Example',
    kind: 'system',
    license: 'MIT',
    system: {
      features: [
        {
          id: 'reach',
          label: 'Reach reactions',
          description: 'Offer before leaving reach',
          default_enabled: true,
        },
      ],
    },
    feature_settings: {},
  } as unknown as PluginDto;
  const snapshot = { plugins: [plugin], system_plugin: 'example' } as Snapshot;
  const sent: IntentEnvelope[] = [];
  const info: CampaignInfo = { role, user_id: 'u1', name: 'C', path: '/c' };
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return info;
    if (cmd === 'remote_current') return null;
    if (cmd === 'plugin_bundled') return [];
    if (cmd === 'submit_intent') {
      sent.push((args as { envelope: IntentEnvelope }).envelope);
      return { status: 'accepted', seq: 1 };
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      {navigator ? <PluginsView /> : <PluginSettingsView pluginId="example" />}
    </QueryClientProvider>,
  );
  return sent;
}

it('opens a plugin in the existing center tab system', async () => {
  useSelection.getState().reset();
  setup('gm', true);
  await userEvent.click(await screen.findByRole('button', { name: 'Example' }));
  expect(useSelection.getState().active).toEqual({ kind: 'plugin', id: 'example' });
});
it('GM changes a campaign rule through an intent', async () => {
  const sent = setup('co_gm');
  const flag = await screen.findByRole('checkbox', { name: 'Reach reactions' });
  expect(flag).toBeChecked();
  expect(flag).toHaveAccessibleDescription('Offer before leaving reach');
  await userEvent.click(flag);
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({
      kind: 'configure_plugin_feature',
      id: 'example',
      feature: 'reach',
      enabled: false,
    });
  });
});
it('players can read campaign rules but cannot change them', async () => {
  const sent = setup('player');
  expect(await screen.findByRole('checkbox', { name: 'Reach reactions' })).toBeDisabled();
  expect(sent).toEqual([]);
});
