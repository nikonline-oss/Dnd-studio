import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { GameClockPanel } from './GameClockPanel';
const state = vi.hoisted(() => ({
  snapshot: {
    campaign: { game_clock: { seconds: 120, version: 3 } },
    plugin_runtime: { action_stacks: [] as object[], movements: [] as object[] },
    roll_requests: [] as { status: string }[],
  },
  mutate: vi.fn(),
}));
vi.mock('./useGame', () => ({
  useSnapshot: () => ({ data: state.snapshot }),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
beforeEach(() => {
  state.mutate.mockClear();
  state.snapshot.campaign.game_clock = { seconds: 120, version: 3 };
  state.snapshot.roll_requests = [];
});
it('reviews the advance before submitting against the clock version', () => {
  render(<GameClockPanel steps={['60']} />);
  fireEvent.click(screen.getByRole('button', { name: '+60 sec' }));
  expect(state.mutate).not.toHaveBeenCalled();
  expect(screen.getByText(/Advance by 60 seconds to 180/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Advance time' }));
  expect(state.mutate).toHaveBeenCalledWith(
    { intent: { kind: 'advance_game_time', seconds: 60 }, expectedVersion: 3 },
    expect.any(Object),
  );
});
it('disables advancement while a manual roll is pending', () => {
  state.snapshot.roll_requests = [{ status: 'pending' }];
  render(<GameClockPanel steps={['6']} />);
  expect(screen.getByRole('button', { name: '+6 sec' })).toBeDisabled();
});
it('removes a stale review when another GM advances the clock', () => {
  const { rerender } = render(<GameClockPanel steps={['60']} />);
  fireEvent.click(screen.getByRole('button', { name: '+60 sec' }));
  state.snapshot.campaign.game_clock = { seconds: 180, version: 4 };
  rerender(<GameClockPanel steps={['60']} />);
  expect(screen.queryByRole('button', { name: 'Advance time' })).not.toBeInTheDocument();
  expect(state.mutate).not.toHaveBeenCalled();
});
