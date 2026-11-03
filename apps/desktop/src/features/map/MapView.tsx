import { useActionAim } from '@/shared/state/actionAim';
import { labelText } from '@/shared/i18n/label';
import { useSettings } from '@/shared/settings/settings';
import type { Application } from 'pixi.js';
import { useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { DEFAULT_FOG_TOOL, FogToolbar, sceneTool, type FogToolState } from './FogToolbar';
import { MapScene, type TokenMove, type ZoneEditCallbacks } from './MapScene';
import { snapToGrid, tokenPosition } from './tokenMath';
import { tokenViews } from './tokenViews';
import { useZoneTool, zoneParts } from './zoneTool';
import { CHARACTER_DRAG_TYPE } from '@/shared/dragTypes';
import {
  useCampaign,
  useSnapshot,
  useSnapshotSelect,
  useSubmitIntent,
} from '@/shared/game/useGame';
import { useShownMap } from '@/shared/game/useShownMap';
import { useSelection } from '@/shared/state/selection';
import { logFrontend, type FogOpKind, type FogShape, type ZoneShape } from '@/shared/ipc/client';
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
export function MapView({ mapId }: { mapId?: string } = {}) {
  const { t, i18n } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: campaign } = useCampaign();
  const submit = useSubmitIntent();
  const isGm = campaign?.role === 'gm' || campaign?.role === 'co_gm';
  const map = useShownMap(mapId);
  const selectedTokenId = useSelection((s) => s.tokenId);
  const combat = useSnapshotSelect((s) => s.combats.some((entry) => entry.map_id === map?.id));
  const triggerZones = useSnapshotSelect((s) =>
    s.triggers.flatMap(({ definition }) =>
      definition.event.type === 'roll_in_zone' || definition.event.type === 'token_entered_zone'
        ? [definition.event.zone]
        : [],
    ),
  );
  const background = map && snapshot?.assets.find((a) => a.id === map.background_asset_id);
  const fog = map && snapshot?.fog.find((f) => f.map_id === map.id);
  const [scene, setScene] = useState<MapScene | null>(null);
  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        scene?.cancelMovement();
        useActionAim.getState().setAim(null);
      }
    };
    window.addEventListener('keydown', cancel);
    return () => {
      window.removeEventListener('keydown', cancel);
    };
  }, [scene]);
  const effects = useSettings((s) => s.pluginEffects);
  const opacity = useSettings((s) => s.pluginEffectOpacity);
  const sharedAim = useActionAim((s) => s.aim);
  const setAim = useActionAim((s) => s.setAim);
  const aim = sharedAim?.mapId === map?.id ? sharedAim : null;
  const availability =
    snapshot?.rules_view?.tokens?.find((t) => t.token === selectedTokenId)?.actions ?? [];
  const plugin = snapshot?.plugins.find((p) => p.id === snapshot.system_plugin);
  const selectedToken = snapshot?.tokens.find(
    (token) => token.id === selectedTokenId && token.map_id === map?.id,
  );
  const ownsSelected =
    isGm ||
    (campaign?.role === 'player' &&
      snapshot?.characters.find((c) => c.id === selectedToken?.character_id)?.owner ===
        campaign.user_id);
  const actions = [
    ...(plugin?.system?.mechanics?.actions?.filter(
      (a) =>
        !a.reaction_only &&
        (!a.feature ||
          (plugin.feature_settings?.[a.feature] ??
            plugin.system?.features?.find((f) => f.id === a.feature)?.default_enabled)),
    ) ?? []),
    ...availability.flatMap((a) => (a.sheet_action ? [a.sheet_action.action] : [])),
  ];
  useEffect(() => {
    scene?.showPluginArea(0, 0, 0, '#ffffff');
  }, [scene, map?.id, aim?.action.id]);
  useEffect(() => {
    if (!scene || !map) return;
    const draw = () => {
      scene.showPluginEffects(
        effects ? (snapshot?.plugin_runtime?.visuals ?? []).filter((v) => v.map === map.id) : [],
        map.grid.cell_size,
        opacity,
      );
    };
    draw();
    const recent = (snapshot?.plugin_runtime?.visuals ?? []).filter(
      (v) => v.map === map.id && Date.now() - v.at_ms < v.visual.duration_ms,
    );
    if (!effects || recent.length === 0) return;
    const timer = window.setInterval(() => {
      draw();
      if (recent.every((v) => Date.now() - v.at_ms >= v.visual.duration_ms))
        window.clearInterval(timer);
    }, 50);
    return () => {
      window.clearInterval(timer);
    };
  }, [scene, map, snapshot?.plugin_runtime?.visuals, effects, opacity]);
  useEffect(() => {
    const source = snapshot?.tokens.find((t) => t.id === aim?.source);
    const range =
      snapshot?.rules_view?.tokens
        ?.find((t) => t.token === source?.id)
        ?.actions.find((a) => a.id === aim?.action.id)?.range_cells ?? 0;
    scene?.showPluginRange(
      source ? source.x / 16 : 0,
      source ? source.y / 16 : 0,
      aim && map ? range * map.grid.cell_size : 0,
      aim?.action.distance_metric === 'chebyshev',
    );
    return () => {
      scene?.showPluginRange(0, 0, 0, false);
    };
  }, [scene, aim, map, snapshot?.rules_view, snapshot?.tokens]);
  useEffect(() => {
    if (!aim?.point || !map) {
      scene?.showPluginArea(0, 0, 0, '#ffffff');
      return;
    }
    scene?.showPluginArea(
      aim.point.x / 16,
      aim.point.y / 16,
      (aim.action.radius_cells ?? 0.4) * map.grid.cell_size,
      aim.action.visual.color,
    );
  }, [scene, aim, map]);
  const aimAt = (clientX: number, clientY: number, canvas: HTMLCanvasElement) => {
    if (!scene || !map || !aim) return null;
    const rect = canvas.getBoundingClientRect();
    const at = scene.canvasToMap(clientX - rect.left, clientY - rect.top);
    const point = tokenPosition(at, map);
    scene.showPluginArea(
      point.x / 16,
      point.y / 16,
      (aim.action.radius_cells ?? 0.4) * map.grid.cell_size,
      aim.action.visual.color,
    );
    const target = snapshot?.tokens.find(
      (token) =>
        token.map_id === map.id &&
        Math.hypot(token.x / 16 - at.x, token.y / 16 - at.y) <=
          (map.grid.cell_size * token.scale) / 2,
    );
    const source = snapshot?.tokens.find((t) => t.id === aim.source);
    const range = snapshot?.rules_view?.tokens
      ?.find((t) => t.token === aim.source)
      ?.actions.find((a) => a.id === aim.action.id)?.range_cells;
    const destination = !aim.action.radius_cells && target ? target : point;
    const dx = source ? Math.abs(destination.x - source.x) / (16 * map.grid.cell_size) : Infinity;
    const dy = source ? Math.abs(destination.y - source.y) / (16 * map.grid.cell_size) : Infinity;
    const distance =
      aim.action.distance_metric === 'chebyshev' ? Math.max(dx, dy) : Math.hypot(dx, dy);
    return {
      ...point,
      target: target?.id ?? null,
      valid:
        range !== undefined &&
        distance <=
          range +
            (!aim.action.radius_cells && source && target
              ? Math.max(0, (source.scale + target.scale) / 2 - 1)
              : 0) +
            1 / (16 * map.grid.cell_size),
    };
  };
  const [failure, setFailure] = useState<'renderer' | 'images' | null>(null);
  const [chosenTool, setTool] = useState<FogToolState>(DEFAULT_FOG_TOOL);
  // The zone list asks to redraw a zone or add a part to it: the zone tool takes the next rectangle.
  const zoneDrawing = useZoneTool((s) => s.drawing);
  const zoneCreating = useZoneTool((s) => s.creating);
  // Zone edit mode is shared with the zone list ("Edit on the map").
  const zoneEditing = useZoneTool((s) => s.editing);
  const selectedZone = useZoneTool((s) => s.selected);
  const tool = useMemo<FogToolState>(
    () =>
      zoneCreating || zoneDrawing.mode !== 'new'
        ? { ...chosenTool, shape: 'zone' }
        : zoneEditing
          ? { ...chosenTool, shape: 'zone_edit' }
          : chosenTool,
    [zoneDrawing, zoneCreating, zoneEditing, chosenTool],
  );
  const chooseTool = (next: FogToolState) => {
    useZoneTool.getState().setEditing(next.shape === 'zone_edit');
    if (next.shape !== 'zone_edit') setTool(next);
  };

  useEffect(() => {
    scene?.show(map ?? null, background ?? null, fog ?? null, isGm);
  }, [scene, map, background, fog, isGm]);

  useEffect(() => {
    scene?.setTool(isGm ? sceneTool(tool) : null);
    scene?.setZoneEditing(isGm && tool.shape === 'zone_edit');
  }, [scene, tool, isGm]);

  // Zones are the GM's (ADR-0024); players never receive any.
  const zones = useSnapshotSelect((s) => s.zones.filter((z) => z.map_id === map?.id));
  useEffect(() => {
    scene?.setZones(isGm ? (zones ?? []) : [], selectedZone, isGm && combat ? triggerZones : []);
  }, [scene, zones, isGm, selectedZone, combat, triggerZones]);
  useEffect(() => {
    scene?.setCombatMovement(Boolean(combat) && !aim);
  }, [scene, combat, aim]);
  // Another map: nothing of the last one stays selected or renamed.
  const shownMapIdForZones = map?.id;
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renamingOn, setRenamingOn] = useState(shownMapIdForZones);
  if (renamingOn !== shownMapIdForZones) {
    setRenamingOn(shownMapIdForZones);
    setRenaming(null);
  }
  useEffect(() => {
    useZoneTool.getState().select(null);
  }, [shownMapIdForZones]);
  const zoneEdits = useRef<Omit<ZoneEditCallbacks, 'onRename'>>({
    onSelect: () => undefined,
    onEdit: () => undefined,
  });
  useEffect(() => {
    zoneEdits.current = {
      onSelect: (zoneId) => {
        useZoneTool.getState().select(zoneId);
      },
      onEdit: (zone, edit) => {
        submit.mutate(
          {
            intent: {
              kind: 'update_zone',
              id: zone.id,
              name: null,
              shape: edit.shape ?? null,
              label: edit.label ?? null,
            },
            expectedVersion: zone.version,
          },
          { onError: () => scene?.clearZoneOverride() },
        );
      },
    };
  });
  const renamed = renaming ? zones?.find((z) => z.id === renaming) : undefined;
  const renameAt = renamed && scene ? scene.zoneLabelOnCanvas(renamed.id) : null;
  const rename = (name: string) => {
    setRenaming(null);
    const trimmed = name.trim();
    if (!renamed || trimmed === '' || trimmed === renamed.name) return;
    submit.mutate({
      intent: { kind: 'update_zone', id: renamed.id, name: trimmed, shape: null },
      expectedVersion: renamed.version,
    });
  };
  const sendZone = useRef<(shape: ZoneShape) => void>(() => undefined);
  useEffect(() => {
    sendZone.current = (shape) => {
      if (!map) return;
      const drawing = useZoneTool.getState().drawing;
      const target =
        drawing.mode === 'new' ? undefined : zones?.find((z) => z.id === drawing.zoneId);
      if (drawing.mode !== 'new' && target) {
        submit.mutate({
          intent: {
            kind: 'update_zone',
            id: target.id,
            name: null,
            shape:
              drawing.mode === 'replace'
                ? shape
                : { type: 'union', parts: [...zoneParts(target.shape), shape] },
          },
          expectedVersion: target.version,
        });
        // One rectangle per request; then back to the tool chosen before.
        useZoneTool.getState().draw({ mode: 'new' });
        return;
      }
      submit.mutate({
        intent: {
          kind: 'create_zone',
          id: uuidv7(),
          map_id: map.id,
          name: `${t('zones.newName')} ${(zones?.length ?? 0) + 1}`,
          shape,
        },
      });
    };
  });

  const shownMapId = map?.id;
  useEffect(() => {
    if (!scene) return;
    scene.setTokens(
      shownMapId && snapshot && campaign
        ? tokenViews(snapshot, shownMapId, campaign, selectedTokenId)
        : [],
    );
  }, [scene, shownMapId, snapshot, campaign, selectedTokenId]);

  // Token moves are last-write-wins: no version, a new intent id each (ADR-0003).
  const sendMove = useRef<(move: TokenMove) => void>(() => undefined);
  useEffect(() => {
    sendMove.current = ({ id, x, y }) => {
      submit.mutate({ intent: { kind: 'move_token', id, x, y } });
    };
  });

  // Token clicks follow the freshest snapshot (the scene outlives renders): a token with a
  // character shows its sheet in the inspector; other tokens are edited through the inspector.
  const tokenClick = useRef<(tokenId: string | null) => void>(() => undefined);
  useEffect(() => {
    tokenClick.current = (tokenId) => {
      const selection = useSelection.getState();
      if (tokenId === null) {
        selection.selectToken(null);
        return;
      }
      const token = snapshot?.tokens.find((entry) => entry.id === tokenId);
      if (token?.character_id) {
        selection.selectToken(null);
        selection.selectCharacter(token.character_id);
      } else {
        selection.selectToken(tokenId);
      }
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
      (tokenId) => {
        tokenClick.current(tokenId);
      },
      (shape) => {
        sendZone.current(shape);
      },
      {
        onSelect: (zoneId) => {
          zoneEdits.current.onSelect(zoneId);
        },
        onEdit: (zone, edit) => {
          zoneEdits.current.onEdit(zone, edit);
        },
        // After the press: its mousedown would take the focus from the field again.
        onRename: (zoneId) => {
          window.setTimeout(() => {
            setRenaming(zoneId);
          }, 0);
        },
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
        disposition: 'neutral',
      },
    });
  };

  return (
    <div
      className="map-view"
      data-dropping={dropping}
      onPointerMoveCapture={(e) => {
        if (aim && !aim.point && e.target instanceof HTMLCanvasElement)
          aimAt(e.clientX, e.clientY, e.target);
      }}
      onPointerDownCapture={(e) => {
        if (!aim || e.button !== 0 || !(e.target instanceof HTMLCanvasElement)) return;
        e.stopPropagation();
        e.preventDefault();
        const point = aimAt(e.clientX, e.clientY, e.target);
        if (point) setAim({ ...aim, point });
      }}
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
      {combat && (
        <button
          className="map-cancel-movement"
          onClick={() => {
            scene?.cancelMovement();
          }}
        >
          {t('maps.cancelMovement')}
        </button>
      )}
      {selectedToken && ownsSelected && actions.length > 0 && (
        <div className="plugin-map-actions">
          {actions.map((action) => (
            <button
              type="button"
              key={action.id}
              disabled={!availability.find((a) => a.id === action.id)?.enabled}
              title={t(
                `actions.reason.${availability.find((a) => a.id === action.id)?.reason ?? 'available'}`,
              )}
              onClick={() => {
                setAim({
                  source: selectedToken.id,
                  mapId: selectedToken.map_id,
                  action,
                  point: action.target_self
                    ? { x: selectedToken.x, y: selectedToken.y, target: selectedToken.id }
                    : null,
                });
              }}
            >
              {labelText(action.label, i18n.language)}
            </button>
          ))}
        </div>
      )}
      {aim && (
        <div className="plugin-action-confirm" role="status">
          <strong>{labelText(aim.action.label, i18n.language)}</strong>
          <p>
            {t(aim.point ? 'actions.targetLocked' : 'plugins.chooseTarget')} ·{' '}
            {t('actions.range', {
              range:
                snapshot?.rules_view?.tokens
                  ?.find((t) => t.token === aim.source)
                  ?.actions.find((a) => a.id === aim.action.id)?.range_cells ?? 0,
            })}
          </p>
          {aim.point && !aim.action.target_self && (
            <button
              type="button"
              disabled={submit.isPending}
              onClick={() => {
                setAim({ ...aim, point: null });
              }}
            >
              {t('actions.retarget')}
            </button>
          )}
          {aim.point?.valid === false && <p role="alert">{t('actions.outOfRange')}</p>}
          <button
            type="button"
            disabled={
              !aim.point ||
              aim.point.valid === false ||
              !snapshot?.rules_view?.tokens
                ?.find((t) => t.token === aim.source)
                ?.actions.find((a) => a.id === aim.action.id)?.enabled ||
              (!aim.action.radius_cells && !aim.point.target) ||
              submit.isPending
            }
            onClick={() => {
              if (!aim.point) return;
              submit.mutate(
                {
                  intent: {
                    kind: 'execute_plugin_action',
                    source: aim.source,
                    action: aim.action.id,
                    target: aim.point.target,
                    x: aim.point.x,
                    y: aim.point.y,
                  },
                },
                {
                  onSuccess: () => {
                    setAim(null);
                    scene?.showPluginArea(0, 0, 0, '#ffffff');
                  },
                },
              );
            }}
          >
            {t('plugins.applyAction')}
          </button>
          <button
            type="button"
            onClick={() => {
              setAim(null);
              scene?.showPluginArea(0, 0, 0, '#ffffff');
            }}
          >
            {t('common.cancel', { defaultValue: 'Cancel' })}
          </button>
        </div>
      )}
      {dropping && <div className="map-drop-hint">{t('mapView.dropHint')}</div>}
      {map && isGm && tool.shape === 'zone_edit' && (
        <p className="map-edit-hint" role="status">
          {t('zones.editHint')}
        </p>
      )}
      {map && combat && (
        <p className="map-combat-hint" role="status">
          {selectedTokenId
            ? t('mapView.combatMoveTargetHint')
            : t(isGm ? 'mapView.combatMoveSelectHintGm' : 'mapView.combatMoveSelectHint')}
        </p>
      )}
      {renamed && renameAt && (
        <input
          className="zone-rename"
          aria-label={t('zones.nameOnMap')}
          style={{ left: renameAt.x, top: renameAt.y }}
          defaultValue={renamed.name}
          maxLength={200}
          autoFocus
          onFocus={(e) => {
            e.currentTarget.select();
          }}
          onKeyDown={(e) => {
            // Both end in blur, so the name is sent once.
            if (e.key === 'Escape') e.currentTarget.dataset.cancelled = 'true';
            if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur();
          }}
          onBlur={(e) => {
            if (e.currentTarget.dataset.cancelled) setRenaming(null);
            else rename(e.currentTarget.value);
          }}
        />
      )}
      {!map && (
        <div className="map-view-empty">
          <Icon name="map" size={40} />
          <p>{t('mapView.noMap')}</p>
        </div>
      )}
      {map && isGm && (
        <FogToolbar
          state={tool}
          onChange={chooseTool}
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
