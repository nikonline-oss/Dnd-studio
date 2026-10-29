// Zones on the map (ADR-0024), drawn for GM roles only: a translucent outline and the name; in
// edit mode the selected zone is highlighted with handles on its corners.
// Positions are Units16; the layer is scaled down to map units like the fog.
import { Container, Graphics, Text } from 'pixi.js';
import { handlesOf, labelPosition, type Point16 } from './zoneEdit';
import { zoneParts } from './zoneTool';
import type { ZoneDto } from '@/shared/ipc/client';

const ZONE_COLOR = 0x9b7bff;
const TRIGGER_COLOR = 0xff9f43;
const SELECTED_COLOR = 0xffd166;
const FILL_ALPHA = 0.12;
const SELECTED_FILL_ALPHA = 0.22;
const LINE_ALPHA = 0.85;
/** Handle size in screen pixels. */
export const HANDLE_PX = 10;
/** Name size in Units16: 14..128 map units. */
const MIN_FONT = 224;
const MAX_FONT = 2048;

/** The bounds of a label on the map, Units16. */
export interface LabelBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export class ZoneLayer {
  readonly view = new Container();
  private readonly shapes = new Graphics();
  private readonly handles = new Graphics();
  private readonly labels = new Container();
  private readonly labelBoxes = new Map<string, LabelBox>();
  private key = '';

  constructor() {
    this.view.eventMode = 'none';
    this.view.scale.set(1 / 16);
    this.view.addChild(this.shapes, this.labels, this.handles);
  }

  /**
   * Shows exactly `zones`, `selected` highlighted with handles sized for `zoom`; names are sized
   * for a map `mapWidth` Units16 wide. Redraws only when something changed.
   */
  show(
    zones: ZoneDto[],
    selected: string | null,
    zoom: number,
    mapWidth: number,
    triggered: string[] = [],
  ): void {
    const triggerSet = new Set(triggered);
    const key = `${mapWidth}:${selected ?? ''}@${selected ? zoom : 0}|${zones
      .map(
        (z) =>
          `${z.id}:${z.version}:${z.name}:${triggerSet.has(z.id)}:${JSON.stringify(z.shape)}:${JSON.stringify(z.label)}`,
      )
      .join(',')}`;
    if (key === this.key) return;
    this.key = key;
    this.shapes.clear();
    this.handles.clear();
    this.labelBoxes.clear();
    for (const label of this.labels.removeChildren()) label.destroy();
    // Readable (and grabbable) on a large map as on a small one.
    const fontSize = Math.min(MAX_FONT, Math.max(MIN_FONT, Math.round(mapWidth / 40)));
    for (const zone of zones) {
      const isSelected = zone.id === selected;
      const hasTrigger = triggerSet.has(zone.id);
      const color = isSelected ? SELECTED_COLOR : hasTrigger ? TRIGGER_COLOR : ZONE_COLOR;
      // A merged zone is drawn part by part.
      for (const part of zoneParts(zone.shape)) {
        if (part.type === 'rect') this.shapes.rect(part.x, part.y, part.width, part.height);
        else this.shapes.poly(part.points, true);
        this.shapes
          .fill({ color, alpha: isSelected ? SELECTED_FILL_ALPHA : FILL_ALPHA })
          .stroke({ color, alpha: LINE_ALPHA, width: isSelected ? 48 : 32 });
      }
      const at: Point16 = labelPosition(zone);
      const label = new Text({
        text: hasTrigger ? `⚡ ${zone.name}` : zone.name,
        style: {
          fill: isSelected || hasTrigger ? color : 0xffffff,
          fontSize,
          fontWeight: '600',
          stroke: { color: 0x000000, width: fontSize / 8 },
        },
      });
      label.position.set(at.x, at.y);
      label.alpha = 0.9;
      this.labels.addChild(label);
      this.labelBoxes.set(zone.id, { x: at.x, y: at.y, width: label.width, height: label.height });
      if (isSelected) {
        const size = (HANDLE_PX * 16) / zoom;
        for (const handle of handlesOf(zone.shape)) {
          this.handles.rect(handle.x - size / 2, handle.y - size / 2, size, size);
        }
        this.handles
          .fill({ color: 0xffffff })
          .stroke({ color: SELECTED_COLOR, width: (2 * 16) / zoom });
      }
    }
  }

  /** Where the zone's name is drawn now (`undefined` if it is not shown). */
  labelBox(zoneId: string): LabelBox | undefined {
    return this.labelBoxes.get(zoneId);
  }

  destroy(): void {
    this.view.destroy({ children: true });
  }
}
