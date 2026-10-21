//! Dev-only generator of the synthetic map for the PixiJS render spike (ADR-0017).
//!
//! Writes one `SIZE`×`SIZE` lossy WebP plus the same image pre-cut into `TILE`² tiles
//! (`tile-{col}-{row}.webp`, edge tiles are smaller) — what the asset pipeline will produce.
//! The output folder is git-ignored; the images are never committed.
//!
//! ```text
//! cargo run --release -p dnd-spike-map-gen [-- <out_dir>]
//! ```

use std::{fs, path::PathBuf, time::Instant};

use anyhow::{Context, Result, anyhow};

const SIZE: u32 = 8000;
const TILE: u32 = 2048;
const QUALITY: f32 = 85.0;
const DEFAULT_OUT: &str = "apps/desktop/src/dev/pixi-spike/generated";

fn main() -> Result<()> {
    let out = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEFAULT_OUT), PathBuf::from);
    fs::create_dir_all(&out).with_context(|| format!("create {}", out.display()))?;

    let started = Instant::now();
    let rgb = render(SIZE);
    println!("rendered {SIZE}x{SIZE} in {:?}", started.elapsed());

    let full = encode(&rgb, SIZE, SIZE)?;
    let full_path = out.join(format!("map-{SIZE}.webp"));
    fs::write(&full_path, &full).with_context(|| format!("write {}", full_path.display()))?;
    println!("{} ({} KiB)", full_path.display(), full.len() / 1024);

    let tiles = SIZE.div_ceil(TILE);
    for row in 0..tiles {
        for col in 0..tiles {
            let (x0, y0) = (col * TILE, row * TILE);
            let (w, h) = (TILE.min(SIZE - x0), TILE.min(SIZE - y0));
            let mut tile = Vec::with_capacity((w * h * 3) as usize);
            for y in y0..y0 + h {
                let start = ((y * SIZE + x0) * 3) as usize;
                let row_px = rgb
                    .get(start..start + (w * 3) as usize)
                    .ok_or_else(|| anyhow!("tile {col},{row} out of bounds"))?;
                tile.extend_from_slice(row_px);
            }
            let bytes = encode(&tile, w, h)?;
            let path = out.join(format!("tile-{col}-{row}.webp"));
            fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
        }
    }
    println!(
        "{} tiles of {TILE}px, total {:?}",
        tiles * tiles,
        started.elapsed()
    );
    Ok(())
}

fn encode(rgb: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    webp::Encoder::from_rgb(rgb, w, h)
        .encode_simple(false, QUALITY)
        .map(|m| m.to_vec())
        .map_err(|e| anyhow!("webp encode {w}x{h}: {e:?}"))
}

/// Terrain-like picture: smooth value noise picks water/sand/grass/rock, a finer octave adds
/// texture, and a 1px "detail" noise keeps the lossy encoder from flattening it into nothing.
fn render(size: u32) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((size * size * 3) as usize);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (f64::from(x), f64::from(y));
            let height = 0.6 * value_noise(fx / 900.0, fy / 900.0, 1)
                + 0.3 * value_noise(fx / 180.0, fy / 180.0, 2)
                + 0.1 * value_noise(fx / 40.0, fy / 40.0, 3);
            let base: [f64; 3] = match height {
                h if h < 0.35 => [40.0, 80.0, 150.0],
                h if h < 0.42 => [200.0, 185.0, 130.0],
                h if h < 0.68 => [70.0, 130.0, 60.0],
                _ => [120.0, 115.0, 110.0],
            };
            let shade = 0.75 + 0.5 * height;
            let grain = (hash(x, y, 4) & 0x1f) as f64 - 16.0;
            for c in base {
                rgb.push((c * shade + grain).clamp(0.0, 255.0) as u8);
            }
        }
    }
    rgb
}

fn value_noise(x: f64, y: f64, seed: u32) -> f64 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (smooth(x - x0), smooth(y - y0));
    let (ix, iy) = (x0 as u32, y0 as u32);
    let corner = |dx: u32, dy: u32| f64::from(hash(ix + dx, iy + dy, seed) & 0xffff) / 65535.0;
    let top = lerp(corner(0, 0), corner(1, 0), tx);
    let bottom = lerp(corner(0, 1), corner(1, 1), tx);
    lerp(top, bottom, ty)
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x
        .wrapping_mul(0x8da6_b343)
        .wrapping_add(y.wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}
