import { render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PixiHost } from './PixiHost';

const { init, resize, destroy } = vi.hoisted(() => ({
  init: vi.fn(),
  resize: vi.fn(),
  destroy: vi.fn(),
}));

vi.mock('pixi.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('pixi.js')>();
  return {
    ...actual,
    Application: class {
      canvas: HTMLCanvasElement;
      renderer: { resize: typeof resize };
      constructor() {
        this.canvas = document.createElement('canvas');
        this.renderer = { resize };
      }
      init = init;
      destroy = destroy;
    },
  };
});

class ObservedResizeObserver implements ResizeObserver {
  static instances: ObservedResizeObserver[] = [];
  callback: ResizeObserverCallback;
  target: Element | null = null;
  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    ObservedResizeObserver.instances.push(this);
  }
  observe(target: Element) {
    this.target = target;
  }
  unobserve() {}
  disconnect() {}
  notify() {
    this.callback([], this);
  }
}

afterEach(() => {
  init.mockReset().mockResolvedValue(undefined);
  resize.mockReset();
  destroy.mockReset();
  ObservedResizeObserver.instances = [];
  globalThis.ResizeObserver = ObservedResizeObserver;
});

describe('PixiHost resizing', () => {
  it('resizes the renderer whenever the host container changes dimensions', async () => {
    globalThis.ResizeObserver = ObservedResizeObserver;
    init.mockResolvedValue(undefined);
    const { container } = render(<PixiHost onReady={() => () => {}} />);
    await vi.waitFor(() => expect(container.querySelector('canvas')).toBeInTheDocument());

    expect(init).toHaveBeenCalledOnce();
    const observer = ObservedResizeObserver.instances.at(-1);
    expect(observer?.target).toBe(container.firstChild);
    observer?.notify();
    expect(resize).toHaveBeenCalled();
  });
});
