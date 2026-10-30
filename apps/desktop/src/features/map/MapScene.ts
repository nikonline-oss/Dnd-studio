// Imperative Pixi scene of one map (ADR-0017): tiled background, grid, fog (ADR-0019), camera,
// the GM's fog tools and zone edit mode (ADR-0024). React only feeds it through `show` and `setTool`; nothing here is rendered
// by React.
import {
  Container,
  Graphics,
  Rectangle,
  Sprite,
  Texture,
  Text,
  TilingSprite,
  type Application,
  type FederatedPointerEvent,
} from 'pixi.js';
import { drawShape, FogLayer } from './FogLayer';
import { BrushStroke, brushRadius16, rectFromDrag } from './fogShapes';
import { gridPattern } from './gridPattern';
import { TokenLayer, type TokenView } from './TokenLayer';
import { shortestMovementPath, movementDistance } from './movementPath';
import i18n from '@/shared/i18n';
import {
  clampLabel,
  dragHandle,
  handleAt,
  labelPosition,
  moveShape,
  zoneAt,
  type Handle,
  type Point16,
} from './zoneEdit';
import { HANDLE_PX, ZoneLayer } from './ZoneLayer';
import { snapToGrid, Throttle, tokenPosition } from './tokenMath';
import {
  clampCamera,
  fitCamera,
  panBy,
  wheelFactor,
  zoomAt,
  type Camera,
  type Point,
  type Size,
} from './viewport';
import type {
  AssetDto,
  FogDto,
  FogOpKind,
  FogShape,
  Grid,
  MapDto,
  ZoneDto,
  ZoneLabel,
  ZoneShape,
} from '@/shared/ipc/client';
import { assetUrl } from '@/shared/ipc/client';

/** Shown under a map without a background image, so its bounds stay visible. */
const BLANK_MAP_COLOR = 0xd9d4c7;
const GRID_COLOR = 0x000000;
const GRID_ALPHA = 0.35;
/** The grid texture is drawn at least this many pixels wide, so it stays sharp when zoomed. */
const GRID_TEXTURE_PX = 256;
const REVEAL_PREVIEW = 0xe3aa45;
const HIDE_PREVIEW = 0x0b0a10;
const PREVIEW_ALPHA = 0.45;
const ZONE_PREVIEW = 0x9b7bff;
const MOVEMENT_PATH = 0x163e42;
/** A press that moves less than this (CSS px) is a click, not a drag. */
const CLICK_SLOP_PX = 4;
/** Token moves while dragging: ~12 Hz (ADR-0003 asks for 10–15 Hz). */
const MOVE_INTERVAL_MS = 80;
/** Two presses on a zone's name within this time rename it. */
const DOUBLE_CLICK_MS = 400;

/** A token position to send; `final` on drop (snapped to the grid). */
export interface TokenMove {
  id: string;
  x: number;
  y: number;
  final: boolean;
}

/** The GM's active tool (fog, or drawing a zone); `null` = the left button pans. */
export interface FogTool {
  shape: 'rect' | 'brush' | 'zone';
  kind: FogOpKind;
  /** Brush radius in map units. */
  radius: number;
}

/** What the GM changed about a zone in edit mode: sent as one `update_zone`. */
export interface ZoneEdit {
  shape?: ZoneShape;
  label?: ZoneLabel;
}

/** Zone edit mode callbacks (GM roles, ADR-0024). */
export interface ZoneEditCallbacks {
  /** A zone was clicked (`null`: the empty map). */
  onSelect: (zoneId: string | null) => void;
  /** A zone, one of its handles or its name was dragged and dropped. */
  onEdit: (zone: ZoneDto, edit: ZoneEdit) => void;
  /** The zone's name was double-clicked. */
  onRename: (zoneId: string) => void;
}

/** What a press in zone edit mode grabbed. */
type ZoneGrab = { kind: 'move' } | { kind: 'label' } | { kind: 'handle'; handle: Handle };

type Drawing =
  | { tool: 'rect'; kind: FogOpKind; from: Point; to: Point; zone: boolean }
  | { tool: 'brush'; kind: FogOpKind; stroke: BrushStroke };

async function loadBitmap(sha256: string): Promise<ImageBitmap> {
  const response = await fetch(assetUrl(sha256));
  if (!response.ok) throw new Error(`asset ${sha256}: HTTP ${response.status}`);
  return createImageBitmap(await response.blob());
}

