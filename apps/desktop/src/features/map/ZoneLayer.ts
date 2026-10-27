// Zones on the map (ADR-0024), drawn for GM roles only: a translucent outline and the name.
// Positions are Units16; the layer is scaled down to map units like the fog.
import { Container, Graphics, Text } from 'pixi.js';
import type { ZoneDto } from '@/shared/ipc/client';

const ZONE_COLOR = 0x9b7bff;
const FILL_ALPHA = 0.12;
const LINE_ALPHA = 0.85;

export class ZoneLayer {
  readonly view = new Container();
  private readonly shapes = new Graphics();
  private readonly labels = new Container();
  private key = '';

  constructor() {
    this.view.eventMode = 'none';
    this.view.scale.set(1 / 16);
    this.view.addChild(this.shapes, this.labels);
  }

  /** Shows exactly `zones`; redraws only when they change. */
  show(zones: ZoneDto[]): void {
    const key = zones.map((z) => `${z.id}:${z.version}`).join(',');
    if (key === this.key) return;
    this.key = key;
    this.shapes.clear();
    for (const label of this.labels.removeChildren()) label.destroy();
    for (const zone of zones) {
      const shape = zone.shape;
      let labelAt: { x: number; y: number };
      if (shape.type === 'rect') {
        this.shapes.rect(shape.x, shape.y, shape.width, shape.height);
        labelAt = { x: shape.x, y: shape.y };
      } else {
        this.shapes.poly(shape.points, true);
        labelAt = { x: shape.points[0] ?? 0, y: shape.points[1] ?? 0 };
      }
      this.shapes
        .fill({ color: ZONE_COLOR, alpha: FILL_ALPHA })
        .stroke({ color: ZONE_COLOR, alpha: LINE_ALPHA, width: 32 });
      const label = new Text({
        text: zone.name,
        style: { fill: 0xffffff, fontSize: 224, fontWeight: '600' },
      });
      label.position.set(labelAt.x + 64, labelAt.y + 48);
      label.alpha = 0.9;
      this.labels.addChild(label);
    }
  }

  destroy(): void {
    this.view.destroy({ children: true });
  }
}
