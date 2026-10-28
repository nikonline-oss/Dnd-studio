//! Import pipeline: validation, limits, tiling, downscaling, content addressing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::io::Cursor;

use dnd_assets::*;
use image::{ImageFormat, Rgba, RgbaImage};

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn map_is_cut_into_2048_tiles_row_by_row() {
    let imported = import_image(&png(5000, 3000), AssetUsage::Map).unwrap();
    assert_eq!((imported.width, imported.height), (5000, 3000));
    assert!(!imported.downscaled);
    assert!(imported.display.is_none());
    let tiles: Vec<_> = imported
        .tiles
        .iter()
        .map(|t| (t.x, t.y, t.width, t.height))
        .collect();
    assert_eq!(
        tiles,
        [
            (0, 0, 2048, 2048),
            (2048, 0, 2048, 2048),
            (4096, 0, 904, 2048),
            (0, 2048, 2048, 952),
            (2048, 2048, 2048, 952),
            (4096, 2048, 904, 952),
        ]
    );
    // Every tile is a decodable WebP of its declared size, addressed by its own hash.
    for t in &imported.tiles {
        let img = image::load_from_memory_with_format(&t.webp.bytes, ImageFormat::WebP).unwrap();
        assert_eq!((img.width(), img.height()), (t.width, t.height));
        assert_eq!(t.webp.sha256, sha256_hex(&t.webp.bytes));
    }
    assert_eq!(imported.mime, "image/png");
}

#[test]
fn tokens_get_one_webp_and_are_downscaled_to_2048() {
    let imported = import_image(&png(3000, 1500), AssetUsage::Token).unwrap();
    assert!(imported.downscaled);
    assert_eq!((imported.width, imported.height), (2048, 1024));
    assert_eq!(
        (imported.source_width, imported.source_height),
        (3000, 1500)
    );
    assert!(imported.tiles.is_empty());
    let display = imported.display.unwrap();
    let img = image::load_from_memory_with_format(&display.bytes, ImageFormat::WebP).unwrap();
    assert_eq!((img.width(), img.height()), (2048, 1024));
}

#[test]
fn maps_larger_than_16384_are_downscaled() {
    // Thin, so the test stays light: 20000×10.
    let imported = import_image(&png(20_000, 10), AssetUsage::Map).unwrap();
    assert!(imported.downscaled);
    assert_eq!(imported.width, 16_384);
    assert_eq!(imported.tiles.len(), 8);
}

#[test]
fn thumbnail_fits_256() {
    let imported = import_image(&png(1000, 400), AssetUsage::Token).unwrap();
    let thumb = image::load_from_memory(&imported.thumbnail.bytes).unwrap();
    assert!(thumb.width() <= THUMBNAIL && thumb.height() <= THUMBNAIL);
    assert_eq!(thumb.width(), 256);
}

#[test]
fn rejects_bad_uploads_with_stable_codes() {
    let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
    let cases: [(&[u8], &str); 4] = [
        (b"", "asset_empty"),
        (svg, "asset_svg"),
        (b"GIF89a....", "asset_unsupported"),
        (b"definitely not an image", "asset_unsupported"),
    ];
    for (bytes, code) in cases {
        let err = import_image(bytes, AssetUsage::Map).unwrap_err();
        assert_eq!(err.code(), code, "{err}");
    }
    let mut truncated = png(64, 64);
    truncated.truncate(80);
    assert_eq!(
        import_image(&truncated, AssetUsage::Map)
            .unwrap_err()
            .code(),
        "asset_decode"
    );
    let too_big = vec![0u8; MAX_FILE_BYTES + 1];
    assert_eq!(
        import_image(&too_big, AssetUsage::Map).unwrap_err().code(),
        "asset_too_large"
    );
}

/// A tiny PNG whose header claims 100 000 × 100 000 pixels is refused before decoding.
#[test]
fn decompression_bomb_is_refused_from_the_header() {
    let mut bytes = png(1, 1);
    // IHDR width/height live at bytes 16..24; the CRC no longer matches, which is fine for the
    // header-only dimension check.
    bytes[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    let err = import_image(&bytes, AssetUsage::Map).unwrap_err();
    assert!(
        matches!(err.code(), "asset_dimensions" | "asset_decode"),
        "{err}"
    );
}

#[test]
fn identical_uploads_have_identical_hashes_and_store_is_idempotent() {
    let bytes = png(300, 200);
    let a = import_image(&bytes, AssetUsage::Token).unwrap();
    let b = import_image(&bytes, AssetUsage::Token).unwrap();
    assert_eq!(a.original.sha256, b.original.sha256);
    assert_eq!(a.original.sha256, sha256_hex(&bytes));

    let dir = tempfile::tempdir().unwrap();
    store(dir.path(), &a).unwrap();
    store(dir.path(), &b).unwrap();
    let display = a.display.as_ref().unwrap();
    assert!(served_path(dir.path(), &display.sha256).exists());
    assert!(served_path(dir.path(), &a.thumbnail.sha256).exists());
    assert!(
        dir.path()
            .join("originals")
            .join(format!("{}.png", a.original.sha256))
            .exists()
    );
    assert!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .all(|e| { !e.unwrap().file_name().to_string_lossy().ends_with(".tmp") })
    );
}

#[test]
fn sha256_hex_shape() {
    let h = sha256_hex(b"abc");
    assert_eq!(
        h,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(is_sha256_hex(&h));
    assert!(!is_sha256_hex("../../etc/passwd"));
    assert!(!is_sha256_hex(&h.to_uppercase()));
}

#[test]
fn sounds_are_recognised_by_their_signature_not_their_name() {
    let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    wav.extend([0u8; 16]);
    for (bytes, mime) in [
        (b"ID3\x04\0\0\0\0\0\0rest".to_vec(), "audio/mpeg"),
        (vec![0xff, 0xfb, 0x90, 0x64, 0], "audio/mpeg"),
        (b"OggS\0\x02rest".to_vec(), "audio/ogg"),
        (wav, "audio/wav"),
        (b"fLaC\0\0\0\x22".to_vec(), "audio/flac"),
        (b"\0\0\0\x20ftypM4A ".to_vec(), "audio/mp4"),
        (vec![0x1a, 0x45, 0xdf, 0xa3, 1], "audio/webm"),
    ] {
        let audio = dnd_assets::import_audio(&bytes).unwrap();
        assert_eq!(audio.mime, mime);
        assert_eq!(audio.file.sha256, dnd_assets::sha256_hex(&bytes));
    }
    assert_eq!(
        dnd_assets::import_audio(b"\x89PNG\r\n\x1a\n"),
        Err(dnd_assets::AssetError::UnsupportedFormat)
    );
    assert_eq!(
        dnd_assets::import_audio(b""),
        Err(dnd_assets::AssetError::Empty)
    );

    let dir = tempfile::tempdir().unwrap();
    let audio = dnd_assets::import_audio(b"OggS sound").unwrap();
    dnd_assets::store_audio(dir.path(), &audio).unwrap();
    assert_eq!(
        std::fs::read(dnd_assets::audio_path(dir.path(), &audio.file.sha256)).unwrap(),
        b"OggS sound"
    );
}