export class MapScene {
  private readonly world = new Container();
  private readonly background = new Container();
  private readonly gridLayer = new Container();
  private readonly fog: FogLayer;
  /** Tool preview, drawn in Units16 (scaled down to map units). */
  private readonly preview = new Graphics();
  /** Shortest-path route preview during combat; drawn above fog and zones. */
  private readonly movementPreview = new Graphics();
  private readonly movementLabel = new Text({
    text: '',
    style: {
      fontFamily: 'Segoe UI',
      fontSize: 13,
      fill: 0xffffff,
      stroke: { color: 0x16232b, width: 4 },
    },
  });
  /** Zones, shown to GM roles only (ADR-0024). */
  private readonly zones = new ZoneLayer();
  private camera: Camera = { x: 0, y: 0, zoom: 1 };
  private mapSize: Size | null = null;
  private mapId: string | null = null;
  private backgroundKey: string | null = null;
  private gridKey: string | null = null;
  private tileTextures: Texture[] = [];
  private bitmaps: ImageBitmap[] = [];
  private gridTexture: Texture | null = null;
  /** Bumped on every background change; late loads of an older one are discarded. */
  private loadSerial = 0;
  /** Fit once the canvas has a size (it may still be 0×0 when the first map arrives). */
  private pendingFit = false;
  private dragFrom: { x: number; y: number } | null = null;
  /** Where a pan started: a press without movement on the map clears the token selection. */
  private panStart: { x: number; y: number } | null = null;
  private tool: FogTool | null = null;
  private drawing: Drawing | null = null;
  private readonly tokens: TokenLayer;
  private tokenViews: TokenView[] = [];
  private grid: Grid | null = null;
  /** The token being dragged and where on it it was grabbed (map units). */
  private tokenDrag: {
    id: string;
    dx: number;
    dy: number;
    /** Screen point of the press; until the pointer leaves it, this is a click. */
    start: Point;
    moved: boolean;
  } | null = null;
  private readonly moves: Throttle<TokenMove>;
  private zoneList: ZoneDto[] = [];
  private selectedZone: string | null = null;
  private triggeredZones: string[] = [];
  private zoneEditing = false;
  private combatMovement = false;
  private movementTokenId: string | null = null;
  private movementPress = false;
  /** The zone being dragged in edit mode and from where (Units16). */
  private zoneDrag: {
    zone: ZoneDto;
    grab: ZoneGrab;
    from: Point16;
    start: Point;
    moved: boolean;
  } | null = null;
  /** Shown instead of the zone while dragging, and until the change comes back. */
  private zoneOverride: { zone: ZoneDto; sent: boolean } | null = null;
  private lastLabelPress: { zoneId: string; time: number } | null = null;

  constructor(
    private readonly app: Application,
    private readonly onLoadError: (error: unknown) => void,
    /** A finished fog drawing: one shape per operation to send, in order. */
    private readonly onFog: (kind: FogOpKind, shapes: FogShape[]) => void = () => undefined,
    /** A token was dragged: throttled positions while moving, then the final one. */
    private readonly onMoveToken: (move: TokenMove) => void = () => undefined,
    /** A token was clicked (`null`: the empty map was clicked). */
    private readonly onSelectToken: (id: string | null) => void = () => undefined,
    /** A zone was drawn (GM roles, ADR-0024). */
    private readonly onZone: (shape: ZoneShape) => void = () => undefined,
    private readonly zoneEdit: ZoneEditCallbacks = {
      onSelect: () => undefined,
      onEdit: () => undefined,
      onRename: () => undefined,
    },
  ) {
    this.fog = new FogLayer(app);
    this.tokens = new TokenLayer(this.onGrabToken);
    this.moves = new Throttle(MOVE_INTERVAL_MS, (move) => {
      this.onMoveToken(move);
    });
    this.preview.scale.set(1 / 16);
    this.movementPreview.scale.set(1 / 16);
    this.movementLabel.visible = false;
    this.movementLabel.eventMode = 'none';
    this.preview.eventMode = 'none';
    // Tokens sit under the fog: players do not see what is hidden.
    this.world.addChild(
      this.background,
      this.gridLayer,
      this.tokens.view,
      this.fog.view,
      this.zones.view,
      this.preview,
      this.movementPreview,
      this.movementLabel,
    );
    app.stage.addChild(this.world);
    const stage = app.stage;
    stage.eventMode = 'static';
    stage.hitArea = app.screen;
    stage.on('pointerdown', this.onDown).on('globalpointermove', this.onMove);
    stage.on('pointerup', this.onUp).on('pointerupoutside', this.onUp);
    app.canvas.addEventListener('wheel', this.onWheel, { passive: false });
    app.canvas.style.cursor = 'grab';
    app.renderer.on('resize', this.onResize);
  }

