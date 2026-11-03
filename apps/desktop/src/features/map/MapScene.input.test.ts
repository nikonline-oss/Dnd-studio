import { MapScene } from './MapScene';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Container, type Application, type FederatedPointerEvent } from 'pixi.js';

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});
// Exercise the actual pointer lifecycle without creating a GPU renderer.
function setup(combat = true) {
  const setCamera = vi.fn();
  const onMoveToken = vi.fn();
  const onSelectToken = vi.fn();
  const drawMovementPath = vi.fn();
  const tokens = { show: vi.fn(), release: vi.fn(), dragTo: vi.fn() };
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
      combatMovement: combat,
      tokenViews: [{ token: { id: 'token', x: 560, y: 560 } }],
      grid: { kind: 'square', cell_size: 70, offset_x: 0, offset_y: 0 },
      app: { canvas: { style: { cursor: '' } } },
      world: { toLocal: (p: unknown) => p },
      screenSize: () => ({ width: 800, height: 600 }),
      setCamera,
      drawMovementPath,
      onMoveToken,
      onSelectToken,
      tokens,
    },
  ) as unknown as {
    onGrabToken: (id: string, e: FederatedPointerEvent) => void;
    onDown: (e: FederatedPointerEvent) => void;
    onMove: (e: FederatedPointerEvent) => void;
    onUp: (e: FederatedPointerEvent) => void;
    onCancel: () => void;
    cancelMovement: () => void;
    movementTokenId: string | null;
  };
  cleanups.push(() => {
    window.removeEventListener('blur', scene.onCancel);
  });
  return { scene, setCamera, onMoveToken, onSelectToken, drawMovementPath, tokens };
}
const pointer = (button: number, x: number, y: number) =>
  ({ button, global: { x, y } }) as FederatedPointerEvent;
describe('token movement gestures', () => {
  for (const combat of [false, true]) {
    it(`a click only selects, and hovering afterwards never moves (combat=${combat})`, () => {
      const { scene, onMoveToken, onSelectToken, drawMovementPath } = setup(combat);
      scene.onGrabToken('token', pointer(0, 35, 35));
      scene.onUp(pointer(0, 35, 35));
      scene.onMove(pointer(0, 105, 105));
      scene.onDown(pointer(0, 105, 105));
      scene.onUp(pointer(0, 105, 105));
      expect(onSelectToken).toHaveBeenCalledWith('token');
      expect(onMoveToken).not.toHaveBeenCalled();
      expect(drawMovementPath).not.toHaveBeenCalled();
    });
    it(`commits a held drag once, only on release (combat=${combat})`, () => {
      const { scene, onMoveToken, drawMovementPath } = setup(combat);
      scene.onGrabToken('token', pointer(0, 35, 35));
      scene.onMove(pointer(0, 105, 105));
      expect(onMoveToken).not.toHaveBeenCalled();
      if (combat) expect(drawMovementPath).toHaveBeenCalledWith({ x: 105, y: 105 });
      scene.onUp(pointer(1, 105, 105));
      expect(onMoveToken).not.toHaveBeenCalled();
      scene.onUp(pointer(0, 105, 105));
      expect(onMoveToken).toHaveBeenCalledExactlyOnceWith({
        id: 'token',
        x: 1680,
        y: 1680,
        final: true,
      });
      scene.onUp(pointer(0, 175, 175));
      expect(onMoveToken).toHaveBeenCalledTimes(1);
    });
    for (const cancel of ['escape', 'outside', 'blur']) {
      it(`cancels a held drag on ${cancel} without committing on release (combat=${combat})`, () => {
        const { scene, onMoveToken, tokens } = setup(combat);
        scene.onGrabToken('token', pointer(0, 35, 35));
        scene.onMove(pointer(0, 105, 105));
        if (cancel === 'escape') scene.cancelMovement();
        else if (cancel === 'outside') scene.onCancel();
        else window.dispatchEvent(new Event('blur'));
        scene.onUp(pointer(0, 105, 105));
        expect(onMoveToken).not.toHaveBeenCalled();
        expect(tokens.release).toHaveBeenCalled();
        expect(tokens.show).toHaveBeenCalled();
      });
    }
  }
  for (const button of [0, 1])
    it(`pans empty map with button ${button}`, () => {
      const { scene, setCamera, onMoveToken } = setup();
      scene.onDown(pointer(button, 100, 100));
      scene.onMove(pointer(button, 140, 120));
      scene.onUp(pointer(button, 140, 120));
      expect(setCamera).toHaveBeenCalledWith({ x: -60, y: -80, zoom: 1 });
      expect(onMoveToken).not.toHaveBeenCalled();
    });
});
