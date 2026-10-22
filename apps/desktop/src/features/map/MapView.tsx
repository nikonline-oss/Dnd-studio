import type { Application } from 'pixi.js';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MapScene } from './MapScene';
import { useCampaign, useSnapshot } from '@/shared/game/useGame';
import { logFrontend } from '@/shared/ipc/client';
import { PixiHost } from '@/shared/pixi/PixiHost';
import { useSelection } from '@/shared/state/selection';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';

/**
 * Center tab: the selected map on a Pixi canvas (ADR-0017). Wheel zooms at the cursor, drag pans.
 * Players without a selection see the first map shared with them.
 */
export function MapView() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const mapId = useSelection((s) => s.mapId);
  const maps = snapshot?.maps ?? [];
  const map =
    maps.find((m) => m.id === mapId) ??
    (campaign?.role === 'player' ? maps.find((m) => m.visible_to_players) : undefined);
  const background = map && snapshot?.assets.find((a) => a.id === map.background_asset_id);
  const [scene, setScene] = useState<MapScene | null>(null);
  const [failure, setFailure] = useState<'renderer' | 'images' | null>(null);

  useEffect(() => {
    scene?.show(map ?? null, background ?? null);
  }, [scene, map, background]);

  const onReady = (app: Application) => {
    const created = new MapScene(app, (error) => {
      logFrontend('error', `map images failed to load: ${String(error)}`);
      setFailure('images');
    });
    setScene(created);
    return () => {
      setScene(null);
      created.destroy();
    };
  };

  const onError = (error: unknown) => {
    logFrontend('error', `map renderer failed to start: ${String(error)}`);
    setFailure('renderer');
  };

  return (
    <div className="map-view">
      <PixiHost onReady={onReady} onError={onError} />
      {!map && (
        <div className="map-view-empty">
          <Icon name="map" size={40} />
          <p>{t('mapView.noMap')}</p>
        </div>
      )}
      {map && (
        <div className="map-toolbar" role="toolbar" aria-label={t('mapView.toolbar')}>
          <span className="map-title">{map.name}</span>
          <IconButton
            icon="fit"
            label={t('mapView.fit')}
            onClick={() => {
              scene?.fit();
            }}
          />
        </div>
      )}
      {failure && (
        <p role="alert" className="map-view-error error-text">
          {t(`mapView.${failure}Failed`)}
        </p>
      )}
    </div>
  );
}