  /**
   * Shows `map` (or nothing) with its fog: translucent for GM roles, opaque for players. Only
   * what changed is rebuilt.
   */
  show(map: MapDto | null, background: AssetDto | null, fog: FogDto | null, gmView: boolean): void {
    this.world.visible = map !== null;
    if (!map) {
      this.mapId = null;
      this.cancelDrawing();
      return;
    }
    const sizeChanged =
      this.mapSize === null ||
      this.mapSize.width !== map.width ||
      this.mapSize.height !== map.height;
    this.mapSize = { width: map.width, height: map.height };

    const backgroundKey = `${background?.id ?? 'none'}:${map.width}x${map.height}`;
    if (backgroundKey !== this.backgroundKey) {
      this.backgroundKey = backgroundKey;
      this.setBackground(map, background);
    }
    const gridKey = `${JSON.stringify(map.grid)}:${map.width}x${map.height}`;
    if (gridKey !== this.gridKey) {
      this.gridKey = gridKey;
      this.setGrid(map);
    }
    this.fog.show(fog, map.width, map.height, gmView);
    if (map.id !== this.mapId || sizeChanged) {
      this.mapId = map.id;
      this.cancelDrawing();
      this.endTokenDrag();
      this.clearMovementPlan();
      this.fit();
    }
    this.grid = map.grid;
    this.tokens.show(this.tokenViews, map.grid, this.tokensGrabbable());
  }

  /** The tokens of the shown map, bottom first. A dragged token keeps its local position. */
  setTokens(views: TokenView[]): void {
    this.tokenViews = views;
    if (this.grid) this.tokens.show(views, this.grid, this.tokensGrabbable());
  }

  /** The zones of the shown map (empty for players, who never get any) and the selected one. */
  setZones(zones: ZoneDto[], selected: string | null = null, triggered: string[] = []): void {
    this.zoneList = zones;
    this.selectedZone = selected;
    this.triggeredZones = triggered;
    // A sent change is shown until the zone comes back changed (or the request fails).
    const shown = this.zoneOverride;
    if (shown?.sent) {
      const current = zones.find((z) => z.id === shown.zone.id);
      if (!current || current.version !== shown.zone.version) this.zoneOverride = null;
    }
    this.drawZones();
  }

  /** Drops a change still shown after its request failed. */
  clearZoneOverride(): void {
    if (this.zoneOverride?.sent) {
      this.zoneOverride = null;
      this.drawZones();
    }
  }

  /** Zone edit mode (GM roles): the left button selects, moves and reshapes zones. */
  setZoneEditing(on: boolean): void {
    if (on === this.zoneEditing) return;
    this.zoneEditing = on;
    this.zoneDrag = null;
    this.app.canvas.style.cursor = this.idleCursor();
    if (this.grid) this.tokens.show(this.tokenViews, this.grid, this.tokensGrabbable());
    this.drawZones();
  }

  /** Combat uses click-to-plan movement instead of dragging the token itself. */
  setCombatMovement(on: boolean): void {
    if (on === this.combatMovement) return;
    this.combatMovement = on;
    this.clearMovementPlan();
    this.drawZones();
  }

  /** Where a zone's name is on the canvas (CSS pixels), to rename it in place. */
  zoneLabelOnCanvas(zoneId: string): Point | null {
    const box = this.zones.labelBox(zoneId);
    return box ? this.world.toGlobal({ x: box.x / 16, y: box.y / 16 }) : null;
  }

  /** Converts a point of the canvas (CSS pixels) to map units. */
  canvasToMap(x: number, y: number): Point {
    return this.world.toLocal({ x, y });
  }

