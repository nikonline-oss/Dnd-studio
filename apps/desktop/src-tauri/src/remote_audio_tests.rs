#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use dnd_protocol::{AssetDto, AssetId, AssetUsage, SoundDto, Version};

fn sound(n: usize, looped: bool, paused: bool) -> SoundDto {
    SoundDto {
        asset_id: AssetId::new_v7(),
        sha256: format!("{n:064x}"),
        mime: "audio/ogg".into(),
        volume: 80,
        looped,
        paused,
        position_ms: 1234,
    }
}

fn view() -> View {
    View::from_snapshot(&GameState::new("Player view").to_snapshot(Seq(0)))
}

fn play(view: &mut View, sound: SoundDto) {
    let batch = EventBatch {
        seq: Seq(view.seq.0 + 1),
        caused_by: None,
        events: vec![Event::SoundPlayed { sound }],
    };
    assert!(!apply(view, &batch));
}

#[test]
fn transient_audio_mime_survives_evolution_and_snapshot_replacement() {
    let mut view = view();
    let sound = sound(1, false, false);
    play(&mut view, sound.clone());
    assert!(view.state.assets.is_empty());
    assert!(view.state.sounds.is_empty());
    assert_eq!(view.asset_mime(&sound.sha256), "audio/ogg");
    view.replace_snapshot(&view.snapshot());
    assert_eq!(view.asset_mime(&sound.sha256), "audio/ogg");
    // Knowledge belongs to this connection, not to a global cache or another session.
    assert_eq!(self::view().asset_mime(&sound.sha256), "image/webp");
}

#[test]
fn snapshot_sounds_supply_mime_without_library_metadata() {
    for (looped, paused) in [(true, false), (false, true)] {
        let sound = sound(2, looped, paused);
        let mut snapshot = GameState::new("Player").to_snapshot(Seq(10));
        snapshot.sounds.push(sound.clone());
        let view = View::from_snapshot(&snapshot);
        assert!(view.state.assets.is_empty());
        assert_eq!(view.asset_mime(&sound.sha256), "audio/ogg");
    }
}

#[test]
fn mime_history_is_bounded_but_active_sounds_remain_resolvable() {
    let mut view = view();
    let music = sound(1, true, false);
    play(&mut view, music.clone());
    for n in 2..=MAX_SOUND_MIMES + 2 {
        play(&mut view, sound(n, false, false));
    }
    assert_eq!(view.sound_mimes.len(), MAX_SOUND_MIMES);
    assert_eq!(view.asset_mime(&format!("{:064x}", 2)), "image/webp");
    assert_eq!(
        view.asset_mime(&format!("{:064x}", MAX_SOUND_MIMES + 2)),
        "audio/ogg"
    );
    assert_eq!(view.asset_mime(&music.sha256), "audio/ogg");
}

#[test]
fn unaccepted_batches_do_not_add_mime_metadata() {
    let mut view = view();
    let sound = sound(1, false, false);
    let mut batch = EventBatch {
        seq: Seq(2),
        caused_by: None,
        events: vec![Event::SoundPlayed {
            sound: sound.clone(),
        }],
    };
    assert!(apply(&mut view, &batch));
    assert_eq!(view.asset_mime(&sound.sha256), "image/webp");
    view.seq = Seq(2);
    batch.seq = Seq(1);
    assert!(!apply(&mut view, &batch));
    assert_eq!(view.asset_mime(&sound.sha256), "image/webp");
}

#[test]
fn only_known_audio_mimes_and_valid_hashes_are_retained() {
    let mut view = view();
    for mime in [
        "text/html",
        "audio/unknown",
        "audio/ogg\r\nX-Evil: yes",
        "image/webp",
    ] {
        let mut sound = sound(1, true, false);
        sound.mime = mime.into();
        play(&mut view, sound.clone());
        assert_eq!(view.asset_mime(&sound.sha256), "image/webp");
    }
    let mut invalid_hash = sound(2, false, false);
    invalid_hash.sha256 = "../file".into();
    play(&mut view, invalid_hash);
    assert!(view.sound_mimes.is_empty());
    for mime in [
        "audio/mpeg",
        "audio/ogg",
        "audio/wav",
        "audio/flac",
        "audio/mp4",
        "audio/webm",
    ] {
        let mut sound = sound(3, false, false);
        sound.mime = mime.into();
        play(&mut view, sound.clone());
        assert_eq!(view.asset_mime(&sound.sha256), mime);
    }
    assert_eq!(view.sound_mimes.len(), 1);
}

#[test]
fn transient_mime_serves_audio_file_with_correct_content_type() {
    let mut view = view();
    let sound = sound(1, false, false);
    play(&mut view, sound.clone());
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    let bytes = b"OggS-test-fixture";
    std::fs::write(
        dir.path()
            .join("assets")
            .join(format!("{}.audio", sound.sha256)),
        bytes,
    )
    .unwrap();
    let response = crate::assets::response_for_mime(
        Some(dir.path()),
        &sound.sha256,
        view.asset_mime(&sound.sha256),
    );
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "audio/ogg");
    assert_eq!(response.body(), bytes);
}

#[test]
fn visible_library_audio_and_image_mimes_are_canonical() {
    let mut view = view();
    let mut asset = AssetDto {
        id: AssetId::new_v7(),
        usage: AssetUsage::Audio,
        sha256: format!("{:064x}", 42),
        mime: "audio/mp4".into(),
        size_bytes: 10,
        original_name: None,
        width: 0,
        height: 0,
        display_sha256: None,
        thumbnail_sha256: String::new(),
        tiles: vec![],
        folder_id: None,
        version: Version::INITIAL,
    };
    view.state.assets.insert(asset.id, asset.clone());
    assert_eq!(view.asset_mime(&asset.sha256), "audio/mp4");
    asset.mime = "text/html".into();
    view.state.assets.insert(asset.id, asset.clone());
    assert_eq!(view.asset_mime(&asset.sha256), "image/webp");
    asset.usage = AssetUsage::Map;
    asset.mime = "image/png".into();
    asset.display_sha256 = Some(format!("{:064x}", 43));
    view.state.assets.insert(asset.id, asset.clone());
    assert_eq!(
        view.asset_mime(asset.display_sha256.as_deref().unwrap()),
        "image/webp"
    );
}
