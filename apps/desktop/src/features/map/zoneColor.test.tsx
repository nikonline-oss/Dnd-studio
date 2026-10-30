import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ZoneList } from './ZoneList';
const { mutate } = vi.hoisted(() => ({ mutate: vi.fn() }));
vi.mock('@/shared/game/useGame', () => ({
  useCampaign: () => ({ data: { role: 'gm' } }),
  useSnapshotSelect: () => [
    { id: 'zone', map_id: 'map', name: 'Chapel', color: 0x123456, version: 3 },
  ],
  useSubmitIntent: () => ({ mutate, isPending: false, error: null }),
}));
vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }));
beforeEach(() => mutate.mockClear());
it('shows persisted color and submits a versioned color change', () => {
  render(<ZoneList mapId="map" />);
  const picker = screen.getByLabelText('zones.color');
  expect(picker).toHaveValue('#123456');
  fireEvent.change(picker, { target: { value: '#abcdef' } });
  expect(mutate).toHaveBeenCalledWith({
    intent: { kind: 'set_zone_color', id: 'zone', color: 0xabcdef },
    expectedVersion: 3,
  });
});
