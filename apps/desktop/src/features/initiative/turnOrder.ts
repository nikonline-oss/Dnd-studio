import type { CombatantDto } from '@/shared/ipc/client';

/**
 * Turn order, mirroring `dnd_game_core::turn_order_cmp` (ADR-0020): higher initiative first,
 * combatants without initiative last, ties by joining order, then by id. The core decides whose
 * turn comes next; the UI only shows the same order.
 */
export function turnOrderCmp(a: CombatantDto, b: CombatantDto): number {
  if (a.initiative !== b.initiative) {
    if (a.initiative === null) return 1;
    if (b.initiative === null) return -1;
    return b.initiative - a.initiative;
  }
  if (a.sort_order !== b.sort_order) return a.sort_order - b.sort_order;
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

export function turnOrder(combatants: CombatantDto[], combatId: string): CombatantDto[] {
  return combatants.filter((c) => c.combat_id === combatId).sort(turnOrderCmp);
}
