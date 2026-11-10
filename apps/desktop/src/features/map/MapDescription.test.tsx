import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MapDescription, MAX_MAP_DESCRIPTION } from './MapDescription';
import type { MapDto } from '@/shared/ipc/client';

const state = vi.hoisted(() => ({
  role: 'gm',
  mutate: vi.fn(),
}));

vi.mock('@/shared/game/useGame', () => ({
  useCampaign: () => ({ data: { role: state.role } }),
  useSubmitIntent: () => ({ mutate: state.mutate, isPending: false, error: null }),
}));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const TEXT = 'The crypt smells of wet stone.';

const map: MapDto = {
  id: 'map-1',
  name: 'Crypt',
  description: TEXT,
  background_asset_id: null,
  width: 4000,
  height: 3000,
  grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
  visible_to_players: true,
  version: 3,
};

beforeEach(() => {
  state.role = 'gm';
  state.mutate.mockClear();
});

describe('MapDescription', () => {
  it('shows the text to players read-only', () => {
    state.role = 'player';
    render(<MapDescription map={map} />);
    expect(screen.getByText(TEXT)).toBeTruthy();
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('renders nothing for players when the map has no text', () => {
    state.role = 'player';
    const { container } = render(<MapDescription map={{ ...map, description: '' }} />);
    expect(container.textContent).toBe('');
  });

  it('does not submit an unchanged description', () => {
    render(<MapDescription map={map} />);
    const save = screen.getByRole('button', { name: 'inspector.descriptionSave' });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    fireEvent.submit(save.closest('form') as HTMLFormElement);
    expect(state.mutate).not.toHaveBeenCalled();
  });

  it('submits a trimmed description with the map version', () => {
    render(<MapDescription map={map} />);
    const box = screen.getByRole('textbox');
    fireEvent.change(box, { target: { value: '  Dust and old bones.  ' } });
    fireEvent.click(screen.getByRole('button', { name: 'inspector.descriptionSave' }));
    expect(state.mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_map',
        id: 'map-1',
        name: null,
        description: 'Dust and old bones.',
        grid: null,
        visible_to_players: null,
      },
      expectedVersion: 3,
    });
  });

  it('accepts an empty description as clearing the text', () => {
    render(<MapDescription map={map} />);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });
    fireEvent.click(screen.getByRole('button', { name: 'inspector.descriptionSave' }));
    expect(state.mutate).toHaveBeenCalledWith({
      intent: {
        kind: 'update_map',
        id: 'map-1',
        name: null,
        description: '',
        grid: null,
        visible_to_players: null,
      },
      expectedVersion: 3,
    });
  });

  it('keeps the same bound as the core', () => {
    render(<MapDescription map={map} />);
    expect(screen.getByRole('textbox').getAttribute('maxlength')).toBe(String(MAX_MAP_DESCRIPTION));
  });
});