  /** Selects a fog tool (`null`: the left button pans again). */
  setTool(tool: FogTool | null): void {
    this.tool = tool;
    this.cancelDrawing();
    this.endTokenDrag();
    if (tool) this.clearMovementPlan();
    this.app.canvas.style.cursor = this.idleCursor();
    // Tokens can be grabbed only without a fog tool (and outside zone edit mode).
    if (this.grid) this.tokens.show(this.tokenViews, this.grid, this.tokensGrabbable());
  }

  /** The whole map in the window. */
  fit(): void {
    const screen = this.screenSize();
    if (!this.mapSize) return;
    if (screen.width === 0 || screen.height === 0) {
      this.pendingFit = true;
      return;
    }
    this.pendingFit = false;
    this.setCamera(fitCamera(screen, this.mapSize));
  }

  destroy(): void {
    this.loadSerial++;
    const stage = this.app.stage;
    stage.off('pointerdown', this.onDown).off('globalpointermove', this.onMove);
    stage.off('pointerup', this.onUp).off('pointerupoutside', this.onUp);
    this.app.canvas.removeEventListener('wheel', this.onWheel);
    this.app.renderer.off('resize', this.onResize);
    this.app.stage.removeChild(this.world);
    this.moves.cancel();
    this.tokens.destroy();
    this.fog.destroy();
    this.zones.destroy();
    this.world.destroy({ children: true });
    this.releaseBackground();
    this.gridTexture?.destroy(true);
    this.gridTexture = null;
  }

  private setBackground(map: MapDto, asset: AssetDto | null): void {
    const serial = ++this.loadSerial;
    this.releaseBackground();
    this.background.removeChildren().forEach((child) => {
      child.destroy();
    });
    // Placeholder until the tiles arrive, and the whole background of a map without an image.
    this.background.addChild(
      new Graphics().rect(0, 0, map.width, map.height).fill({ color: BLANK_MAP_COLOR }),
    );
    if (!asset || asset.tiles.length === 0) return;

    // The image is stretched over the logical map size (ADR-0016).
    const sx = map.width / asset.width;
    const sy = map.height / asset.height;
    void Promise.all(asset.tiles.map((tile) => loadBitmap(tile.sha256))).then(
      (bitmaps) => {
        if (serial !== this.loadSerial) {
          for (const bitmap of bitmaps) bitmap.close();
          return;
        }
        this.bitmaps = bitmaps;
        const layer = new Container();
        asset.tiles.forEach((tile, i) => {
          const bitmap = bitmaps[i];
          if (!bitmap) return;
          const texture = Texture.from(bitmap);
          this.tileTextures.push(texture);
          const sprite = new Sprite(texture);
          sprite.cullable = true;
          sprite.position.set(tile.x * sx, tile.y * sy);
          sprite.scale.set((tile.width * sx) / bitmap.width, (tile.height * sy) / bitmap.height);
          layer.addChild(sprite);
        });
        this.background.removeChildren().forEach((child) => {
          child.destroy();
        });
        this.background.addChild(layer);
      },
      (error: unknown) => {
        if (serial === this.loadSerial) this.onLoadError(error);
      },
    );
  }

  private releaseBackground(): void {
    for (const texture of this.tileTextures) texture.destroy(true);
    for (const bitmap of this.bitmaps) bitmap.close();
    this.tileTextures = [];
    this.bitmaps = [];
  }

  private setGrid(map: MapDto): void {
    this.gridLayer.removeChildren().forEach((child) => {
      child.destroy();
    });
    this.gridTexture?.destroy(true);
    this.gridTexture = null;
    const pattern = gridPattern(map.grid);
    if (!pattern) return;

    const cell = new Graphics();
    for (const [x1, y1, x2, y2] of pattern.segments) cell.moveTo(x1, y1).lineTo(x2, y2);
    cell.stroke({ color: GRID_COLOR, alpha: GRID_ALPHA, width: pattern.lineWidth });
    const resolution = Math.min(8, Math.max(1, GRID_TEXTURE_PX / pattern.width));
    // The frame clips the hexes that stick out of one period.
    const texture = this.app.renderer.generateTexture({
      target: cell,
      frame: new Rectangle(0, 0, pattern.width, pattern.height),
      resolution,
      // Zoomed out, a cell shrinks ~10× on screen: without mipmaps thin lines alias away or
      // show up at random. The texture is one cell, so mipmaps cost nothing (unlike tiles).
      textureSourceOptions: { autoGenerateMipmaps: true, scaleMode: 'linear' },
    });
    cell.destroy();
    this.gridTexture = texture;
    const sprite = new TilingSprite({ texture, width: map.width, height: map.height });
    // Corrects the rounding of the texture to whole pixels.
    sprite.tileScale.set(pattern.width / texture.width, pattern.height / texture.height);
    sprite.tilePosition.set(pattern.originX, pattern.originY);
    this.gridLayer.addChild(sprite);
  }

