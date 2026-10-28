//! Libraries with folders and sounds (ADR-0026): rights of every role, folders and their assets,
//! sounds to everyone, and triggers that play them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{GameState, players_state};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;

fn folder(n: u128) -> LibraryFolderId {
    LibraryFolderId(v7(0x9100 + n))
}

fn asset_id(n: u128) -> AssetId {
    AssetId(v7(0x9200 + n))
}

fn hex(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn sound_asset(n: u128) -> AssetDto {
    let sha = hex(0x40 + u8::try_from(n).unwrap());
    AssetDto {
        id: asset_id(n),
        usage: AssetUsage::Audio,
        sha256: sha.clone(),
        mime: "audio/ogg".into(),
        size_bytes: 1000,
        original_name: Some(format!("track {n}.ogg")),
        width: 0,
        height: 0,
        display_sha256: Some(sha.clone()),
        thumbnail_sha256: sha,
        tiles: Vec::new(),
        folder_id: None,
        version: Version(1),
    }
}

fn picture_asset(n: u128) -> AssetDto {
    AssetDto {
        id: asset_id(n),
        usage: AssetUsage::Token,
        sha256: hex(0x60 + u8::try_from(n).unwrap()),
        mime: "image/png".into(),
        size_bytes: 1000,
        original_name: Some("hall.png".into()),
        width: 64,
        height: 64,
        display_sha256: Some(hex(0x70 + u8::try_from(n).unwrap())),
        thumbnail_sha256: hex(0x80 + u8::try_from(n).unwrap()),
        tiles: Vec::new(),
        folder_id: None,
        version: Version(1),
    }
}

fn create_folder(n: u128, library: Library, parent: Option<u128>, name: &str) -> Intent {
    Intent::CreateLibraryFolder {
        id: folder(n),
        library,
        parent: parent.map(folder),
        name: name.into(),
    }
}

/// Sounds 0 and 1, picture 2; audio folders 0 "Music" > 1 "Battle"; image folder 2 "Maps".
fn library_state() -> GameState {
    let mut state = table_state();
    for asset in [sound_asset(0), sound_asset(1), picture_asset(2)] {
        run(
            &mut state,
            Role::Gm,
            Intent::RegisterAsset {
                asset: Box::new(asset),
            },
            None,
        )
        .unwrap();
    }
    for intent in [
        create_folder(0, Library::Audio, None, "Music"),
        create_folder(1, Library::Audio, Some(0), "Battle"),
        create_folder(2, Library::Images, None, "Maps"),
    ] {
        run(&mut state, Role::Gm, intent, None).unwrap();
    }
    state
}

fn play(n: u128, looped: bool) -> Intent {
    Intent::PlaySound {
        asset_id: asset_id(n),
        volume: 80,
        looped,
    }
}

#[test]
fn only_gm_roles_manage_libraries_and_play_sounds() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        for (intent, version) in [
            (create_folder(9, Library::Audio, None, "New"), None),
            (
                Intent::RenameLibraryFolder {
                    id: folder(0),
                    name: "Songs".into(),
                },
                Some(Version(1)),
            ),
            (
                Intent::MoveAsset {
                    id: asset_id(0),
                    folder_id: Some(folder(0)),
                },
                Some(Version(1)),
            ),
            (
                Intent::RenameAsset {
                    id: asset_id(0),
                    name: "Theme".into(),
                },
                Some(Version(1)),
            ),
            (
                Intent::DeleteLibraryFolder { id: folder(1) },
                Some(Version(1)),
            ),
            (play(0, true), None),
            (Intent::StopSound { asset_id: None }, None),
        ] {
            let mut state = library_state();
            let kind = intent.kind();
            let result = run(&mut state, role, intent, version);
            assert_eq!(result.is_ok(), allowed, "{role:?} {kind}: {result:?}");
            if !allowed {
                assert_eq!(result, Err(RejectReason::Forbidden));
            }
        }
    }
}

#[test]
fn folders_keep_to_their_library_and_their_tree() {
    let mut state = library_state();
    // Sibling names differ in more than case; the same name in another library is fine.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            create_folder(9, Library::Audio, None, "music"),
            None
        ),
        Err(RejectReason::AlreadyExists)
    );
    run(
        &mut state,
        Role::Gm,
        create_folder(9, Library::Images, None, "Music"),
        None,
    )
    .unwrap();
    // A folder of another library is no parent.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            create_folder(10, Library::Images, Some(0), "Inside"),
            None
        ),
        Err(RejectReason::invalid("parent"))
    );
    // Never into itself or below itself.
    for parent in [0, 1] {
        assert_eq!(
            run(
                &mut state,
                Role::Gm,
                Intent::MoveLibraryFolder {
                    id: folder(0),
                    parent: Some(folder(parent)),
                },
                Some(Version(1))
            ),
            Err(RejectReason::invalid("parent"))
        );
    }
    // A sound goes only into an audio folder, a picture only into an image folder.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::MoveAsset {
                id: asset_id(0),
                folder_id: Some(folder(2)),
            },
            Some(Version(1))
        ),
        Err(RejectReason::invalid("folder_id"))
    );
    run(
        &mut state,
        Role::Gm,
        Intent::MoveAsset {
            id: asset_id(2),
            folder_id: Some(folder(2)),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.assets[&asset_id(2)].folder_id, Some(folder(2)));
}

