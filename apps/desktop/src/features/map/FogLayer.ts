// Fog of war on the canvas (ADR-0019): the operations are drawn into one render texture of at most
// FOG_TEXTURE_PX, stretched over the map. New operations are drawn on top; a reset, another map or
// a changed size redraws everything.
import { Container, Graphics, Matrix, RenderTexture, Sprite, type Application } from 'pixi.js';
import type { FogDto, FogOp, FogShape } from '@/shared/ipc/client';

/** Longest side of the fog texture. Fog edges are soft anyway; memory stays at 16 MiB. */
export const FOG_TEXTURE_PX = 2048;
const FOG_COLOR = 0x0b0a10;
/** GM roles see through the fog to work on the map; players do not. */
const GM_FOG_ALPHA = 0.55;

/** Draws one shape (Units16) into `g` with the given fill. */
export function drawShape(g: Graphics, shape: FogShape, color: number, alpha = 1): void {
  if (shape.type === 'rect') {
    g.rect(shape.x, shape.y, shape.width, shape.height).fill({ color, alpha });
    return;
  }
  const { points, radius } = shape;
  for (let i = 0; i + 1 < points.length; i += 2) {
    g.circle(points[i] ?? 0, points[i + 1] ?? 0, radius);
  }
  g.fill({ color, alpha });
  if (points.length >= 4) {
    g.moveTo(points[0] ?? 0, points[1] ?? 0);
    for (let i = 2; i + 1 < points.length; i += 2) g.lineTo(points[i] ?? 0, points[i + 1] ?? 0);
    g.stroke({ color, alpha, width: radius * 2, cap: 'round', join: 'round' });
  }
}

export class FogLayer {
  readonly view = new Container();
  private readonly sprite = new Sprite();
  private texture: RenderTexture | null = null;
  /** Units16 → texture pixels. */
  private toTexture = new Matrix();
  /** Map size in Units16. */
  private size16 = { width: 0, height: 0 };
  private key: string | null = null;
  private drawn: FogOp[] = [];

  constructor(private readonly app: Application) {
    this.view.addChild(this.sprite);
    this.view.eventMode = 'none';
  }

  /** Shows `fog` over a map of `width × height` logical units, for a GM or a player. */
  show(fog: FogDto | null, width: number, height: number, gmView: boolean): void {
    this.sprite.alpha = gmView ? GM_FOG_ALPHA : 1;
    if (!fog) {
      this.view.visible = false;
      this.key = null;
      this.drawn = [];
      return;
    }
    this.view.visible = true;
    const key = `${fog.map_id}:${width}x${height}:${fog.covered}`;
    // Only appended operations can be drawn on top; anything else (reset, another map, an
    // operation list that does not extend what is drawn) redraws from scratch.
    const extends_ =
      key === this.key &&
      fog.ops.length >= this.drawn.length &&
      this.drawn.every((op, i) => fog.ops[i]?.id === op.id);
    if (!extends_) {
      this.resize(width, height);
      this.key = key;
      this.drawn = [];
      this.render(fog.covered ? [{ cover: true }] : [], true);
    }
    const fresh = fog.ops.slice(this.drawn.length);
    if (fresh.length > 0) {
      this.render(
        fresh.map((op) => ({ op })),
        false,
      );
      this.drawn = [...fog.ops];
    }
  }

  destroy(): void {
    this.view.destroy({ children: true });
    this.texture?.destroy(true);
    this.texture = null;
  }

  private resize(width: number, height: number): void {
    const scale = Math.min(1, FOG_TEXTURE_PX / Math.max(width, height));
    const tw = Math.max(1, Math.ceil(width * scale));
    const th = Math.max(1, Math.ceil(height * scale));
    if (!this.texture || this.texture.width !== tw || this.texture.height !== th) {
      this.texture?.destroy(true);
      this.texture = RenderTexture.create({ width: tw, height: th, resolution: 1 });
      this.sprite.texture = this.texture;
    }
    this.size16 = { width: width * 16, height: height * 16 };
    this.toTexture = new Matrix().scale(tw / this.size16.width, th / this.size16.height);
    this.sprite.width = width;
    this.sprite.height = height;
  }

  private render(items: ({ cover: true } | { op: FogOp })[], clear: boolean): void {
    if (!this.texture) return;
    const batch = new Container();
    // Operations must apply in order (hide then reveal ≠ reveal then hide): one Graphics per run
    // of the same kind keeps draw calls low without reordering.
    let current: Graphics | null = null;
    let currentKind: 'hide' | 'reveal' | null = null;
    for (const item of items) {
      const kind = 'cover' in item ? 'hide' : item.op.kind;
      if (kind !== currentKind) {
        current = new Graphics();
        if (kind === 'reveal') current.blendMode = 'erase';
        batch.addChild(current);
        currentKind = kind;
      }
      if (!current) continue;
      if ('cover' in item) {
        current.rect(0, 0, this.size16.width, this.size16.height).fill({ color: FOG_COLOR });
      } else {
        drawShape(current, item.op.shape, FOG_COLOR);
      }
    }
    this.app.renderer.render({
      container: batch,
      target: this.texture,
      clear,
      transform: this.toTexture,
    });
    batch.destroy({ children: true });
  }
}
