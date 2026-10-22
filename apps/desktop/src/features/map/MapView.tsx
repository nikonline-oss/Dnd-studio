import type { Application } from 'pixi.js';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DEFAULT_FOG_TOOL, FogToolbar, sceneTool, type FogToolState } from './FogToolbar';
import { MapScene } from './MapScene';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { logFrontend, type FogOpKind, type FogShape } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { PixiHost } from '@/shared/pixi/PixiHost';
import { useSelection } from '@/shared/state/selection';
import { ErrorText } from '@/shared/ui/ErrorText';
import { Icon } from '@/shared/ui/Icon';
import { IconButton } from '@/shared/ui/IconButton';

/**
 * Center tab: the selected map on a Pixi canvas (ADR-0017). Wheel zooms at the cursor, drag pans.
 * GM roles draw fog (ADR-0019) and see it translucent; players see it opaque.
 * Players without a selection see the first map shared with them.
 */
export function MapView() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const mapId = useSelection((s) => s.mapId);
  const maps = snapshot?.maps ?? [];
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const map =
    maps.find((m) => m.id === mapId) ??
    (campaign?.role === 'player' ? maps.find((m) => m.visible_to_players) : undefined);
  const background = map && snapshot?.assets.find((a) => a.id === map.background_asset_id);
  const fog = map && snapshot?.fog.find((f) => f.map_id === map.id);
  const [scene, setScene] = useState<MapScene | null>(null);
  const [failure, setFailure] = useState<'renderer' | 'images' | null>(null);
  const [tool, setTool] = useState<FogToolState>(DEFAULT_FOG_TOOL);

  useEffect(() => {
    scene?.show(map ?? null, background ?? null, fog ?? null, isGm);
  }, [scene, map, background, fog, isGm]);

  useEffect(() => {
    scene?.setTool(isGm ? sceneTool(tool) : null);
  }, [scene, tool, isGm]);

  // The scene is created once; its fog callback reads the current map through this ref.
  const sendFog = useRef<(kind: FogOpKind, shapes: FogShape[]) => void>(() => undefined);
  useEffect(() => {
    sendFog.current = (kind, shapes) => {
      if (!map) return;
      // One intent per shape, in order: a long stroke is several operations (ADR-0019).
      void shapes
        .reduce(
          (previous, shape) =>
            previous.then(() =>
              submit.mutateAsync({
                intent: { kind: 'apply_fog', map_id: map.id, op: { id: uuidv7(), kind, shape } },
              }),
            ),
          Promise.resolve<unknown>(undefined),
        )
        .catch(() => undefined); // shown by ErrorText below
    };
  });

  const onReady = (app: Application) => {
    const created = new MapScene(
      app,
      (error) => {
        logFrontend('error', `map images failed to load: ${String(error)}`);
        setFailure('images');
      },
      (kind, shapes) => {
        sendFog.current(kind, shapes);
      },
    );
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
      {map && isGm && (
        <FogToolbar
          state={tool}
          onChange={setTool}
          hasFog={fog !== undefined}
          disabled={submit.isPending}
          onReset={(covered) => {
            submit.mutate({ intent: { kind: 'reset_fog', map_id: map.id, covered } });
          }}
        />
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
      {(failure || submit.error) && (
        <div className="map-view-error">
          {failure && (
            <p role="alert" className="error-text">
              {t(`mapView.${failure}Failed`)}
            </p>
          )}
          <ErrorText error={submit.error} />
        </div>
      )}
    </div>
  );
}
