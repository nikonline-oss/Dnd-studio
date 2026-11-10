import { useTranslation } from 'react-i18next';
import { CharacterEditor } from '@/features/characters/CharacterEditor';
import { MapInspector } from '@/features/map/MapInspector';
import { TokenInspector } from '@/features/tokens/TokenInspector';
import { TokenList } from '@/features/tokens/TokenList';
import { useSelection } from '@/shared/state/selection';

/**
 * The player's inspector (ADR-0061): one panel that follows what the player picked — a character,
 * a token on the map, or the map itself. Characters are picked in the navigator; a click on a
 * token selects it here, and a click on the map goes back to the map.
 */
export function PlayerInspector() {
  const { t } = useTranslation();
  const inspect = useSelection((s) => s.inspect);
  const tokenId = useSelection((s) => s.tokenId);
  const mapId = useSelection((s) => s.mapId);

  if (inspect === 'character') return <CharacterEditor />;
  if (inspect === 'token') {
    return tokenId ? (
      <TokenInspector />
    ) : (
      <p className="inspector-empty muted">{t('inspector.empty')}</p>
    );
  }
  if (!mapId) return <p className="inspector-empty muted">{t('inspector.empty')}</p>;
  return (
    <>
      <MapInspector />
      <TokenList />
    </>
  );
}
