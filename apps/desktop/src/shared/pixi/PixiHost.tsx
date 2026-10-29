import { Application, CullerPlugin, extensions } from 'pixi.js';
import { useEffect, useRef } from 'react';
// The app's CSP has no 'unsafe-eval', and without this Pixi refuses to start its renderer in the
// built app (the dev server does not apply the CSP). It swaps generated code for prebuilt functions.
import 'pixi.js/unsafe-eval';

// Containers marked `cullable` (tokens, map tiles) are not drawn when off screen.
extensions.add(CullerPlugin);

interface PixiHostProps {
  /** Called once the renderer is ready; the returned function runs before the app is destroyed. */
  onReady: (app: Application) => () => void;
  /** Renderer could not start (e.g. no WebGL). */
  onError?: (error: unknown) => void;
  /** Canvas colour; transparent (the page background shows through) when omitted. */
  background?: number;
}

/**
 * Owns one Pixi `Application` for the lifetime of the component (ADR-0017). React renders only
 * this `div`; everything on the canvas is driven imperatively by `onReady`.
 *
 * `Application.init` is async, so an unmount (StrictMode does one right away in dev) can arrive
 * before it resolves: the app is then destroyed as soon as init finishes, and `onReady` never runs.
 */
export function PixiHost({ onReady, onError, background }: PixiHostProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const onReadyRef = useRef(onReady);
  const onErrorRef = useRef(onError);
  const backgroundRef = useRef(background);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const app = new Application();
    let disposed = false;
    let started = false;
    let release: (() => void) | null = null;
    let resizeObserver: ResizeObserver | null = null;

    const ready = app
      .init({
        preference: 'webgl',
        // Software and blocklisted GPUs (old drivers, VMs) still get a renderer.
        failIfMajorPerformanceCaveat: false,
        antialias: false,
        autoDensity: true,
        resolution: window.devicePixelRatio,
        ...(backgroundRef.current === undefined
          ? { backgroundAlpha: 0 }
          : { background: backgroundRef.current }),
      })
      .then(
        () => {
          started = true;
          if (disposed) return;
          host.appendChild(app.canvas);
          const resizeRenderer = () => {
            const { width, height } = host.getBoundingClientRect();
            app.renderer.resize(width, height);
          };
          resizeObserver = new ResizeObserver(resizeRenderer);
          resizeObserver.observe(host);
          resizeRenderer();
          release = onReadyRef.current(app);
        },
        (error: unknown) => {
          if (!disposed) onErrorRef.current?.(error);
        },
      );

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      void ready.then(() => {
        if (!started) return;
        release?.();
        app.destroy({ removeView: true }, { children: true, texture: true, textureSource: true });
      });
    };
  }, []);

  return <div ref={hostRef} style={{ position: 'absolute', inset: 0 }} />;
}
