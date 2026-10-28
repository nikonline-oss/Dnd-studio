//! Asset import pipeline (ADR-0017, CLAUDE.md §9): validate an uploaded image, decode it within
//! limits, downscale if needed, encode WebP derivatives (map tiles or one display image, plus a
//! thumbnail) and address every file by its SHA-256.
//!
//! [`import_image`] is pure (bytes in, bytes out); [`store`] writes the result into a campaign's
//! `assets/` folder. Dialogs, intents and serving live in the desktop app.

use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits, RgbaImage};
use sha2::{Digest, Sha256};

pub use dnd_protocol::AssetUsage;

/// Largest accepted upload.
pub const MAX_FILE_BYTES: usize = 100 * 1024 * 1024;
/// Largest side accepted for decoding (bigger maps are downscaled, see [`max_side`]).
pub const MAX_DECODE_SIDE: u32 = 32_768;
/// Upper bound for the decoder's allocations (decompression-bomb guard).
pub const MAX_DECODE_ALLOC: u64 = 1536 * 1024 * 1024;
/// Map tile size (ADR-0017).
pub const TILE: u32 = 2048;
/// Thumbnails fit in this square.
pub const THUMBNAIL: u32 = 256;
/// WebP quality for lossy encoding.
pub const WEBP_QUALITY: f32 = 85.0;

/// Largest side after import for a usage (ADR-0017).
#[must_use]
pub fn max_side(usage: AssetUsage) -> u32 {
    match usage {
        AssetUsage::Map => 16_384,
        AssetUsage::Token => 2048,
        AssetUsage::Audio => 0,
    }
}

/// Why an upload was refused. Codes are stable for the UI.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssetError {
    #[error("the file is empty")]
    Empty,
    #[error("the file is too large: {bytes} bytes (max {MAX_FILE_BYTES})")]
    TooLarge { bytes: usize },
    #[error("SVG images are not supported yet")]
    Svg,
    #[error("unsupported image format (PNG, JPEG and WebP are accepted)")]
    UnsupportedFormat,
    #[error("the image is too large to decode safely: {width}×{height}")]
    Dimensions { width: u32, height: u32 },
    #[error("the image could not be decoded: {0}")]
    Decode(String),
}

impl AssetError {
    /// Stable code, mirrored by `IpcError.code` and the UI locales.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "asset_empty",
            Self::TooLarge { .. } => "asset_too_large",
            Self::Svg => "asset_svg",
            Self::UnsupportedFormat => "asset_unsupported",
            Self::Dimensions { .. } => "asset_dimensions",
            Self::Decode(_) => "asset_decode",
        }
    }
}

/// A file addressed by its content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    pub sha256: String,
    pub bytes: Vec<u8>,
}

impl Encoded {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            sha256: sha256_hex(&bytes),
            bytes,
        }
    }
}

/// One map tile in image pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub webp: Encoded,
}

/// Everything produced from one upload.
#[derive(Clone, Debug)]
pub struct Imported {
    pub usage: AssetUsage,
    /// The upload as is (kept for export and re-processing).
    pub original: Encoded,
    pub mime: &'static str,
    pub extension: &'static str,
    /// Size of the upload in pixels.
    pub source_width: u32,
    pub source_height: u32,
    /// Size after downscaling.
    pub width: u32,
    pub height: u32,
    pub downscaled: bool,
    /// Whole image as WebP (tokens only).
    pub display: Option<Encoded>,
    pub thumbnail: Encoded,
    /// Row-major tiles (maps only).
    pub tiles: Vec<Tile>,
}

/// Lowercase hex SHA-256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// A well-formed SHA-256 hex string (no paths can hide in it).
#[must_use]
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let text = String::from_utf8_lossy(head);
    let text = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg"))
}

/// Validates and converts an uploaded image.
///
/// # Errors
/// See [`AssetError`]; nothing is decoded before the size, signature and dimensions are checked.
pub fn import_image(bytes: &[u8], usage: AssetUsage) -> Result<Imported, AssetError> {
    if bytes.is_empty() {
        return Err(AssetError::Empty);
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AssetError::TooLarge { bytes: bytes.len() });
    }
    if looks_like_svg(bytes) {
        return Err(AssetError::Svg);
    }
    if usage == AssetUsage::Audio {
        return Err(AssetError::UnsupportedFormat);
    }
    let format = image::guess_format(bytes).map_err(|_| AssetError::UnsupportedFormat)?;
    let (mime, extension) = match format {
        ImageFormat::Png => ("image/png", "png"),
        ImageFormat::Jpeg => ("image/jpeg", "jpg"),
        ImageFormat::WebP => ("image/webp", "webp"),
        _ => return Err(AssetError::UnsupportedFormat),
    };

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits.clone());
    // Header first: reject absurd dimensions before allocating anything.
    let (source_width, source_height) = reader
        .into_dimensions()
        .map_err(|e| AssetError::Decode(e.to_string()))?;
    if source_width == 0
        || source_height == 0
        || source_width > MAX_DECODE_SIDE
        || source_height > MAX_DECODE_SIDE
    {
        return Err(AssetError::Dimensions {
            width: source_width,
            height: source_height,
        });
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => AssetError::Dimensions {
            width: source_width,
            height: source_height,
        },
        other => AssetError::Decode(other.to_string()),
    })?;

    let (image, downscaled) = fit(decoded, max_side(usage));
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    let thumbnail = encode_webp(&fit_rgba(&rgba, THUMBNAIL));
    let (display, tiles) = match usage {
        AssetUsage::Token | AssetUsage::Audio => (Some(encode_webp(&rgba)), Vec::new()),
        AssetUsage::Map => (None, tiles(&rgba)),
    };

    Ok(Imported {
        usage,
        original: Encoded::new(bytes.to_vec()),
        mime,
        extension,
        source_width,
        source_height,
        width,
        height,
        downscaled,
        display,
        thumbnail,
        tiles,
    })
}

