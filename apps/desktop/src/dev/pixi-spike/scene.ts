// Dev-only PixiJS render spike (ADR-0017). Imperative scene: React only hosts the canvas and HUD.
import {
  Container,
  Graphics,
  Sprite,
  Texture,
  TilingSprite,
  type Application,
  type FederatedPointerEvent,
  type WebGLRenderer,
  UPDATE_PRIORITY,
} from 'pixi.js';

// Must match tools/spike-map-gen.
export const MAP_SIZE = 8000;
export const TILE_SIZE = 2048;
const ASSET_BASE = '/src/dev/pixi-spike/generated/';

const CELL = 70;
const TOKEN_COUNT = 200;
const MOVING_TOKENS = 10;
const TOKEN_KINDS = 8;
export const MEASURE_MS = 10_000;

export type TextureMode = 'single' | 'tiles';
export type GridMode = 'graphics' | 'tiling';
export interface Variant {
  texture: TextureMode;
  grid: GridMode;
}

export interface LoadTimings {
  fetchMs: number;
  decodeMs: number;
  uploadMs: number;
  firstFrameMs: number;
  textures: number;
  textureBytes: number;
}

export interface FrameStats {
  avgFps: number;
  /** Lowest frame count over the whole 1 s windows of the run. */
  minFps: number;
  /** 99th percentile frame time. */
  p99FrameMs: number;
  maxFrameMs: number;
  /** CPU time of `app.render()` (scene traversal + GL submit); FPS alone is capped by vsync. */
  renderAvgMs: number;
  renderP99Ms: number;
  frames: number;
}

export interface GpuInfo {
  renderer: string;
  maxTextureSize: number;
  webgl: number;
}

export interface MemoryInfo {
  usedJsHeapMb: number;
  totalJsHeapMb: number;
}

interface ChromiumMemory {
  usedJSHeapSize: number;
  totalJSHeapSize: number;
}

const mb = (bytes: number) => Math.round((bytes / 1024 / 1024) * 10) / 10;
const round1 = (n: number) => Math.round(n * 10) / 10;

export function gpuInfo(app: Application): GpuInfo {
  const { gl } = app.renderer as WebGLRenderer;
  const debug = gl.getExtension('WEBGL_debug_renderer_info');
  const renderer = debug
    ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL))
    : String(gl.getParameter(gl.RENDERER));
  return {
    renderer,
    maxTextureSize: Number(gl.getParameter(gl.MAX_TEXTURE_SIZE)),
    webgl: gl instanceof WebGL2RenderingContext ? 2 : 1,
  };
}

export function memoryInfo(): MemoryInfo | null {
  const memory = (performance as Performance & { memory?: ChromiumMemory }).memory;
  return memory
    ? { usedJsHeapMb: mb(memory.usedJSHeapSize), totalJsHeapMb: mb(memory.totalJSHeapSize) }
    : null;
}

function mapUrls(mode: TextureMode): { url: string; x: number; y: number }[] {
  if (mode === 'single') return [{ url: `${ASSET_BASE}map-${MAP_SIZE}.webp`, x: 0, y: 0 }];
  const tiles = Math.ceil(MAP_SIZE / TILE_SIZE);
  const out = [];
  for (let row = 0; row < tiles; row++) {
    for (let col = 0; col < tiles; col++) {
      out.push({
        url: `${ASSET_BASE}tile-${col}-${row}.webp`,
        x: col * TILE_SIZE,
        y: row * TILE_SIZE,
      });
    }
  }
  return out;
}

async function fetchBlob(url: string): Promise<Blob> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const blob = await response.blob();
  // Vite answers unknown paths with index.html.
  if (!blob.type.startsWith('image/')) {
    throw new Error(`${url} is missing — run: cargo run --release -p dnd-spike-map-gen`);
  }
  return blob;
}

const nextFrame = () =>
  new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });

/** One variant on stage: map, grid, tokens and a camera. */
export class SpikeScene {
  readonly world = new Container();
  private readonly tokens: Sprite[] = [];
  private readonly ownedTextures: Texture[] = [];
  private bitmaps: ImageBitmap[] = [];
  private zoom = 1;
  private dragging: { x: number; y: number } | null = null;
  private autoCamera = false;
  private elapsed = 0;

  constructor(
    private readonly app: Application,
    readonly variant: Variant,
  ) {}

