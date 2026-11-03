import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ActionSaves } from './ActionSaves';
const state = vi.hoisted(() => ({
  role: 'player',
  mutate: vi.fn(),
  snapshot: {
    characters: [
      { id: 'hero', name: 'Hero', owner: 'me' },
      { id: 'npc', name: 'NPC', owner: null },
    ],
    plugin_runtime: {
      saves: [
        {
          id: 'group',
          damage: 28,
          difficulty: 15,
          action: { label: 'Fireball' },
          targets: [
            { token: 'a', character: 'hero', status: 'pending' },
            { token: 'b', character: 'npc', status: 'pending' },
          ],
        },
      ],
    },
  },
}));
vi.mock('./useGame', () => ({
  useCampaign: () => ({ data: { role: state.role, user_id: 'me' } }),
  useSnapshot: () => ({ data: state.snapshot }),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
beforeEach(() => {
  state.role = 'player';
  state.mutate.mockClear();
});
it('offers a player only their own saving throw', () => {
  render(<ActionSaves />);
  const buttons = screen.getAllByRole('button', { name: 'Roll saving throw' });
  expect(buttons[0]).toBeEnabled();
  expect(buttons[1]).toBeDisabled();
  fireEvent.click(buttons[0] as HTMLElement);
  expect(state.mutate).toHaveBeenCalledWith({
    intent: { kind: 'resolve_action_save', group: 'group', target: 'a' },
  });
  expect(screen.queryByRole('button', { name: 'Cancel remaining saves' })).not.toBeInTheDocument();
});
it('allows a co-GM to resolve NPC saves and cancel remaining targets', () => {
  state.role = 'co_gm';
  render(<ActionSaves />);
  const buttons = screen.getAllByRole('button', { name: 'Roll saving throw' });
  expect(buttons[1]).toBeEnabled();
  fireEvent.click(buttons[1] as HTMLElement);
  expect(state.mutate).toHaveBeenCalledWith({
    intent: { kind: 'resolve_action_save', group: 'group', target: 'b' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Cancel remaining saves' }));
  expect(state.mutate).toHaveBeenLastCalledWith({
    intent: { kind: 'cancel_action_saves', group: 'group' },
  });
});