/// Downscales so that the larger side is at most `max` (aspect ratio kept).
fn fit(image: DynamicImage, max: u32) -> (DynamicImage, bool) {
    let (w, h) = image.dimensions();
    if w <= max && h <= max {
        return (image, false);
    }
    (image.resize(max, max, FilterType::Lanczos3), true)
}

fn fit_rgba(image: &RgbaImage, max: u32) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w <= max && h <= max {
        return image.clone();
    }
    DynamicImage::ImageRgba8(image.clone())
        .resize(max, max, FilterType::Triangle)
        .to_rgba8()
}

fn tiles(image: &RgbaImage) -> Vec<Tile> {
    let (w, h) = image.dimensions();
    let mut out = Vec::new();
    for y in (0..h).step_by(TILE as usize) {
        for x in (0..w).step_by(TILE as usize) {
            let tw = TILE.min(w - x);
            let th = TILE.min(h - y);
            let view = image::imageops::crop_imm(image, x, y, tw, th).to_image();
            out.push(Tile {
                x,
                y,
                width: tw,
                height: th,
                webp: encode_webp(&view),
            });
        }
    }
    out
}

fn encode_webp(image: &RgbaImage) -> Encoded {
    let (w, h) = image.dimensions();
    let memory = webp::Encoder::from_rgba(image.as_raw(), w, h).encode(WEBP_QUALITY);
    Encoded::new(memory.to_vec())
}

/// Where a derived (served) file lives: `assets/<sha256>.webp`.
#[must_use]
pub fn served_path(assets_dir: &Path, sha256: &str) -> PathBuf {
    assets_dir.join(format!("{sha256}.webp"))
}

/// Writes every file of an import into `assets_dir` (originals under `originals/`). Files are
/// content-addressed, so an existing file is left alone; new ones are written atomically.
pub fn store(assets_dir: &Path, imported: &Imported) -> io::Result<()> {
    std::fs::create_dir_all(assets_dir.join("originals"))?;
    write_once(
        &assets_dir.join("originals").join(format!(
            "{}.{}",
            imported.original.sha256, imported.extension
        )),
        &imported.original.bytes,
    )?;
    let derived = imported
        .display
        .iter()
        .chain(std::iter::once(&imported.thumbnail))
        .chain(imported.tiles.iter().map(|t| &t.webp));
    for file in derived {
        write_once(&served_path(assets_dir, &file.sha256), &file.bytes)?;
    }
    Ok(())
}

// ---------- Audio (ADR-0026) ----------

/// Largest audio file accepted.
pub const MAX_AUDIO_BYTES: usize = 50 * 1024 * 1024;

/// An imported sound: kept as it is, addressed by its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedAudio {
    pub file: Encoded,
    pub mime: &'static str,
}

/// The audio container, from the file's signature (never from its name).
fn audio_mime(bytes: &[u8]) -> Option<&'static str> {
    let at = |offset: usize, magic: &[u8]| bytes.get(offset..offset + magic.len()) == Some(magic);
    if at(0, b"ID3") || (bytes.len() > 1 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0) {
        Some("audio/mpeg")
    } else if at(0, b"OggS") {
        Some("audio/ogg")
    } else if at(0, b"RIFF") && at(8, b"WAVE") {
        Some("audio/wav")
    } else if at(0, b"fLaC") {
        Some("audio/flac")
    } else if at(4, b"ftyp") {
        Some("audio/mp4")
    } else if at(0, &[0x1a, 0x45, 0xdf, 0xa3]) {
        Some("audio/webm")
    } else {
        None
    }
}

/// Checks a sound file (MP3, Ogg, WAV, FLAC, M4A, WebM) by its signature.
///
/// # Errors
/// `Empty`, `TooLarge` (over [`MAX_AUDIO_BYTES`]) or `UnsupportedFormat`.
pub fn import_audio(bytes: &[u8]) -> Result<ImportedAudio, AssetError> {
    if bytes.is_empty() {
        return Err(AssetError::Empty);
    }
    if bytes.len() > MAX_AUDIO_BYTES {
        return Err(AssetError::TooLarge { bytes: bytes.len() });
    }
    let mime = audio_mime(bytes).ok_or(AssetError::UnsupportedFormat)?;
    Ok(ImportedAudio {
        file: Encoded::new(bytes.to_vec()),
        mime,
    })
}

/// Where a served sound lives: `assets/<sha256>.audio`.
#[must_use]
pub fn audio_path(assets_dir: &Path, sha256: &str) -> PathBuf {
    assets_dir.join(format!("{sha256}.audio"))
}

/// Writes a sound into `assets_dir` (once: files are content-addressed).
pub fn store_audio(assets_dir: &Path, audio: &ImportedAudio) -> io::Result<()> {
    std::fs::create_dir_all(assets_dir)?;
    write_once(
        &audio_path(assets_dir, &audio.file.sha256),
        &audio.file.bytes,
    )
}

fn write_once(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