  private screenSize(): Size {
    return { width: this.app.screen.width, height: this.app.screen.height };
  }

  private setCamera(camera: Camera): void {
    this.camera = camera;
    this.world.position.set(camera.x, camera.y);
    this.world.scale.set(camera.zoom);
    // Handles keep their size on screen.
    if (this.zoneEditing && this.selectedZone) this.drawZones();
  }

  private tokensGrabbable(): boolean {
    return this.tool === null && !this.zoneEditing;
  }

  private idleCursor(): string {
    if (this.tool) return 'crosshair';
    if (this.combatMovement && this.movementTokenId) return 'crosshair';
    return this.zoneEditing ? 'default' : 'grab';
  }

  private drawZones(): void {
    const override = this.zoneOverride?.zone;
    const zones = override
      ? this.zoneList.map((z) => (z.id === override.id ? override : z))
      : this.zoneList;
    this.zones.show(
      zones,
      this.zoneEditing ? this.selectedZone : null,
      this.camera.zoom,
      (this.mapSize?.width ?? 0) * 16,
      this.combatMovement ? this.triggeredZones : [],
    );
  }

  /** The map point under the pointer, Units16. */
  private at16(event: FederatedPointerEvent): Point16 {
    const at = this.world.toLocal(event.global);
    return { x: at.x * 16, y: at.y * 16 };
  }

  /** The map in Units16. */
  private map16(): { width: number; height: number } | null {
    return this.mapSize
      ? { width: this.mapSize.width * 16, height: this.mapSize.height * 16 }
      : null;
  }

  /** What a press at `at` grabs in zone edit mode: the selected zone's handles first. */
  private zoneGrabAt(at: Point16): { zone: ZoneDto; grab: ZoneGrab } | null {
    const selected = this.zoneList.find((z) => z.id === this.selectedZone);
    if (selected) {
      const handle = handleAt(selected.shape, at, (HANDLE_PX * 16) / this.camera.zoom);
      if (handle) return { zone: selected, grab: { kind: 'handle', handle } };
    }
    const onLabel = (zone: ZoneDto) => {
      const box = this.zones.labelBox(zone.id);
      return (
        box !== undefined &&
        at.x >= box.x &&
        at.x <= box.x + box.width &&
        at.y >= box.y &&
        at.y <= box.y + box.height
      );
    };
    const labelled = [...(selected ? [selected] : []), ...[...this.zoneList].reverse()].find(
      onLabel,
    );
    if (labelled) return { zone: labelled, grab: { kind: 'label' } };
    const inside = selected && zoneAt([selected], at) ? selected : zoneAt(this.zoneList, at);
    return inside ? { zone: inside, grab: { kind: 'move' } } : null;
  }

  /** The zone as it would be with this drag at the pointer. */
  private draggedZone(
    drag: NonNullable<MapScene['zoneDrag']>,
    event: FederatedPointerEvent,
  ): ZoneDto | null {
    const map = this.map16();
    if (!map) return null;
    const at = this.at16(event);
    const dx = at.x - drag.from.x;
    const dy = at.y - drag.from.y;
    const { zone, grab } = drag;
    switch (grab.kind) {
      case 'move': {
        // A name the GM placed moves with the zone; the default one follows it anyway.
        const moved = moveShape(zone.shape, dx, dy, map);
        const label = zone.label
          ? clampLabel({ x: zone.label.x + moved.dx, y: zone.label.y + moved.dy }, map)
          : null;
        return { ...zone, shape: moved.shape, label };
      }
      case 'handle':
        return { ...zone, shape: dragHandle(zone.shape, grab.handle, at, map) };
      case 'label': {
        const from = labelPosition(zone);
        return { ...zone, label: clampLabel({ x: from.x + dx, y: from.y + dy }, map) };
      }
    }
  }

