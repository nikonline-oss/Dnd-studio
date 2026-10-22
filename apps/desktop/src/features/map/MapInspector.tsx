import { useTranslation } from 'react-i18next';
import { GridForm } from './GridForm';
import { useCampaign, useSnapshot } from '@/shared/game/useGame';
import { assetUrl, type AssetDto, type MapDto } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';

/** Inspector panel: properties of the selected map; GM roles edit its grid. */
export function MapInspector() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const mapId = useSelection((s) => s.mapId);
  const map = snapshot?.maps.find((m) => m.id === mapId);

  if (!map) return <p className="muted">{t('inspector.empty')}</p>;
  const tokenCount = snapshot?.tokens.filter((tok) => tok.map_id === map.id).length ?? 0;
  const background = snapshot?.assets.find((a) => a.id === map.background_asset_id);
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';

  return (
    <>
      <MapProperties map={map} background={background} tokenCount={tokenCount} showGrid={!isGm} />
      {isGm && <GridForm key={`${map.id}:${map.version}`} map={map} />}
    </>
  );
}

interface PropertiesProps {
  map: MapDto;
  background: AssetDto | undefined;
  tokenCount: number;
  showGrid: boolean;
}

function MapProperties({ map, background, tokenCount, showGrid }: PropertiesProps) {
  const { t } = useTranslation();
  return (
    <dl className="properties">
      <dt>{t('inspector.name')}</dt>
      <dd>{map.name}</dd>
      {background && (
        <>
          <dt>{t('inspector.background')}</dt>
          <dd>
            <img
              className="thumbnail"
              src={assetUrl(background.thumbnail_sha256)}
              alt={background.original_name ?? map.name}
            />
          </dd>
        </>
      )}
      <dt>{t('inspector.size')}</dt>
      <dd>
        {map.width} × {map.height}
      </dd>
      {showGrid && (
        <>
          <dt>{t('inspector.grid')}</dt>
          <dd>
            {t(`inspector.gridKind.${map.grid.kind}`)}, {map.grid.cell_size}
          </dd>
        </>
      )}
      <dt>{t('inspector.visibility')}</dt>
      <dd>{map.visible_to_players ? t('maps.visible') : t('maps.hidden')}</dd>
      <dt>{t('inspector.tokens')}</dt>
      <dd>{tokenCount}</dd>
      <dt>{t('inspector.version')}</dt>
      <dd>{map.version}</dd>
    </dl>
  );
}
