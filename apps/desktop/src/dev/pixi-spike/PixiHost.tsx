import { Application } from 'pixi.js';
import { useEffect, useRef } from 'react';

interface PixiHostProps {
  /** Called once the renderer is ready; the returned function runs before the app is destroyed. */
  onReady: (app: Application) => () => void;
}

/**
 * Owns one Pixi `Application` for the lifetime of the component. React renders only this `div`;
 * everything on the canvas is driven imperatively by `onReady`.
 *
 * `Application.init` is async, so an unmount (StrictMode does one right away in dev) can arrive
 * before it resolves: the app is then destroyed as soon as init finishes, and `onReady` never runs.
 */
export function PixiHost({ onReady }: PixiHostProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const onReadyRef = useRef(onReady);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const app = new Application();
    let disposed = false;
    let release: (() => void) | null = null;

    const ready = app
      .init({
        resizeTo: host,
        preference: 'webgl',
        antialias: false,
        autoDensity: true,
        resolution: window.devicePixelRatio,
        background: 0x202020,
      })
      .then(() => {
        if (disposed) return;
        host.appendChild(app.canvas);
        release = onReadyRef.current(app);
      });

    return () => {
      disposed = true;
      void ready.then(() => {
        release?.();
        app.destroy({ removeView: true }, { children: true, texture: true, textureSource: true });
      });
    };
  }, []);

  return <div ref={hostRef} style={{ position: 'absolute', inset: 0 }} />;
}
