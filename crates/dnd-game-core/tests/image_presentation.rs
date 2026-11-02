#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use dnd_game_core::{ForPlayers, filter_for_players, players_state, snapshot_for};
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;

fn picture() -> AssetDto {
    AssetDto {
        id: AssetId(v7(0xABCD)),
        usage: AssetUsage::Token,
        sha256: "a".repeat(64),
        mime: "image/png".into(),
        size_bytes: 1,
        original_name: Some("Secret art.png".into()),
        width: 800,
        height: 600,
        display_sha256: Some("b".repeat(64)),
        thumbnail_sha256: "c".repeat(64),
        tiles: vec![],
        folder_id: None,
        version: Version::INITIAL,
    }
}

#[test]
fn only_gm_roles_present_images_and_the_asset_is_shared_without_other_hidden_content() {
    for role in [Role::Gm, Role::CoGm, Role::Player, Role::Spectator] {
        let mut state = table_state();
        let asset = picture();
        state.assets.insert(asset.id, asset.clone());
        let before = state.clone();
        let events = run(
            &mut state,
            role,
            Intent::ShowImage { asset_id: asset.id },
            None,
        );
        assert_eq!(events.is_ok(), matches!(role, Role::Gm | Role::CoGm));
        if let Ok(events) = events {
            let view = snapshot_for(Role::Player, &state, Seq(1), [].iter());
            assert_eq!(view.presented_image.as_ref().unwrap().asset_id, asset.id);
            assert_eq!(view.assets.len(), 1);
            assert_eq!(view.maps.len(), 1, "hidden map remains hidden");
            assert!(matches!(
                filter_for_players(
                    &before,
                    &state,
                    &EventBatch {
                        seq: Seq(1),
                        caused_by: None,
                        events
                    }
                ),
                ForPlayers::Resnapshot
            ));
            let shown = state.presented_image.clone().unwrap();
            run(
                &mut state,
                Role::Gm,
                Intent::HideImage { id: shown.id },
                Some(shown.version),
            )
            .unwrap();
            assert!(state.presented_image.is_none());
            assert!(players_state(&state).assets.is_empty());
        }
    }
}

#[test]
fn audio_and_missing_assets_are_rejected_and_a_stale_close_cannot_hide_a_new_show() {
    let mut state = table_state();
    let mut asset = picture();
    asset.usage = AssetUsage::Audio;
    state.assets.insert(asset.id, asset.clone());
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::ShowImage { asset_id: asset.id },
            None
        )
        .is_err()
    );
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::ShowImage {
                asset_id: AssetId(v7(0xDEAD))
            },
            None
        )
        .is_err()
    );
    asset.usage = AssetUsage::Token;
    state.assets.insert(asset.id, asset.clone());
    run(
        &mut state,
        Role::Gm,
        Intent::ShowImage { asset_id: asset.id },
        None,
    )
    .unwrap();
    let old = state.presented_image.clone().unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::ShowImage { asset_id: asset.id },
        None,
    )
    .unwrap();
    assert!(
        run(
            &mut state,
            Role::Gm,
            Intent::HideImage { id: old.id },
            Some(old.version)
        )
        .is_err()
    );
    assert!(state.presented_image.is_some());
    assert!(
        run(
            &mut state,
            Role::Player,
            Intent::HideImage { id: old.id },
            Some(Version(2))
        )
        .is_err()
    );
}
