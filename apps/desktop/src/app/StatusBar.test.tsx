import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StatusBar } from './StatusBar';
import type { CampaignInfo } from '@/shared/ipc/client';

const state = vi.hoisted(() => ({ plugins: [] as { id: string }[] }));
vi.mock('@tanstack/react-query', () => ({ useQuery: () => ({ data: { version: 'test' } }) }));
vi.mock('@/shared/game/useRemote', () => ({ useRemote: () => null }));
vi.mock('@/shared/game/useGame', () => ({
  useSnapshotSelect: (select: (snapshot: typeof state) => unknown) => select(state),
}));

const campaign: CampaignInfo = { name: 'Test', path: '/test', user_id: 'u1', role: 'gm' };

describe('StatusBar plugin count', () => {
  it('follows the installed plugin membership', () => {
    state.plugins = [{ id: 'rules' }, { id: 'content' }];
    const view = render(<StatusBar campaign={campaign} />);
    expect(screen.getByText('2 plugins')).toBeInTheDocument();
    state.plugins = [];
    view.rerender(<StatusBar campaign={campaign} />);
    expect(screen.getByText('0 plugins')).toBeInTheDocument();
  });
});
