import { MapScene } from './MapScene';
import { describe, expect, it, vi } from 'vitest';
import { Container, type Application, type FederatedPointerEvent } from 'pixi.js';

// Run the real pointer handlers without a GPU renderer.
function setup() {
  const setCamera = vi.fn();
  const onMoveToken = vi.fn();
  const scene = Object.assign(
    new MapScene(
      {
        stage: new Container(),
        screen: { width: 800, height: 600 },
        canvas: document.createElement('canvas'),
        renderer: { on: vi.fn() },
      } as unknown as Application,
      vi.fn(),
    ) as object,
    {
      mapSize: { width: 4000, height: 3000 },
      camera: { x: -100, y: -100, zoom: 1 },
      combatMovement: true,
      movementTokenId: 'token',
      movementPress: false,
      tokenViews: [{ token: { id: 'token', x: 560, y: 560 } }],
      grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
      dragFrom: null,
      panStart: null,
      app: { canvas: { style: { cursor: '' } } },
      world: { toLocal: (p: unknown) => p },
      screenSize: () => ({ width: 800, height: 600 }),
      setCamera,
      drawMovementPath: vi.fn(),
      onMoveToken,
    },
  ) as unknown as {
    onDown: (e: FederatedPointerEvent) => void;
    onMove: (e: FederatedPointerEvent) => void;
    onUp: (e: FederatedPointerEvent) => void;
    movementTokenId: string;
    movementPress: boolean;
  };
  return { scene, setCamera, onMoveToken };
}
const pointer = (button: number, x: number, y: number) =>
  ({ button, global: { x, y } }) as FederatedPointerEvent;
describe('combat camera input', () => {
  for (const button of [0, 1])
    it(`pans with button ${button} without confirming or cancelling the route`, () => {
      const { scene, setCamera, onMoveToken } = setup();
      scene.onDown(pointer(button, 100, 100));
      scene.onMove(pointer(button, 140, 120));
      scene.onUp(pointer(button, 140, 120));
      expect(setCamera).toHaveBeenCalledWith({ x: -60, y: -80, zoom: 1 });
      expect(scene.movementTokenId).toBe('token');
      expect(scene.movementPress).toBe(false);
      expect(onMoveToken).not.toHaveBeenCalled();
    });
});

it('confirms a short left click as a movement after panning remains available', () => {
  const { scene, setCamera, onMoveToken } = setup();
  scene.onDown(pointer(0, 105, 105));
  scene.onUp(pointer(0, 105, 105));
  expect(setCamera).not.toHaveBeenCalled();
  expect(onMoveToken).toHaveBeenCalledWith({ id: 'token', x: 1680, y: 1680, final: true });
  expect(scene.movementTokenId).toBeNull();
});
