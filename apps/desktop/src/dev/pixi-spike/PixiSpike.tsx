import type { Application } from 'pixi.js';
import { useState } from 'react';
import { commands } from '@/shared/ipc/bindings';
import { PixiHost } from './PixiHost';
import {
  gpuInfo,
  MAP_SIZE,
  MEASURE_MS,
  memoryInfo,
  SpikeScene,
  TILE_SIZE,
  type Variant,
} from './scene';

const VARIANTS: Variant[] = [
  { texture: 'single', grid: 'graphics' },
  { texture: 'single', grid: 'tiling' },
  { texture: 'tiles', grid: 'graphics' },
  { texture: 'tiles', grid: 'tiling' },
];
// The second pass runs in reverse so warm-up (caches, shader compile) does not favour one variant.
const PASSES = [VARIANTS, [...VARIANTS].reverse()];

const label = (v: Variant) => `${v.texture}+${v.grid}`;

function log(record: Record<string, unknown>): void {
  const line = `pixi-spike ${JSON.stringify(record)}`;
  console.info(line);
  // Outside Tauri (plain Vite in a browser) there is no IPC; the console is enough.
  commands.logFrontend('info', line).catch(() => undefined);
}

/** Dev-only page (`?spike=pixi`): measures map rendering variants for ADR-0017. */
export function PixiSpike() {
  const [lines, setLines] = useState<string[]>([]);
  const [running, setRunning] = useState(true);

  const onReady = (app: Application) => {
    let scene: SpikeScene | null = null;
    let cancelled = false;
    const say = (line: string) => {
      setLines((prev) => [...prev, line]);
    };

    const run = async () => {
      const gpu = gpuInfo(app);
      const env = {
        userAgent: navigator.userAgent,
        devicePixelRatio: window.devicePixelRatio,
        screen: { width: app.screen.width, height: app.screen.height },
        mapSize: MAP_SIZE,
        tileSize: TILE_SIZE,
        ...gpu,
      };
      log({ kind: 'env', ...env });
      say(`${gpu.renderer} · WebGL${gpu.webgl} · MAX_TEXTURE_SIZE ${gpu.maxTextureSize}`);

      for (const [pass, variants] of PASSES.entries()) {
        for (const variant of variants) {
          if (cancelled) return;
          scene?.destroy();
          scene = new SpikeScene(app, variant);
          say(`${label(variant)}: loading…`);
          try {
            const load = await scene.load();
            const frames = await scene.measure(MEASURE_MS);
            const memory = memoryInfo();
            log({ kind: 'result', pass: pass + 1, variant: label(variant), load, frames, memory });
            say(
              `#${pass + 1} ${label(variant)}: avg ${frames.avgFps} / min ${frames.minFps} fps, ` +
                `render ${frames.renderAvgMs} ms (p99 ${frames.renderP99Ms}); ` +
                `decode ${load.decodeMs} ms, upload ${load.uploadMs} ms, ` +
                `textures ${Math.round(load.textureBytes / 1048576)} MiB` +
                (memory ? `, JS heap ${memory.usedJsHeapMb} MiB` : ''),
            );
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            log({ kind: 'error', variant: label(variant), message });
            say(`${label(variant)}: ${message}`);
          }
        }
      }
      log({ kind: 'done' });
      setRunning(false);
    };
    void run();

    return () => {
      cancelled = true;
      scene?.destroy();
    };
  };

  return (
    <div style={{ position: 'fixed', inset: 0 }}>
      <PixiHost onReady={onReady} />
      <pre
        style={{
          position: 'absolute',
          left: 8,
          top: 8,
          margin: 0,
          padding: 8,
          maxWidth: '60%',
          whiteSpace: 'pre-wrap',
          font: '12px monospace',
          color: '#fff',
          background: 'rgba(0,0,0,0.6)',
          pointerEvents: 'none',
        }}
      >
        {`PixiJS spike — ${running ? 'measuring…' : 'done; wheel = zoom, drag = pan'}\n`}
        {lines.join('\n')}
      </pre>
    </div>
  );
}
