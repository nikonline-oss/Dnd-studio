import { useTranslation } from 'react-i18next';
import { useSnapshot } from '@/shared/game/useGame';
import { useSelection } from '@/shared/state/selection';

/** Inspector panel: properties of the selected map (read-only until the map view lands). */
export function MapInspector() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const mapId = useSelection((s) => s.mapId);
  const map = snapshot?.maps.find((m) => m.id === mapId);

  if (!map) return <p className="muted">{t('inspector.empty')}</p>;
  const tokenCount = snapshot?.tokens.filter((tok) => tok.map_id === map.id).length ?? 0;

  return (
    <dl className="properties">
      <dt>{t('inspector.name')}</dt>
      <dd>{map.name}</dd>
      <dt>{t('inspector.grid')}</dt>
      <dd>
        {t(`inspector.gridKind.${map.grid.kind}`)}, {map.grid.cell_size}px
      </dd>
      <dt>{t('inspector.visibility')}</dt>
      <dd>{map.visible_to_players ? t('maps.visible') : t('maps.hidden')}</dd>
      <dt>{t('inspector.tokens')}</dt>
      <dd>{tokenCount}</dd>
      <dt>{t('inspector.version')}</dt>
      <dd>{map.version}</dd>
    </dl>
  );
}
