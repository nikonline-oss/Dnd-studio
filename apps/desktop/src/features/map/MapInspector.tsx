import { useTranslation } from 'react-i18next';
import { GridForm } from './GridForm';
import { useCampaign, useSnapshotSelect } from '@/shared/game/useGame';
import { assetUrl, type AssetDto, type MapDto } from '@/shared/ipc/client';
import { useSelection } from '@/shared/state/selection';
import { Icon } from '@/shared/ui/Icon';

/** Inspector panel: properties of the selected map; GM roles edit its grid. */
export function MapInspector() {
  const { t } = useTranslation();
  const { data: campaign } = useCampaign();
  const mapId = useSelection((s) => s.mapId);
  const map = useSnapshotSelect((s) => s.maps.find((m) => m.id === mapId));
  // Counts and lookups, not the token list itself: moving a token changes nothing here.
  const tokenCount = useSnapshotSelect(
    (s) => s.tokens.filter((tok) => tok.map_id === mapId).length,
  );
  const background = useSnapshotSelect((s) =>
    s.assets.find((a) => a.id === map?.background_asset_id),
  );

  if (!map) return <p className="inspector-empty muted">{t('inspector.empty')}</p>;
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';

  return (
    <div className="inspector-stack">
      <section className="inspector-section">
        <MapPreview map={map} background={background} />
        <MapProperties map={map} tokenCount={tokenCount ?? 0} showGrid={!isGm} />
      </section>
      {isGm && (
        <section className="inspector-section">
          <h3 className="section-title">
            <Icon name="grid" />
            {t('inspector.grid')}
          </h3>
          <GridForm key={`${map.id}:${map.version}`} map={map} />
        </section>
      )}
    </div>
  );
}

function MapPreview({ map, background }: { map: MapDto; background: AssetDto | undefined }) {
  const { t } = useTranslation();
  return (
    <figure className="map-preview">
      {background ? (
        <img
          className="thumbnail"
          src={assetUrl(background.thumbnail_sha256)}
          alt={background.original_name ?? map.name}
        />
      ) : (
        <div className="thumbnail placeholder" aria-hidden="true">
          <Icon name="grid" size={28} />
        </div>
      )}
      <figcaption>
        <strong>{map.name}</strong>
        <span className="muted">
          {background
            ? (background.original_name ?? t('inspector.background'))
            : t('inspector.noBackground')}
        </span>
      </figcaption>
    </figure>
  );
}

interface PropertiesProps {
  map: MapDto;
  tokenCount: number;
  showGrid: boolean;
}

function MapProperties({ map, tokenCount, showGrid }: PropertiesProps) {
  const { t } = useTranslation();
  return (
    <dl className="properties">
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