  /** Loads the map and builds the stage; timings split into network, decode and GPU upload. */
  async load(): Promise<LoadTimings> {
    const renderer = this.app.renderer as WebGLRenderer;
    const parts = mapUrls(this.variant.texture);

    const t0 = performance.now();
    const blobs = await Promise.all(parts.map((p) => fetchBlob(p.url)));
    const t1 = performance.now();
    this.bitmaps = await Promise.all(blobs.map((b) => createImageBitmap(b)));
    const t2 = performance.now();

    const mapLayer = new Container();
    let textureBytes = 0;
    parts.forEach((part, i) => {
      const bitmap = this.bitmaps[i];
      if (!bitmap) return;
      const texture = Texture.from(bitmap);
      this.ownedTextures.push(texture);
      renderer.texture.initSource(texture.source);
      textureBytes += bitmap.width * bitmap.height * 4;
      const sprite = new Sprite(texture);
      sprite.position.set(part.x, part.y);
      mapLayer.addChild(sprite);
    });
    renderer.gl.finish();
    const t3 = performance.now();

    this.world.addChild(mapLayer, this.buildGrid(), this.buildTokens());
    this.app.stage.addChild(this.world);
    this.fit();
    this.app.render();
    renderer.gl.finish();
    const t4 = performance.now();

    textureBytes += this.ownedTextures
      .slice(parts.length)
      .reduce((sum, t) => sum + t.source.pixelWidth * t.source.pixelHeight * 4, 0);

    this.app.ticker.add(this.tick);
    this.bindInput();
    return {
      fetchMs: round1(t1 - t0),
      decodeMs: round1(t2 - t1),
      uploadMs: round1(t3 - t2),
      firstFrameMs: round1(t4 - t3),
      textures: this.ownedTextures.length,
      textureBytes,
    };
  }

  /** Runs the animated camera for `ms` and collects frame times. */
  async measure(ms: number): Promise<FrameStats> {
    this.autoCamera = true;
    this.elapsed = 0;
    await nextFrame();
    // Application renders on the ticker at LOW priority: bracket it from both sides.
    const renderTimes: number[] = [];
    let renderStart = 0;
    const before = () => {
      renderStart = performance.now();
    };
    const after = () => {
      renderTimes.push(performance.now() - renderStart);
    };
    this.app.ticker.add(before, undefined, UPDATE_PRIORITY.LOW + 1);
    this.app.ticker.add(after, undefined, UPDATE_PRIORITY.UTILITY);
    const stamps: number[] = [];
    const start = performance.now();
    while (performance.now() - start < ms) {
      await nextFrame();
      stamps.push(performance.now());
    }
    this.autoCamera = false;
    this.app.ticker.remove(before);
    this.app.ticker.remove(after);
    const renders = [...renderTimes].sort((a, b) => a - b);

    const deltas = stamps.slice(1).map((t, i) => t - (stamps[i] ?? t));
    const sorted = [...deltas].sort((a, b) => a - b);
    const seconds = Math.floor(ms / 1000);
    const perSecond = Array.from({ length: seconds }, () => 0);
    for (const t of stamps) {
      const bucket = Math.floor((t - start) / 1000);
      if (bucket < seconds) perSecond[bucket] = (perSecond[bucket] ?? 0) + 1;
    }
    const total = (stamps.at(-1) ?? start) - start;
    return {
      avgFps: round1((stamps.length / total) * 1000),
      minFps: Math.min(...perSecond),
      p99FrameMs: round1(sorted[Math.floor(sorted.length * 0.99)] ?? 0),
      maxFrameMs: round1(sorted.at(-1) ?? 0),
      renderAvgMs: round1(renders.reduce((sum, t) => sum + t, 0) / Math.max(1, renders.length)),
      renderP99Ms: round1(renders[Math.floor(renders.length * 0.99)] ?? 0),
      frames: stamps.length,
    };
  }

  destroy(): void {
    this.app.ticker.remove(this.tick);
    this.unbindInput();
    this.app.stage.removeChild(this.world);
    this.world.destroy({ children: true });
    for (const texture of this.ownedTextures) texture.destroy(true);
    for (const bitmap of this.bitmaps) bitmap.close();
  }

  private buildGrid(): Container {
    const lineColor = { color: 0x000000, alpha: 0.35, width: 2 };
    if (this.variant.grid === 'graphics') {
      const g = new Graphics();
      for (let x = 0; x <= MAP_SIZE; x += CELL) g.moveTo(x, 0).lineTo(x, MAP_SIZE);
      for (let y = 0; y <= MAP_SIZE; y += CELL) g.moveTo(0, y).lineTo(MAP_SIZE, y);
      g.stroke(lineColor);
      return g;
    }
    const cell = new Graphics()
      .rect(0, 0, CELL, CELL)
      .fill({ color: 0x000000, alpha: 0 })
      .moveTo(0, 0)
      .lineTo(CELL, 0)
      .moveTo(0, 0)
      .lineTo(0, CELL)
      .stroke(lineColor);
    const texture = this.app.renderer.generateTexture({ target: cell, resolution: 1 });
    cell.destroy();
    this.ownedTextures.push(texture);
    return new TilingSprite({ texture, width: MAP_SIZE, height: MAP_SIZE });
  }

