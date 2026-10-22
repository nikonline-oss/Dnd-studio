// Camera math of the map view. Pure: no Pixi, no DOM, so it is unit-tested directly.
//
// Three coordinate systems (ADR-0016):
// - screen: CSS pixels of the canvas;
// - map: logical map units (`MapDto.width/height`, grid sizes);
// - Units16: integers in 1/16 of a logical unit (token positions, fog).

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

/** Screen position of the map origin and screen pixels per logical unit. */
export interface Camera {
  x: number;
  y: number;
  zoom: number;
}

/** Closest zoom: 4 screen pixels per logical unit (unless the whole map is smaller). */
export const MAX_ZOOM = 4;
/** Share of the viewport the map takes when fitted. */
export const FIT_MARGIN = 0.95;
/** Screen pixels of the map that stay visible however far it is panned. */
export const KEEP_VISIBLE = 64;

export function screenToMap(camera: Camera, p: Point): Point {
  return { x: (p.x - camera.x) / camera.zoom, y: (p.y - camera.y) / camera.zoom };
}

export function mapToScreen(camera: Camera, p: Point): Point {
  return { x: camera.x + p.x * camera.zoom, y: camera.y + p.y * camera.zoom };
}

export function toUnits16(value: number): number {
  return Math.round(value * 16);
}

export function fromUnits16(value: number): number {
  return value / 16;
}

/** Zoom at which the whole map fits the screen. */
export function fitZoom(screen: Size, map: Size): number {
  return (Math.min(screen.width / map.width, screen.height / map.height) || 1) * FIT_MARGIN;
}

export function zoomLimits(screen: Size, map: Size): { min: number; max: number } {
  const fit = fitZoom(screen, map);
  return { min: Math.min(fit / 2, MAX_ZOOM), max: Math.max(MAX_ZOOM, fit) };
}

/** The whole map, centred. */
export function fitCamera(screen: Size, map: Size): Camera {
  const zoom = fitZoom(screen, map);
  return {
    x: (screen.width - map.width * zoom) / 2,
    y: (screen.height - map.height * zoom) / 2,
    zoom,
  };
}

/** Keeps at least {@link KEEP_VISIBLE} pixels of the map on screen on each axis. */
export function clampCamera(camera: Camera, screen: Size, map: Size): Camera {
  const clampAxis = (pos: number, mapPx: number, screenPx: number) => {
    const keep = Math.min(KEEP_VISIBLE, mapPx, screenPx);
    return Math.min(screenPx - keep, Math.max(keep - mapPx, pos));
  };
  return {
    x: clampAxis(camera.x, map.width * camera.zoom, screen.width),
    y: clampAxis(camera.y, map.height * camera.zoom, screen.height),
    zoom: camera.zoom,
  };
}

/** Multiplies the zoom by `factor`, keeping the map point under `at` (screen) in place. */
export function zoomAt(camera: Camera, at: Point, factor: number, screen: Size, map: Size): Camera {
  const { min, max } = zoomLimits(screen, map);
  const zoom = Math.min(max, Math.max(min, camera.zoom * factor));
  const anchor = screenToMap(camera, at);
  return clampCamera({ x: at.x - anchor.x * zoom, y: at.y - anchor.y * zoom, zoom }, screen, map);
}

export function panBy(camera: Camera, dx: number, dy: number, screen: Size, map: Size): Camera {
  return clampCamera({ ...camera, x: camera.x + dx, y: camera.y + dy }, screen, map);
}

const LINE_HEIGHT_PX = 16;
const PAGE_HEIGHT_PX = 800;

/** Zoom factor for a wheel event: smooth for touchpads, ~20 % per mouse notch. */
export function wheelFactor(deltaY: number, deltaMode: number): number {
  const px =
    deltaMode === 1 ? deltaY * LINE_HEIGHT_PX : deltaMode === 2 ? deltaY * PAGE_HEIGHT_PX : deltaY;
  return Math.exp(-px / 500);
}
