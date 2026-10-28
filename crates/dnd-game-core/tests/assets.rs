//! Asset registration: permissions of all four roles, record validation, dedup, references.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_protocol::arbitrary::{pooled_map_id, v7};
use dnd_protocol::*;

fn hex(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn map_asset(n: u128, sha: u8) -> AssetDto {
    AssetDto {
        id: AssetId(v7(0x7000 + n)),
        usage: AssetUsage::Map,
        sha256: hex(sha),
        mime: "image/png".into(),
        size_bytes: 1234,
        original_name: Some("crypt.png".into()),
        width: 3000,
        height: 2048,
        display_sha256: None,
        thumbnail_sha256: hex(0xEE),
        tiles: vec![
            AssetTileDto {
                x: 0,
                y: 0,
                width: 2048,
                height: 2048,
                sha256: hex(0xA1),
            },
            AssetTileDto {
                x: 2048,
                y: 0,
                width: 952,
                height: 2048,
                sha256: hex(0xA2),
            },
        ],
        folder_id: None,
        version: Version(1),
    }
}

fn register(asset: AssetDto) -> Intent {
    Intent::RegisterAsset {
        asset: Box::new(asset),
    }
}

#[test]
fn only_gm_roles_register_assets() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = table_state();
        let result = run(&mut state, role, register(map_asset(1, 1)), None);
        assert_eq!(result.is_ok(), allowed, "{role:?}: {result:?}");
        if !allowed {
            assert_eq!(result, Err(RejectReason::Forbidden));
        }
    }
}

#[test]
fn same_original_cannot_be_registered_twice() {
    let mut state = table_state();
    run(&mut state, Role::Gm, register(map_asset(1, 1)), None).unwrap();
    assert_eq!(
        run(&mut state, Role::Gm, register(map_asset(2, 1)), None),
        Err(RejectReason::AlreadyExists)
    );
}

#[test]
fn malformed_records_are_rejected() {
    let cases: Vec<(AssetDto, &str)> = vec![
        (
            AssetDto {
                sha256: "../../etc/passwd".into(),
                ..map_asset(1, 1)
            },
            "sha256",
        ),
        (
            AssetDto {
                width: 16_385,
                ..map_asset(1, 1)
            },
            "width",
        ),
        (
            AssetDto {
                mime: "image/svg+xml".into(),
                ..map_asset(1, 1)
            },
            "mime",
        ),
        (
            AssetDto {
                tiles: vec![],
                folder_id: None,
                ..map_asset(1, 1)
            },
            "tiles",
        ),
        // Tiles that do not cover the image exactly.
        (
            AssetDto {
                width: 4000,
                ..map_asset(1, 1)
            },
            "tiles",
        ),
        // Tokens have one display image and no tiles.
        (
            AssetDto {
                usage: AssetUsage::Token,
                width: 2048,
                ..map_asset(1, 1)
            },
            "tiles",
        ),
    ];
    for (asset, field) in cases {
        let mut state = table_state();
        assert_eq!(
            run(&mut state, Role::Gm, register(asset.clone()), None),
            Err(RejectReason::invalid(field)),
            "{asset:?}"
        );
    }
}

#[test]
fn map_backgrounds_must_be_map_images() {
    let mut state = table_state();
    run(&mut state, Role::Gm, register(map_asset(1, 1)), None).unwrap();
    let token = AssetDto {
        usage: AssetUsage::Token,
        width: 256,
        height: 256,
        display_sha256: Some(hex(0xD1)),
        tiles: vec![],
        folder_id: None,
        ..map_asset(2, 2)
    };
    run(&mut state, Role::Gm, register(token), None).unwrap();

    let create = |n: u128, background: Option<AssetId>| Intent::CreateMap {
        id: pooled_map_id(n),
        name: "Crypt".into(),
        background_asset_id: background,
        width: 3000,
        height: 2048,
        grid: Grid::default(),
        visible_to_players: false,
    };
    run(
        &mut state,
        Role::Gm,
        create(2, Some(AssetId(v7(0x7001)))),
        None,
    )
    .unwrap();
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            create(3, Some(AssetId(v7(0x7002)))),
            None
        ),
        Err(RejectReason::invalid("background_asset_id"))
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            create(3, Some(AssetId(v7(0x7FFF)))),
            None
        ),
        Err(RejectReason::NotFound)
    );
}