  /** A press in edit mode: select, start a drag, or rename on a double click on the name. */
  private zoneDown(event: FederatedPointerEvent): boolean {
    const at = this.at16(event);
    const hit = this.zoneGrabAt(at);
    if (!hit) {
      this.lastLabelPress = null;
      if (this.selectedZone !== null) this.zoneEdit.onSelect(null);
      return false;
    }
    if (hit.zone.id !== this.selectedZone) this.zoneEdit.onSelect(hit.zone.id);
    if (hit.grab.kind === 'label') {
      const last = this.lastLabelPress;
      const now = performance.now();
      if (last?.zoneId === hit.zone.id && now - last.time < DOUBLE_CLICK_MS) {
        this.lastLabelPress = null;
        this.zoneEdit.onRename(hit.zone.id);
        return true;
      }
      this.lastLabelPress = { zoneId: hit.zone.id, time: now };
    }
    this.zoneDrag = {
      zone: hit.zone,
      grab: hit.grab,
      from: at,
      start: { x: event.global.x, y: event.global.y },
      moved: false,
    };
    return true;
  }

  private zoneMove(event: FederatedPointerEvent): void {
    const drag = this.zoneDrag;
    if (drag) {
      if (!drag.moved) {
        const distance = Math.hypot(event.global.x - drag.start.x, event.global.y - drag.start.y);
        if (distance < CLICK_SLOP_PX) return;
        drag.moved = true;
        this.lastLabelPress = null;
      }
      const zone = this.draggedZone(drag, event);
      if (zone) {
        this.zoneOverride = { zone, sent: false };
        this.drawZones();
      }
      return;
    }
    const hit = this.zoneGrabAt(this.at16(event));
    this.app.canvas.style.cursor = !hit
      ? 'default'
      : hit.grab.kind === 'handle'
        ? 'crosshair'
        : 'move';
  }

  private zoneUp(event: FederatedPointerEvent): void {
    const drag = this.zoneDrag;
    this.zoneDrag = null;
    if (!drag?.moved) return;
    const zone = this.draggedZone(drag, event);
    if (!zone) {
      this.zoneOverride = null;
      this.drawZones();
      return;
    }
    const edit: ZoneEdit = {};
    if (JSON.stringify(zone.shape) !== JSON.stringify(drag.zone.shape)) edit.shape = zone.shape;
    if (zone.label && JSON.stringify(zone.label) !== JSON.stringify(drag.zone.label)) {
      edit.label = zone.label;
    }
    if (!edit.shape && !edit.label) {
      this.zoneOverride = null;
      this.drawZones();
      return;
    }
    this.zoneOverride = { zone, sent: true };
    this.drawZones();
    this.zoneEdit.onEdit(drag.zone, edit);
  }

  private cancelDrawing(): void {
    this.drawing = null;
    this.preview.clear();
  }

  /** Stops a token drag without sending anything more. */
  private endTokenDrag(): void {
    this.moves.cancel();
    this.tokenDrag = null;
    this.tokens.release();
  }

  /** Where the dragged token is now, in map units. */
  private tokenDragPoint(event: FederatedPointerEvent): Point {
    const at = this.world.toLocal(event.global);
    const drag = this.tokenDrag;
    return drag ? { x: at.x - drag.dx, y: at.y - drag.dy } : at;
  }

  private readonly onGrabToken = (id: string, event: FederatedPointerEvent): void => {
    const view = this.tokenViews.find((v) => v.token.id === id);
    if (!view || this.tool) return;
    if (this.combatMovement) {
      this.movementTokenId = id;
      this.onSelectToken(id);
      this.movementPreview.clear();
      this.movementLabel.visible = false;
      this.app.canvas.style.cursor = 'crosshair';
      return;
    }
    const at = this.world.toLocal(event.global);
    this.tokenDrag = {
      id,
      dx: at.x - view.token.x / 16,
      dy: at.y - view.token.y / 16,
      start: { x: event.global.x, y: event.global.y },
      moved: false,
    };
  };

  /** The shapes the current drawing would send. */
  private drawingShapes(drawing: Drawing, map: Size): FogShape[] {
    if (drawing.tool === 'brush') return drawing.stroke.shapes();
    const rect = rectFromDrag(drawing.from, drawing.to, map);
    return rect ? [rect] : [];
  }

