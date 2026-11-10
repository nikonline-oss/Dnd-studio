import type { TriggerDef, TriggerDto, ZoneDto, TokenDto } from '@/shared/ipc/client';

/** The same legacy inference as the core; an empty manual/item scope remains global. */
export function triggerMaps(def: TriggerDef, zones: ZoneDto[]): string[] {
  if (def.map_ids?.length) return def.map_ids;
  if (def.event.type !== 'roll_in_zone' && def.event.type !== 'token_entered_zone') return [];
  const zoneId = def.event.zone;
  const zone = zones.find((z) => z.id === zoneId);
  return zone ? [zone.map_id] : [];
}

/** A spent map never disarms other maps. Old spent records remain spent until edited. */
export function canFireTrigger(trigger: TriggerDto, maps: string[], selectedMap?: string): boolean {
  const spent = trigger.fired_maps ?? [];
  if (trigger.fired && spent.length === 0) return false;
  if (maps.length === 0) return !trigger.fired;
  if (selectedMap !== undefined) return maps.includes(selectedMap) && !spent.includes(selectedMap);
  return maps.some((id) => !spent.includes(id));
}

/** UI explanation only: authoritative execution and rights remain in the core. */
export function triggerRunBlock({
  trigger,
  maps,
  tokenId,
  token,
  dirty,
  busy,
}: {
  trigger: TriggerDto;
  maps: string[];
  tokenId: string | null;
  token: Pick<TokenDto, 'id' | 'map_id'> | undefined;
  dirty: boolean;
  busy: boolean;
}):
  | 'busy'
  | 'unsaved'
  | 'disabled'
  | 'map_missing'
  | 'token_missing'
  | 'wrong_map'
  | 'token_not_allowed'
  | 'spent'
  | null {
  if (busy) return 'busy';
  if (dirty) return 'unsaved';
  if (!trigger.definition.enabled) return 'disabled';
  if (trigger.definition.map_ids?.length && maps.length === 0) return 'map_missing';
  if (tokenId && !token) return 'token_missing';
  if (token && maps.length > 0 && !maps.includes(token.map_id)) return 'wrong_map';
  if (
    tokenId &&
    trigger.definition.tokens.length > 0 &&
    !trigger.definition.tokens.includes(tokenId)
  )
    return 'token_not_allowed';
  if (!canFireTrigger(trigger, maps, token?.map_id)) return 'spent';
  return null;
}
