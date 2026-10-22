import { describe, expect, it } from 'vitest';
import { turnOrder } from './turnOrder';
import type { CombatantDto } from '@/shared/ipc/client';

const c = (
  id: string,
  initiative: number | null,
  sortOrder: number,
  combat = 'c1',
): CombatantDto => ({
  id,
  combat_id: combat,
  token_id: null,
  character_id: null,
  name: id,
  initiative,
  sort_order: sortOrder,
  version: 1,
});

describe('turnOrder (mirrors the core)', () => {
  it('sorts by initiative, unrolled last, ties by joining order then id', () => {
    const list = [
      c('late', null, 0),
      c('b', 12, 2),
      c('top', 18, 3),
      c('a', 12, 1),
      c('neg', -1, 4),
      c('z', null, 5),
      c('y', null, 5),
      c('other-combat', 30, 0, 'c2'),
    ];
    expect(turnOrder(list, 'c1').map((x) => x.id)).toEqual([
      'top',
      'a',
      'b',
      'neg',
      'late',
      'y',
      'z',
    ]);
  });
});