  private drawPreview(): void {
    this.preview.clear();
    const drawing = this.drawing;
    if (!drawing || !this.mapSize) return;
    const color =
      drawing.tool === 'rect' && drawing.zone
        ? ZONE_PREVIEW
        : drawing.kind === 'reveal'
          ? REVEAL_PREVIEW
          : HIDE_PREVIEW;
    for (const shape of this.drawingShapes(drawing, this.mapSize)) {
      drawShape(this.preview, shape, color, PREVIEW_ALPHA);
    }
  }

  /** The brush outline under the pointer, so the GM sees its size before drawing. */
  private drawBrushCursor(at: Point, radius: number): void {
    this.preview.clear();
    this.preview
      .circle(at.x * 16, at.y * 16, brushRadius16(radius))
      .stroke({ color: 0xffffff, alpha: 0.85, width: (1.5 * 16) / this.camera.zoom });
  }

  private readonly onResize = (): void => {
    if (!this.mapSize) return;
    if (this.pendingFit) {
      this.fit();
    } else {
      this.setCamera(clampCamera(this.camera, this.screenSize(), this.mapSize));
    }
  };

  private readonly onWheel = (event: WheelEvent): void => {
    event.preventDefault();
    if (!this.mapSize) return;
    this.setCamera(
      zoomAt(
        this.camera,
        { x: event.offsetX, y: event.offsetY },
        wheelFactor(event.deltaY, event.deltaMode),
        this.screenSize(),
        this.mapSize,
      ),
    );
  };

  private readonly onDown = (event: FederatedPointerEvent): void => {
    // With a fog tool the left button draws; the middle button always pans. Without a tool the
    // left button pans too (tokens will claim left drags on themselves, step 7).
    if (event.button === 0 && this.tool && this.mapSize) {
      const at = this.world.toLocal(event.global);
      if (this.tool.shape === 'rect' || this.tool.shape === 'zone') {
        const zone = this.tool.shape === 'zone';
        this.drawing = { tool: 'rect', kind: this.tool.kind, from: at, to: at, zone };
      } else {
        const stroke = new BrushStroke(brushRadius16(this.tool.radius), this.mapSize);
        stroke.add(at);
        this.drawing = { tool: 'brush', kind: this.tool.kind, stroke };
      }
      this.drawPreview();
      return;
    }
    // In zone edit mode the left button works on zones; on the empty map it still pans.
    if (event.button === 0 && this.zoneEditing && this.mapSize && this.zoneDown(event)) return;
    if (event.button === 0 && this.combatMovement && this.movementTokenId && this.mapSize) {
      this.movementPress = true;
      this.drawMovementPath(this.world.toLocal(event.global));
      return;
    }
    if (event.button !== 0 && event.button !== 1) return;
    this.dragFrom = { x: event.global.x, y: event.global.y };
    this.panStart = event.button === 0 ? { ...this.dragFrom } : null;
    this.app.canvas.style.cursor = 'grabbing';
  };

  private readonly onMove = (event: FederatedPointerEvent): void => {
    if (!this.mapSize) return;
    if (this.combatMovement && this.movementTokenId && !this.tokenDrag) {
      this.drawMovementPath(this.world.toLocal(event.global));
      return;
    }
    const drag = this.tokenDrag;
    if (drag) {
      if (!drag.moved) {
        const distance = Math.hypot(event.global.x - drag.start.x, event.global.y - drag.start.y);
        if (distance < CLICK_SLOP_PX) return;
        drag.moved = true;
      }
      const p = this.tokenDragPoint(event);
      const clamped = tokenPosition(p, this.mapSize);
      this.tokens.dragTo(drag.id, clamped.x / 16, clamped.y / 16);
      this.moves.push({ id: drag.id, ...clamped, final: false });
      return;
    }
    if (this.zoneEditing && !this.dragFrom) {
      this.zoneMove(event);
      return;
    }
    const drawing = this.drawing;
    if (drawing) {
      const at = this.world.toLocal(event.global);
      if (drawing.tool === 'rect') drawing.to = at;
      else drawing.stroke.add(at);
      this.drawPreview();
      return;
    }
    if (this.dragFrom) {
      const dx = event.global.x - this.dragFrom.x;
      const dy = event.global.y - this.dragFrom.y;
      this.dragFrom = { x: event.global.x, y: event.global.y };
      this.setCamera(panBy(this.camera, dx, dy, this.screenSize(), this.mapSize));
      return;
    }
    if (this.tool?.shape === 'brush') {
      this.drawBrushCursor(this.world.toLocal(event.global), this.tool.radius);
    }
  };

