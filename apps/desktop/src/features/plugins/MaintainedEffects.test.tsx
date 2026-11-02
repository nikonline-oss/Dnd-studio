import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { MaintainedEffects } from './MaintainedEffects';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';

function setup(role: Role, owner = 'me', offline = false) {
  const sent: IntentEnvelope[] = [];
  const snapshot = {
    characters: [{ id: 'c', owner }],
    tokens: [{ id: 'source', name: 'Mira' }],
    sounds: [],
    plugin_runtime: {
      maintained: [
        { id: 'effect', owner: 'c', source: 'source', label: 'Shield of Faith', effects: [] },
      ],
    },
  } as unknown as Snapshot;
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current') return { role, user_id: 'me', name: 'C', path: '/c' };
    if (cmd === 'remote_current') return offline ? { status: { state: 'reconnecting' } } : null;
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
      <MaintainedEffects />
    </QueryClientProvider>,
  );
  return sent;
}
it('the owner ends maintenance through an intent', async () => {
  const sent = setup('player');
  expect(await screen.findByText('Shield of Faith')).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: 'End maintenance' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({ kind: 'end_maintained_effect', id: 'effect' });
  });
});
it('the co-GM may end another characters maintenance', async () => {
  const sent = setup('co_gm', 'other');
  await userEvent.click(await screen.findByRole('button', { name: 'End maintenance' }));
  await waitFor(() => {
    expect(sent).toHaveLength(1);
  });
});
it('a player cannot see controls for another owners maintenance', async () => {
  setup('player', 'other');
  expect(await screen.findByText('No maintained effects')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'End maintenance' })).not.toBeInTheDocument();
});
it('maintenance controls are disabled while disconnected', async () => {
  setup('gm', 'other', true);
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'End maintenance' })).toBeDisabled();
  });
});
