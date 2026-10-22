import type { Application } from 'pixi.js';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MapScene } from './MapScene';
import { useCampaign, useSnapshot } from '@/shared/game/useGame';
import { logFrontend } from '@/shared/ipc/client';
import { PixiHost } from '@/shared/pixi/PixiHost';
import { useSelection } from '@/shared/state/selection';

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
      {!map && <p className="map-view-empty muted">{t('mapView.noMap')}</p>}
      {map && (
        <div className="map-toolbar">
          <button
            type="button"
            onClick={() => {
              scene?.fit();
            }}
          >
            {t('mapView.fit')}
          </button>
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
