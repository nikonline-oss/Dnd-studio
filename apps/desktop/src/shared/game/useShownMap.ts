import { useCampaign, useSnapshot } from './useGame';
import { useSelection } from '@/shared/state/selection';
import type { MapDto } from '@/shared/ipc/client';

/**
 * The map the table is on: the selected map tab, or for players without a selection the first
 * map shared with them. Used by the map view and everything that works on "this map".
 */
export function useShownMap(): MapDto | undefined {
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const mapId = useSelection((s) => s.mapId);
  const maps = snapshot?.maps ?? [];
  return (
    maps.find((m) => m.id === mapId) ??
    (campaign?.role === 'player' ? maps.find((m) => m.visible_to_players) : undefined)
  );
}
