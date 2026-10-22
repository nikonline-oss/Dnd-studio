import type { Application } from 'pixi.js';
import { useEffect, useRef, useState, type DragEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { DEFAULT_FOG_TOOL, FogToolbar, sceneTool, type FogToolState } from './FogToolbar';
import { MapScene, type TokenMove } from './MapScene';
import { snapToGrid, tokenPosition } from './tokenMath';
import { tokenViews } from './tokenViews';
import { CHARACTER_DRAG_TYPE } from '@/shared/dragTypes';
import { useCampaign, useSnapshot, useSubmitIntent } from '@/shared/game/useGame';
import { useShownMap } from '@/shared/game/useShownMap';
import { logFrontend, type FogOpKind, type FogShape } from '@/shared/ipc/client';
import { uuidv7 } from '@/shared/ipc/uuid';
import { PixiHost } from '@/shared/pixi/PixiHost';
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
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const map = useShownMap();
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

  const shownMapId = map?.id;
  useEffect(() => {
    if (!scene) return;
    scene.setTokens(
      shownMapId && snapshot && campaign ? tokenViews(snapshot, shownMapId, campaign) : [],
    );
  }, [scene, shownMapId, snapshot, campaign]);

  // Token moves are last-write-wins: no version, a new intent id each (ADR-0003).
  const sendMove = useRef<(move: TokenMove) => void>(() => undefined);
  useEffect(() => {
    sendMove.current = ({ id, x, y }) => {
      submit.mutate({ intent: { kind: 'move_token', id, x, y } });
    };
  });

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
      (move) => {
        sendMove.current(move);
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

  // A character dragged from the navigator becomes a token where it is dropped (GM roles).
  const canDrop = isGm && map !== undefined && scene !== null;
  const [dropping, setDropping] = useState(false);
  const accepts = (e: DragEvent<HTMLDivElement>) =>
    canDrop && e.dataTransfer.types.includes(CHARACTER_DRAG_TYPE);

  const dropCharacter = (e: DragEvent<HTMLDivElement>) => {
    const characterId = e.dataTransfer.getData(CHARACTER_DRAG_TYPE);
    const character = snapshot?.characters.find((c) => c.id === characterId);
    if (!map || !scene || !character) return;
    const box = e.currentTarget.getBoundingClientRect();
    const at = scene.canvasToMap(e.clientX - box.left, e.clientY - box.top);
    const onMap = (v: number, max: number) => Number.isFinite(v) && v >= 0 && v <= max;
    if (!onMap(at.x, map.width) || !onMap(at.y, map.height)) return;
    // Snapped to the grid unless Alt is held, like a moved token.
    const { x, y } = tokenPosition(e.altKey ? at : snapToGrid(at, map.grid), map);
    submit.mutate({
      intent: {
        kind: 'create_token',
        id: uuidv7(),
        map_id: map.id,
        character_id: character.id,
        asset_id: null,
        name: character.name,
        x,
        y,
        rotation: 0,
        scale: 1,
        layer: 'tokens',
        visible: true,
      },
    });
  };

  return (
    <div
      className="map-view"
      data-dropping={dropping}
      onDragOver={(e) => {
        if (!accepts(e)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        setDropping(true);
      }}
      onDragLeave={(e) => {
        // Leaving for a child (toolbar, canvas) is not leaving the map.
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false);
      }}
      onDrop={(e) => {
        setDropping(false);
        if (!accepts(e)) return;
        e.preventDefault();
        dropCharacter(e);
      }}
    >
      <PixiHost onReady={onReady} onError={onError} />
      {dropping && <div className="map-drop-hint">{t('mapView.dropHint')}</div>}
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
