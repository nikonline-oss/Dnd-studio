import type { ItemStateDef } from '@/shared/ipc/client';

/**
 * The states a campaign starts with (ADR-0062). The core resolves them on every read, but the
 * snapshot only carries the campaign's own list when it differs, so the client needs the same
 * fallback — otherwise a campaign that never edited its states would offer an empty dropdown.
 */
export function builtinItemStates(): ItemStateDef[] {
  const label = (ru: string, en: string) => ({ ru, en });
  return [
    { id: 'ok', label: label('Целый', 'Intact'), active: true },
    { id: 'broken', label: label('Сломан', 'Broken'), active: false },
    { id: 'fragment', label: label('Обломок', 'Fragment'), active: false },
    { id: 'activated', label: label('Активирован', 'Activated'), active: true },
    { id: 'deactivated', label: label('Деактивирован', 'Deactivated'), active: false },
  ];
}

/** The states an item may be in: the campaign's own list or the built-in one. */
export function campaignItemStates(states: ItemStateDef[] | undefined): ItemStateDef[] {
  return states && states.length > 0 ? states : builtinItemStates();
}