  private buildTokens(): Container {
    const layer = new Container();
    const kinds = Array.from({ length: TOKEN_KINDS }, (_, i) => {
      const g = new Graphics()
        .circle(CELL / 2, CELL / 2, CELL / 2 - 4)
        .fill({ color: (0x3355ff + i * 0x2a1f13) & 0xffffff })
        .stroke({ color: 0xffffff, width: 3 });
      const texture = this.app.renderer.generateTexture(g);
      g.destroy();
      this.ownedTextures.push(texture);
      return texture;
    });
    for (let i = 0; i < TOKEN_COUNT; i++) {
      const sprite = new Sprite(kinds[i % TOKEN_KINDS]);
      sprite.anchor.set(0.5);
      // Deterministic scatter, snapped to the grid.
      const col = (i * 37) % Math.floor(MAP_SIZE / CELL);
      const row = (i * 53) % Math.floor(MAP_SIZE / CELL);
      sprite.position.set(col * CELL + CELL / 2, row * CELL + CELL / 2);
      this.tokens.push(sprite);
      layer.addChild(sprite);
    }
    return layer;
  }

  private readonly tick = (): void => {
    const dt = this.app.ticker.deltaMS;
    this.elapsed += dt;
    const t = this.elapsed / 1000;
    for (let i = 0; i < MOVING_TOKENS; i++) {
      const token = this.tokens[i];
      if (!token) continue;
      const cx = 1000 + i * 600;
      token.position.set(cx + Math.cos(t + i) * 300, 4000 + Math.sin(t * 1.3 + i) * 300);
      token.rotation = t;
    }
    if (this.autoCamera) {
      // Sweep from "whole map" to close-up and pan across, so every tile gets drawn.
      const fit = this.fitZoom();
      this.zoom = fit + (1.5 - fit) * (0.5 - 0.5 * Math.cos(t * 1.2));
      this.centerOn(
        MAP_SIZE / 2 + Math.sin(t * 0.7) * MAP_SIZE * 0.4,
        MAP_SIZE / 2 + Math.cos(t * 0.5) * MAP_SIZE * 0.4,
      );
    }
  };

  private fitZoom(): number {
    const { width, height } = this.app.screen;
    return Math.min(width, height) / MAP_SIZE;
  }

  private fit(): void {
    this.zoom = this.fitZoom();
    this.centerOn(MAP_SIZE / 2, MAP_SIZE / 2);
  }

  private centerOn(x: number, y: number): void {
    const { width, height } = this.app.screen;
    this.world.scale.set(this.zoom);
    this.world.position.set(width / 2 - x * this.zoom, height / 2 - y * this.zoom);
  }

  private readonly onWheel = (event: WheelEvent): void => {
    event.preventDefault();
    if (this.autoCamera) return;
    const before = this.world.toLocal({ x: event.offsetX, y: event.offsetY });
    this.zoom = Math.min(
      4,
      Math.max(this.fitZoom() / 2, this.zoom * Math.exp(-event.deltaY / 500)),
    );
    this.world.scale.set(this.zoom);
    // Keep the point under the cursor fixed.
    this.world.position.set(
      event.offsetX - before.x * this.zoom,
      event.offsetY - before.y * this.zoom,
    );
  };

  private readonly onDown = (event: FederatedPointerEvent): void => {
    this.dragging = { x: event.global.x - this.world.x, y: event.global.y - this.world.y };
  };

  private readonly onMove = (event: FederatedPointerEvent): void => {
    if (!this.dragging || this.autoCamera) return;
    this.world.position.set(event.global.x - this.dragging.x, event.global.y - this.dragging.y);
  };

  private readonly onUp = (): void => {
    this.dragging = null;
  };

  private bindInput(): void {
    const stage = this.app.stage;
    stage.eventMode = 'static';
    stage.hitArea = this.app.screen;
    stage.on('pointerdown', this.onDown).on('globalpointermove', this.onMove);
    stage.on('pointerup', this.onUp).on('pointerupoutside', this.onUp);
    this.app.canvas.addEventListener('wheel', this.onWheel, { passive: false });
  }

  private unbindInput(): void {
    const stage = this.app.stage;
    stage.off('pointerdown', this.onDown).off('globalpointermove', this.onMove);
    stage.off('pointerup', this.onUp).off('pointerupoutside', this.onUp);
    this.app.canvas.removeEventListener('wheel', this.onWheel);
  }
}
