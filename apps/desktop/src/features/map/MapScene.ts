// Imperative Pixi scene of one map (ADR-0017): tiled background, grid, camera. React only feeds
// it the selected map through `show`; nothing here is rendered by React.
import {
  Container,
  Graphics,
  Rectangle,
  Sprite,
  Texture,
  TilingSprite,
  type Application,
  type FederatedPointerEvent,
} from 'pixi.js';
import { gridPattern } from './gridPattern';
import {
  clampCamera,
  fitCamera,
  panBy,
  wheelFactor,
  zoomAt,
  type Camera,
  type Size,
} from './viewport';
import type { AssetDto, MapDto } from '@/shared/ipc/client';
import { assetUrl } from '@/shared/ipc/client';

/** Shown under a map without a background image, so its bounds stay visible. */
const BLANK_MAP_COLOR = 0xd9d4c7;
const GRID_COLOR = 0x000000;
const GRID_ALPHA = 0.35;
/** The grid texture is drawn at least this many pixels wide, so it stays sharp when zoomed. */
const GRID_TEXTURE_PX = 256;

async function loadBitmap(sha256: string): Promise<ImageBitmap> {
  const response = await fetch(assetUrl(sha256));
  if (!response.ok) throw new Error(`asset ${sha256}: HTTP ${response.status}`);
  return createImageBitmap(await response.blob());
}

export class MapScene {
  private readonly world = new Container();
  private readonly background = new Container();
  private readonly gridLayer = new Container();
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

  constructor(
    private readonly app: Application,
    private readonly onLoadError: (error: unknown) => void,
  ) {
    this.world.addChild(this.background, this.gridLayer);
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

  /** Shows `map` (or nothing). Only what changed is rebuilt. */
  show(map: MapDto | null, background: AssetDto | null): void {
    this.world.visible = map !== null;
    if (!map) {
      this.mapId = null;
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
    if (map.id !== this.mapId || sizeChanged) {
      this.mapId = map.id;
      this.fit();
    }
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
    // Left or middle button pans (tokens will claim left drags on themselves, step 7).
    if (event.button !== 0 && event.button !== 1) return;
    this.dragFrom = { x: event.global.x, y: event.global.y };
    this.app.canvas.style.cursor = 'grabbing';
  };

  private readonly onMove = (event: FederatedPointerEvent): void => {
    if (!this.dragFrom || !this.mapSize) return;
    const dx = event.global.x - this.dragFrom.x;
    const dy = event.global.y - this.dragFrom.y;
    this.dragFrom = { x: event.global.x, y: event.global.y };
    this.setCamera(panBy(this.camera, dx, dy, this.screenSize(), this.mapSize));
  };

  private readonly onUp = (): void => {
    this.dragFrom = null;
    this.app.canvas.style.cursor = 'grab';
  };
}
