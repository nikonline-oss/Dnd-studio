// A procedural dungeon picture for the mock IPC: parchment, stone rooms, corridors, water.
// Deterministic (fixed seed), so screenshots compare across runs.

export interface SampleMap {
  width: number;
  height: number;
  /** Data URL of the only tile (WebP, or PNG where WebP encoding is unavailable). */
  tile: string;
  thumbnail: string;
}

function rng(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1_664_525 + 1_013_904_223) % 2 ** 32;
    return s / 2 ** 32;
  };
}

export async function drawSampleMap(): Promise<SampleMap> {
  const width = 2048;
  const height = 1536;
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no 2d context');
  const random = rng(7);

  // Parchment with blotches.
  ctx.fillStyle = '#d8c9a3';
  ctx.fillRect(0, 0, width, height);
  for (let i = 0; i < 400; i++) {
    ctx.fillStyle = `rgba(120, 90, 40, ${0.02 + random() * 0.04})`;
    ctx.beginPath();
    ctx.arc(random() * width, random() * height, 20 + random() * 120, 0, Math.PI * 2);
    ctx.fill();
  }

  const rooms: [number, number, number, number][] = [
    [192, 192, 448, 320],
    [832, 128, 576, 384],
    [1600, 256, 320, 448],
    [256, 832, 384, 512],
    [896, 768, 640, 448],
    [1664, 960, 256, 384],
  ];
  const corridors: [number, number, number, number][] = [
    [640, 320, 192, 64],
    [1408, 384, 192, 64],
    [384, 512, 64, 320],
    [640, 1024, 256, 64],
    [1088, 512, 64, 256],
    [1536, 1088, 128, 64],
    [1728, 704, 64, 256],
  ];
  const stone = (x: number, y: number, w: number, h: number) => {
    ctx.fillStyle = '#9c968a';
    ctx.fillRect(x, y, w, h);
    for (let i = 0; i < (w * h) / 900; i++) {
      ctx.fillStyle = `rgba(60, 55, 50, ${0.05 + random() * 0.1})`;
      ctx.fillRect(x + random() * w, y + random() * h, 6 + random() * 18, 4 + random() * 10);
    }
  };
  for (const r of [...corridors, ...rooms]) stone(...r);

  // Water in the big hall.
  const water = ctx.createRadialGradient(1216, 992, 20, 1216, 992, 260);
  water.addColorStop(0, 'rgba(40, 90, 120, 0.85)');
  water.addColorStop(1, 'rgba(40, 90, 120, 0)');
  ctx.fillStyle = water;
  ctx.fillRect(896, 768, 640, 448);

  ctx.strokeStyle = '#3b2f22';
  ctx.lineWidth = 10;
  for (const [x, y, w, h] of rooms) ctx.strokeRect(x, y, w, h);

  // Pillars and a sarcophagus.
  ctx.fillStyle = '#5a5248';
  for (const [x, y] of [
    [960, 224],
    [1280, 224],
    [960, 416],
    [1280, 416],
  ] as const) {
    ctx.beginPath();
    ctx.arc(x, y, 22, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.fillStyle = '#6d5f4c';
  ctx.fillRect(1712, 400, 96, 160);

  const encode = (source: HTMLCanvasElement) => {
    const webp = source.toDataURL('image/webp', 0.85);
    return webp.startsWith('data:image/webp') ? webp : source.toDataURL('image/png');
  };
  const thumb = document.createElement('canvas');
  thumb.width = 256;
  thumb.height = Math.round((256 * height) / width);
  thumb.getContext('2d')?.drawImage(canvas, 0, 0, thumb.width, thumb.height);
  await Promise.resolve();
  return { width, height, tile: encode(canvas), thumbnail: encode(thumb) };
}
