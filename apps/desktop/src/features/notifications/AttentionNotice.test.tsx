import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { AttentionNotice } from './AttentionNotice';
import { useAttention } from '@/shared/state/attention';
import { useSelection } from '@/shared/state/selection';
const data = vi.hoisted(() => ({
  role: 'player',
  mutate: vi.fn(),
  tone: vi.fn(),
  snapshot: {
    characters: [
      { id: 'mine', owner: 'me', name: 'Aria' },
      { id: 'other', owner: 'you', name: 'Secret NPC' },
    ],
    roll_requests: [
      { id: 'request', character: 'mine', label: 'Perception', status: 'pending', version: 3 },
      { id: 'secret', character: 'other', label: 'Secret trap', status: 'pending', version: 1 },
    ],
    combats: [{ id: 'combat', map_id: 'map', round: 2, turn: 'turn' }],
    combatants: [
      { id: 'turn', combat_id: 'combat', character_id: 'mine', token_id: 'token', name: 'Aria' },
    ],
    tokens: [],
  },
}));
vi.mock('@/shared/game/useGame', () => ({
  useCampaign: () => ({ data: { user_id: 'me', role: data.role } }),
  useSnapshotSelect: (select: (s: typeof data.snapshot) => unknown) => select(data.snapshot),
  useSubmitIntent: () => ({ mutate: data.mutate, isPending: false, error: null }),
}));
vi.mock('./attentionSound', () => ({
  playAttentionTone: () => {
    data.tone();
  },
}));
afterEach(() => {
  data.role = 'player';
  vi.clearAllMocks();
  useAttention.setState({ requestsOpen: false, soundEnabled: true });
});
it('shows only own pending requests and fulfills the checked request', () => {
  render(<AttentionNotice />);
  expect(screen.getByText('Perception')).toBeInTheDocument();
  expect(screen.queryByText('Secret trap')).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Roll' }));
  expect(data.mutate).toHaveBeenCalledWith({
    intent: { kind: 'fulfill_roll_request', id: 'request' },
    expectedVersion: 3,
  });
});
it('does not sound repeatedly for the same snapshot and can mute notices', () => {
  const view = render(<AttentionNotice />);
  expect(data.tone).toHaveBeenCalledTimes(1);
  view.rerender(<AttentionNotice />);
  expect(data.tone).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: 'Mute notification sounds' }));
  expect(useAttention.getState().soundEnabled).toBe(false);
});
it('opens the map and selects the token whose turn it is', () => {
  render(<AttentionNotice />);
  fireEvent.click(screen.getByRole('button', { name: 'Show my turn' }));
  expect(useSelection.getState().active).toEqual({ kind: 'map', id: 'map' });
  expect(useSelection.getState().tokenId).toBe('token');
});
it('never prompts spectators to roll', () => {
  data.role = 'spectator';
  const view = render(<AttentionNotice />);
  expect(view.container).toBeEmptyDOMElement();
  expect(data.tone).not.toHaveBeenCalled();
});