#[test]
fn deleting_a_folder_returns_its_assets_to_the_root() {
    let mut state = library_state();
    for (n, f) in [(0, 1), (1, 0)] {
        run(
            &mut state,
            Role::Gm,
            Intent::MoveAsset {
                id: asset_id(n),
                folder_id: Some(folder(f)),
            },
            Some(Version(1)),
        )
        .unwrap();
    }
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteLibraryFolder { id: folder(0) },
        Some(Version(1)),
    )
    .unwrap();
    // Assets first, then the folders deepest first.
    let kinds: Vec<_> = events
        .iter()
        .map(|e| match e {
            Event::AssetUpdated { .. } => "asset_updated",
            Event::LibraryFolderDeleted { .. } => "library_folder_deleted",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "asset_updated",
            "asset_updated",
            "library_folder_deleted",
            "library_folder_deleted"
        ]
    );
    assert!(state.assets.values().all(|a| a.folder_id.is_none()));
    assert_eq!(state.assets[&asset_id(0)].version, Version(3));
    assert_eq!(
        state.library_folders.keys().collect::<Vec<_>>(),
        [&folder(2)]
    );
}

#[test]
fn renaming_an_asset_changes_its_shown_name() {
    let mut state = library_state();
    run(
        &mut state,
        Role::Gm,
        Intent::RenameAsset {
            id: asset_id(0),
            name: "  Tavern theme ".into(),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        state.assets[&asset_id(0)].original_name.as_deref(),
        Some("Tavern theme")
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::RenameAsset {
                id: asset_id(0),
                name: " ".into(),
            },
            Some(Version(2))
        ),
        Err(RejectReason::invalid("name"))
    );
}

#[test]
fn looping_sounds_stay_until_stopped_and_only_sounds_play() {
    let mut state = library_state();
    let events = run(&mut state, Role::Gm, play(0, true), None).unwrap();
    let Some(Event::SoundPlayed { sound }) = events.first() else {
        panic!("{events:?}");
    };
    assert_eq!(
        (sound.sha256.as_str(), sound.mime.as_str()),
        (hex(0x40).as_str(), "audio/ogg")
    );
    run(&mut state, Role::Gm, play(1, false), None).unwrap();
    // Only the looping one is kept (and sent in snapshots).
    assert_eq!(state.sounds.keys().collect::<Vec<_>>(), [&asset_id(0)]);
    assert_eq!(state.to_snapshot(Seq(1)).sounds.len(), 1);
    run(
        &mut state,
        Role::Gm,
        Intent::StopSound {
            asset_id: Some(asset_id(0)),
        },
        None,
    )
    .unwrap();
    assert!(state.sounds.is_empty());

    assert_eq!(
        run(&mut state, Role::Gm, play(2, false), None),
        Err(RejectReason::invalid("asset_id"))
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::PlaySound {
                asset_id: asset_id(0),
                volume: MAX_VOLUME + 1,
                looped: false,
            },
            None
        ),
        Err(RejectReason::invalid("volume"))
    );
}

#[test]
fn players_hear_sounds_but_never_see_library_folders() {
    let mut state = library_state();
    run(
        &mut state,
        Role::Gm,
        Intent::MoveAsset {
            id: asset_id(0),
            folder_id: Some(folder(0)),
        },
        Some(Version(1)),
    )
    .unwrap();
    run(&mut state, Role::Gm, play(0, true), None).unwrap();
    let view = players_state(&state);
    assert!(view.library_folders.is_empty());
    assert!(view.assets.values().all(|a| a.folder_id.is_none()));
    // The music plays on for players (they fetch it by hash, not from the library).
    assert_eq!(view.sounds.len(), 1);
}

#[test]
fn a_trigger_plays_and_stops_sounds() {
    let mut state = library_state();
    let definition = TriggerDef {
        name: "Ambush".into(),
        enabled: true,
        once: false,
        event: TriggerEvent::Manual,
        tokens: Vec::new(),
        condition: None,
        steps: vec![
            TriggerStep {
                when: None,
                action: TriggerAction::StopSound { asset: None },
            },
            TriggerStep {
                when: None,
                action: TriggerAction::PlaySound {
                    asset: asset_id(1),
                    volume: 100,
                    looped: true,
                },
            },
        ],
    };
    let trigger = TriggerId(v7(0x9300));
    run(&mut state, Role::Gm, play(0, true), None).unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger,
            definition: definition.clone(),
        },
        None,
    )
    .unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger,
            token: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(state.sounds.keys().collect::<Vec<_>>(), [&asset_id(1)]);

    // A picture is not a sound.
    let mut bad = definition;
    bad.steps[1].action = TriggerAction::PlaySound {
        asset: asset_id(2),
        volume: 100,
        looped: false,
    };
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateTrigger {
                id: TriggerId(v7(0x9301)),
                definition: bad,
            },
            None
        ),
        Err(RejectReason::invalid("steps"))
    );
}

#[test]
fn sound_records_are_checked() {
    let mut bad_hash = sound_asset(5);
    bad_hash.display_sha256 = Some(hex(0x01));
    let mut bad_mime = sound_asset(6);
    bad_mime.mime = "image/png".into();
    let mut filed = sound_asset(7);
    filed.folder_id = Some(folder(0));
    for (asset, field) in [
        (bad_hash, "display_sha256"),
        (bad_mime, "mime"),
        (filed, "folder_id"),
    ] {
        let mut state = library_state();
        assert_eq!(
            run(
                &mut state,
                Role::Gm,
                Intent::RegisterAsset {
                    asset: Box::new(asset)
                },
                None
            ),
            Err(RejectReason::invalid(field))
        );
    }
}
