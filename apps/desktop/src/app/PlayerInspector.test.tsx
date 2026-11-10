import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PlayerInspector } from './PlayerInspector';
import { useSelection } from '@/shared/state/selection';

vi.mock('@/features/characters/CharacterEditor', () => ({
  CharacterEditor: () => <p>character editor</p>,
}));
vi.mock('@/features/map/MapInspector', () => ({
  MapInspector: () => <p>map inspector</p>,
}));
vi.mock('@/features/tokens/TokenInspector', () => ({
  TokenInspector: () => <p>token inspector</p>,
}));
vi.mock('@/features/tokens/TokenList', () => ({
  TokenList: () => <p>token list</p>,
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

beforeEach(() => {
  useSelection.getState().reset();
});

describe('PlayerInspector', () => {
  it('shows the map and its tokens when a map is open', () => {
    act(() => {
      useSelection.getState().openMap('m1');
    });
    render(<PlayerInspector />);
    expect(screen.getByText('map inspector')).toBeInTheDocument();
    expect(screen.getByText('token list')).toBeInTheDocument();
  });

  it('follows a selected token', () => {
    act(() => {
      useSelection.getState().openMap('m1');
    });
    render(<PlayerInspector />);
    act(() => {
      useSelection.getState().selectToken('t1');
    });
    expect(screen.getByText('token inspector')).toBeInTheDocument();
    expect(screen.queryByText('map inspector')).not.toBeInTheDocument();
  });

  it('follows a selected character', () => {
    render(<PlayerInspector />);
    act(() => {
      useSelection.getState().selectCharacter('c1');
    });
    expect(screen.getByText('character editor')).toBeInTheDocument();
  });

  it('says so when there is nothing to inspect', () => {
    render(<PlayerInspector />);
    expect(screen.getByText('inspector.empty')).toBeInTheDocument();
  });

  it('draws no tabs: the panel is driven by the selection', () => {
    act(() => {
      useSelection.getState().openMap('m1');
    });
    render(<PlayerInspector />);
    expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
  });
});
