import { create } from 'zustand';
import type { ZoneShape } from '@/shared/ipc/client';

/** What the next rectangle drawn with the zone tool does (ADR-0024). */
export type ZoneDrawing =
  | { mode: 'new' }
  /** Replaces the zone's shape. */
  | { mode: 'replace'; zoneId: string }
  /** Adds a part to the zone. */
  | { mode: 'add'; zoneId: string };

interface ZoneToolState {
  drawing: ZoneDrawing;
  /** Zones ticked in the list (to merge); the first is kept. */
  ticked: string[];
  draw: (drawing: ZoneDrawing) => void;
  toggle: (zoneId: string) => void;
  clearTicks: () => void;
}

/** Shared by the zone list (inspector) and the map view; both are the map feature. */
export const useZoneTool = create<ZoneToolState>()((set) => ({
  drawing: { mode: 'new' },
  ticked: [],
  draw: (drawing) => {
    set({ drawing });
  },
  toggle: (zoneId) => {
    set((s) => ({
      ticked: s.ticked.includes(zoneId)
        ? s.ticked.filter((id) => id !== zoneId)
        : [...s.ticked, zoneId],
    }));
  },
  clearTicks: () => {
    set({ ticked: [] });
  },
}));

/** The rectangles and polygons a shape is made of (mirrors `ZoneShape::parts`). */
export function zoneParts(shape: ZoneShape): Exclude<ZoneShape, { type: 'union' }>[] {
  return shape.type === 'union' ? shape.parts.flatMap(zoneParts) : [shape];
}
