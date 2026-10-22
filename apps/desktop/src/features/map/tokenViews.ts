import type { TokenView } from './TokenLayer';
import type { CampaignInfo, Snapshot, TokenLayer } from '@/shared/ipc/client';

const LAYER_ORDER: Record<TokenLayer, number> = { background: 0, tokens: 1, gm: 2 };

/**
 * The tokens of `mapId` as the viewer sees them, bottom layer first. Players do not see the GM
 * layer or hidden tokens; they may drag tokens of their own characters on a shared map. This
 * mirrors `dnd_game_core::authorize` for the UI only — the core decides for real (§5).
 */
export function tokenViews(snapshot: Snapshot, mapId: string, campaign: CampaignInfo): TokenView[] {
  const isGm = campaign.role === 'gm' || campaign.role === 'co_gm';
  const map = snapshot.maps.find((m) => m.id === mapId);
  if (!map) return [];
  // Whose turn it is in this map's combat (ADR-0020).
  const combat = snapshot.combats.find((c) => c.map_id === mapId);
  const turnToken = combat?.turn
    ? snapshot.combatants.find((c) => c.id === combat.turn)?.token_id
    : null;
  const owned = new Set(
    snapshot.characters.filter((c) => c.owner === campaign.user_id).map((c) => c.id),
  );
  return snapshot.tokens
    .filter((t) => t.map_id === mapId)
    .filter((t) => isGm || (t.layer !== 'gm' && t.visible))
    .sort((a, b) => LAYER_ORDER[a.layer] - LAYER_ORDER[b.layer])
    .map((token) => ({
      token,
      movable:
        isGm ||
        (campaign.role === 'player' &&
          map.visible_to_players &&
          token.layer !== 'gm' &&
          token.character_id !== null &&
          owned.has(token.character_id)),
      gmOnly: token.layer === 'gm' || !token.visible,
      colorKey: token.character_id ?? token.id,
      active: token.id === turnToken,
    }));
}
