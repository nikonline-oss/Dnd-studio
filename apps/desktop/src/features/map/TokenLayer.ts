// Tokens on the canvas (ADR-0017: Pixi is driven imperatively, React never renders a token).
// One container per token, updated in place; positions come in Units16 (ADR-0016).
import { Container, Graphics, Text, type FederatedPointerEvent } from 'pixi.js';
import { tokenDiameter } from './tokenMath';
import { tokenColor, tokenInitial } from '@/shared/game/tokenLook';
import type { Grid, TokenDto } from '@/shared/ipc/client';

/** Ring around the token whose turn it is (the accent colour). */
const ACTIVE_RING = 0xe3aa45;

/** How the viewer sees a token. */
export interface TokenView {
  token: TokenDto;
  /** The viewer may drag it (the core decides for real). */
  movable: boolean;
  /** Seen only by GM roles (GM layer or hidden): drawn translucent. */
  gmOnly: boolean;
  /** Colour key: the character, else the token itself. */
  colorKey: string;
  /** It is this token's turn in combat: drawn with a ring. */
  active: boolean;
  /** Selected (its properties are in the inspector): drawn with a white ring. */
  selected: boolean;
}

interface Sprite {
  root: Container;
  disc: Graphics;
  label: Text;
  /** Name under the token, created on first hover (hundreds of tokens, few hovered). */
  name: Text | null;
  /** Diameter the name is laid out for. */
  radius: number;
  title: string;
  /** What the sprite was drawn for; redrawn when it changes. */
  look: string;
}

/** Pointer pressed on a token: the scene starts a drag. */
export type TokenGrab = (id: string, event: FederatedPointerEvent) => void;

export class TokenLayer {
  readonly view = new Container();
  private readonly sprites = new Map<string, Sprite>();
  /** The token being dragged keeps its local position until dropped. */
  private dragging: string | null = null;

  constructor(private readonly onGrab: TokenGrab) {
    this.view.sortableChildren = true;
  }

  /** Shows exactly `tokens` (sorted by layer by the caller), on a map with `grid`. */
  show(tokens: TokenView[], grid: Grid, interactive: boolean): void {
    const seen = new Set<string>();
    tokens.forEach((view, order) => {
      const { token } = view;
      seen.add(token.id);
      let sprite = this.sprites.get(token.id);
      if (!sprite) {
        sprite = this.create(token.id);
        this.sprites.set(token.id, sprite);
        this.view.addChild(sprite.root);
      }
      const diameter = tokenDiameter(grid, token.scale);
      const look = `${diameter}:${token.name}:${view.colorKey}:${view.gmOnly}:${view.active}:${view.selected}`;
      if (look !== sprite.look) {
        this.draw(sprite, view, diameter);
        sprite.look = look;
      }
      sprite.root.zIndex = order;
      sprite.root.angle = token.rotation;
      sprite.root.eventMode = interactive && view.movable ? 'static' : 'none';
      sprite.root.cursor = 'grab';
      if (this.dragging !== token.id) sprite.root.position.set(token.x / 16, token.y / 16);
    });
    for (const [id, sprite] of this.sprites) {
      if (seen.has(id)) continue;
      sprite.root.destroy({ children: true });
      this.sprites.delete(id);
      if (this.dragging === id) this.dragging = null;
    }
  }

  /** Moves a token locally while it is dragged (map units). */
  dragTo(id: string, x: number, y: number): void {
    const sprite = this.sprites.get(id);
    if (!sprite) return;
    this.dragging = id;
    sprite.root.position.set(x, y);
    sprite.root.zIndex = Number.MAX_SAFE_INTEGER;
    sprite.root.cursor = 'grabbing';
  }

  /** The drag ended; the next `show` places the token where the events say. */
  release(): void {
    const sprite = this.dragging ? this.sprites.get(this.dragging) : undefined;
    if (sprite) sprite.root.cursor = 'grab';
    this.dragging = null;
  }

  destroy(): void {
    this.view.destroy({ children: true });
    this.sprites.clear();
  }

  private create(id: string): Sprite {
    const root = new Container();
    const disc = new Graphics();
    const label = new Text({
      text: '',
      style: { fill: 0xffffff, fontWeight: '700', fontFamily: 'system-ui, sans-serif' },
    });
    label.anchor.set(0.5);
    // Text is rasterised at its font size and zoomed with the map: render it sharper.
    label.resolution = 3;
    root.addChild(disc, label);
    // Off-screen tokens are skipped by the renderer (the app registers the culler).
    root.cullable = true;
    root.on('pointerdown', (event: FederatedPointerEvent) => {
      if (event.button !== 0) return;
      event.stopPropagation();
      this.onGrab(id, event);
    });
    const sprite: Sprite = { root, disc, label, name: null, radius: 0, title: '', look: '' };
    root.on('pointerover', () => {
      this.nameOf(sprite).visible = true;
    });
    root.on('pointerout', () => {
      if (sprite.name) sprite.name.visible = false;
    });
    return sprite;
  }

  /** The name label, created on demand and kept in step with the token. */
  private nameOf(sprite: Sprite): Text {
    if (!sprite.name) {
      sprite.name = new Text({
        text: '',
        style: {
          fill: 0xffffff,
          fontFamily: 'system-ui, sans-serif',
          stroke: { color: 0x000000, width: 4, join: 'round' },
        },
      });
      sprite.name.anchor.set(0.5, 0);
      sprite.name.resolution = 3;
      sprite.root.addChild(sprite.name);
    }
    const name = sprite.name;
    name.text = sprite.title;
    name.style.fontSize = Math.max(10, sprite.radius * 0.45);
    name.position.set(0, sprite.radius + 4);
    return name;
  }

  private draw(sprite: Sprite, view: TokenView, diameter: number): void {
    const r = diameter / 2;
    const color = tokenColor(view.colorKey);
    sprite.disc
      .clear()
      .circle(0, 0, r)
      .fill({ color })
      .stroke({ color: 0xffffff, width: Math.max(1.5, r * 0.08), alignment: 1 })
      .circle(0, 0, r)
      .stroke({ color: 0x000000, alpha: 0.35, width: Math.max(1, r * 0.04), alignment: 0 });
    if (view.selected) {
      sprite.disc
        .circle(0, 0, r + Math.max(1.5, r * 0.06))
        .stroke({ color: 0xffffff, width: Math.max(2, r * 0.1) });
    }
    if (view.active) {
      // Whose turn it is: a bright ring outside the token.
      sprite.disc
        .circle(0, 0, r + Math.max(3, r * 0.18))
        .stroke({ color: ACTIVE_RING, width: Math.max(2.5, r * 0.14) });
    }
    sprite.root.alpha = view.gmOnly ? 0.55 : 1;
    sprite.label.text = tokenInitial(view.token.name);
    sprite.label.style.fontSize = r;
    sprite.radius = r;
    sprite.title = view.token.name;
    if (sprite.name) this.nameOf(sprite);
  }
}