  private readonly onUp = (event: FederatedPointerEvent): void => {
    if (this.movementPress) {
      this.movementPress = false;
      const path = this.movementPathAt(this.world.toLocal(event.global));
      const token = this.tokenViews.find((view) => view.token.id === this.movementTokenId)?.token;
      const destination = path?.at(-1);
      if (token && destination && this.mapSize) {
        const final = tokenPosition(destination, this.mapSize);
        if (final.x !== token.x || final.y !== token.y) {
          this.onMoveToken({ id: token.id, ...final, final: true });
        }
      }
      this.clearMovementPlan();
      return;
    }
    if (this.zoneDrag) {
      this.zoneUp(event);
      return;
    }
    const drag = this.tokenDrag;
    if (drag && !drag.moved) {
      // A click: select the token, do not move it.
      this.endTokenDrag();
      this.onSelectToken(drag.id);
      return;
    }
    if (drag && this.mapSize) {
      // Drop: snapped to the grid unless Alt is held; replaces any move still waiting.
      const p = this.tokenDragPoint(event);
      const snapped = event.altKey || !this.grid ? p : snapToGrid(p, this.grid);
      const final = tokenPosition(snapped, this.mapSize);
      this.moves.cancel();
      this.tokens.dragTo(drag.id, final.x / 16, final.y / 16);
      this.onMoveToken({ id: drag.id, ...final, final: true });
      this.tokenDrag = null;
      this.tokens.release(final);
      return;
    }
    const drawing = this.drawing;
    if (drawing && this.mapSize) {
      this.cancelDrawing();
      const shapes = this.drawingShapes(drawing, this.mapSize);
      const [rect] = shapes;
      if (drawing.tool === 'rect' && drawing.zone) {
        if (rect?.type === 'rect' && rect.width > 0 && rect.height > 0) this.onZone(rect);
      } else if (shapes.length > 0) {
        this.onFog(drawing.kind, shapes);
      }
    }
    if (this.dragFrom) {
      const start = this.panStart;
      const clicked =
        start !== null &&
        Math.hypot(event.global.x - start.x, event.global.y - start.y) < CLICK_SLOP_PX;
      this.dragFrom = null;
      this.panStart = null;
      this.app.canvas.style.cursor = this.idleCursor();
      if (clicked) this.onSelectToken(null);
    }
  };

  private movementPathAt(destination: Point): Point[] | null {
    const token = this.tokenViews.find((view) => view.token.id === this.movementTokenId)?.token;
    if (!token || !this.grid || !this.mapSize) return null;
    return shortestMovementPath(
      { x: token.x / 16, y: token.y / 16 },
      destination,
      this.grid,
      this.mapSize,
    );
  }

  private drawMovementPath(destination: Point): void {
    this.movementPreview.clear();
    this.movementLabel.visible = false;
    const path = this.movementPathAt(destination);
    if (!path || path.length < 2 || !this.grid) return;
    const first = path[0];
    if (!first) return;
    const lineWidth = (6 * 16) / this.camera.zoom;
    const radius = (4 * 16) / this.camera.zoom;
    this.movementPreview.moveTo(first.x * 16, first.y * 16);
    for (const point of path.slice(1)) this.movementPreview.lineTo(point.x * 16, point.y * 16);
    this.movementPreview.stroke({ color: MOVEMENT_PATH, alpha: 0.6, width: lineWidth });
    for (const point of path) {
      this.movementPreview
        .circle(point.x * 16, point.y * 16, radius)
        .fill({ color: MOVEMENT_PATH, alpha: 0.5 });
    }
    const end = path.at(-1);
    if (end) {
      this.movementLabel.text = `${movementDistance(path, this.grid).toLocaleString(i18n.language, { maximumFractionDigits: 1 })} ${i18n.t(this.grid.kind === 'none' ? 'maps.distanceUnits' : 'maps.distanceCells')}`;
      this.movementLabel.position.set(end.x + 12 / this.camera.zoom, end.y - 24 / this.camera.zoom);
      this.movementLabel.scale.set(1 / this.camera.zoom);
      this.movementLabel.visible = true;
    }
  }

  private clearMovementPlan(): void {
    this.movementLabel.visible = false;
    this.movementTokenId = null;
    this.movementPress = false;
    this.movementPreview.clear();
    this.app.canvas.style.cursor = this.idleCursor();
  }
}
