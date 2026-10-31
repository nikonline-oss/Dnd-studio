import type { TriggerDef, TriggerDto, ZoneDto } from '@/shared/ipc/client';

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
