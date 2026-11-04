import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { mockIPC } from '@tauri-apps/api/mocks';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { ActionWindows } from './ActionWindows';
import type { IntentEnvelope, Role, Snapshot } from '@/shared/ipc/client';

function setup(
  role: Role,
  mode: 'offer' | 'waiting' | 'continue' | 'empty' = 'offer',
  offline = false,
) {
  const sent: IntentEnvelope[] = [];
  const snapshot = {
    sounds: [],
    plugins: [],
    system_plugin: null,
    characters: [
      { id: 'caster', owner: 'caster-owner' },
      { id: 'reactor', owner: 'me' },
    ],
    tokens: [
      { id: 'source', character_id: 'caster', name: 'Mage' },
      { id: 'reaction', character_id: 'reactor', name: 'Mira' },
    ],
    plugin_runtime: {
      action_stacks: [
        {
          id: 'stack',
          map: 'map',
          frames:
            mode === 'empty'
              ? []
              : [
                  {
                    id: 'frame',
                    label: 'Fireball',
                    source: 'source',
                    owner: 'caster',
                    ready: true,
                    deferred: [],
                    awaiting_reactions: mode !== 'continue',
                    offers:
                      mode === 'offer'
                        ? [{ source: 'reaction', label: 'Counterspell', ready: true }]
                        : [],
                  },
                ],
        },
      ],
    },
  } as unknown as Snapshot;
  mockIPC((cmd, args) => {
    if (cmd === 'get_snapshot') return snapshot;
    if (cmd === 'campaign_current')
      return {
        role,
        name: 'C',
        path: '/c',
        user_id: mode === 'continue' || mode === 'waiting' ? 'caster-owner' : 'me',
      };
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
      <ActionWindows />
    </QueryClientProvider>,
  );
  return sent;
}
it('the offered controller accepts an interruption through an intent', async () => {
  const sent = setup('player');
  await userEvent.click(await screen.findByRole('button', { name: 'Use reaction' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({
      kind: 'resolve_action_reaction',
      stack: 'stack',
      frame: 'frame',
      source: 'reaction',
      accept: true,
    });
  });
});
it('a caster cannot skip another players pending reaction', async () => {
  setup('player', 'waiting');
  expect(await screen.findByText('Waiting for participants or the GM')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Continue action' })).not.toBeInTheDocument();
});
it('the owner continues an empty authoritative window', async () => {
  const sent = setup('player', 'continue');
  await userEvent.click(await screen.findByRole('button', { name: 'Continue action' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({
      kind: 'continue_plugin_action',
      stack: 'stack',
      frame: 'frame',
    });
  });
});
it('the co-GM can cancel a chain', async () => {
  const sent = setup('co_gm');
  await userEvent.click(await screen.findByRole('button', { name: 'Cancel chain' }));
  await waitFor(() => {
    expect(sent[0]?.intent).toEqual({ kind: 'cancel_action_stack', stack: 'stack' });
  });
});
it('an observer of a locked map receives no hidden caster details', async () => {
  setup('player', 'empty');
  expect(await screen.findByText('Waiting for participants or the GM')).toBeInTheDocument();
  expect(screen.queryByText('Fireball')).not.toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
it('offline reaction controls are disabled', async () => {
  setup('gm', 'offer', true);
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Use reaction' })).toBeDisabled();
  });
});
