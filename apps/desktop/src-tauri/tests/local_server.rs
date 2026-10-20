//! The local server end to end: intent → events → SQLite → reopen → same state.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{CampaignDb, list_backups};
use dnd_desktop_lib::{IntentOutcome, LocalServer, folder_name};
use dnd_protocol::*;

fn envelope(intent: Intent, expected_version: Option<Version>) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version,
        intent,
    }
}

fn create_map(id: MapId) -> IntentEnvelope {
    envelope(
        Intent::CreateMap {
            id,
            name: "Cave".into(),
            background_asset_id: None,
            grid: Grid::default(),
            visible_to_players: false,
        },
        None,
    )
}

/// Events stored in the campaign file, read through a second connection (WAL allows it).
async fn stored_seq(server: &LocalServer) -> Seq {
    let (db, _) = CampaignDb::open(&server.folder().join("campaign.sqlite"), 0)
        .await
        .unwrap();
    let seq = db.last_seq().await.unwrap();
    db.close().await.unwrap();
    seq
}

#[tokio::test]
async fn intents_persist_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Lost Mine").await.unwrap();
    let info = server.info();
    assert_eq!(info.name, "Lost Mine");
    assert_eq!(info.role, Role::Gm);

    let map_id = MapId::new_v7();
    let (outcome, batch) = server.submit(&create_map(map_id));
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert_eq!(batch.unwrap().seq, Seq(1));

    let token_id = TokenId::new_v7();
    server.submit(&envelope(
        Intent::CreateToken {
            id: token_id,
            map_id,
            character_id: None,
            asset_id: None,
            name: "Goblin".into(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale: 1.0,
            layer: TokenLayer::Tokens,
            visible: true,
        },
        None,
    ));
    server.submit(&envelope(
        Intent::MoveToken {
            id: token_id,
            x: 70.0,
            y: 140.0,
        },
        None,
    ));

    // A stale edit is rejected without touching state.
    let (outcome, batch) = server.submit(&envelope(
        Intent::UpdateMap {
            id: map_id,
            name: Some("Stale".into()),
            grid: None,
            visible_to_players: None,
        },
        Some(Version(7)),
    ));
    assert_eq!(
        outcome,
        IntentOutcome::Rejected {
            reason: RejectReason::StaleVersion {
                current: Version(1)
            }
        }
    );
    assert!(batch.is_none());

    let before = server.snapshot();
    assert_eq!(before.seq, Seq(3));
    let folder = server.folder().to_owned();
    // close() flushes the autosave queue.
    server.close().await.unwrap();

    let reopened = LocalServer::open(&folder).await.unwrap();
    assert_eq!(reopened.snapshot(), before);
    assert_eq!(reopened.info().user_id, info.user_id);
}

#[tokio::test]
async fn changes_are_queued_until_flush() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Queue").await.unwrap();
    assert!(!server.is_dirty());

    server.submit(&create_map(MapId::new_v7()));
    assert!(server.is_dirty());
    assert_eq!(server.snapshot().maps.len(), 1, "state updates immediately");
    assert_eq!(stored_seq(&server).await, Seq(0), "disk updates on flush");

    server.flush().await.unwrap();
    assert!(!server.is_dirty());
    assert_eq!(stored_seq(&server).await, Seq(1));

    server.submit(&create_map(MapId::new_v7()));
    server.flush_and_checkpoint().await.unwrap();
    assert_eq!(stored_seq(&server).await, Seq(2));
}

#[tokio::test]
async fn opening_takes_a_backup_when_none_is_recent() {
    let dir = tempfile::tempdir().unwrap();
    let server = LocalServer::create(dir.path(), "Backed").await.unwrap();
    let folder = server.folder().to_owned();
    server.close().await.unwrap();
    let file = folder.join("campaign.sqlite");
    assert!(list_backups(&file).unwrap().is_empty());

    LocalServer::open(&folder)
        .await
        .unwrap()
        .close()
        .await
        .unwrap();
    assert_eq!(list_backups(&file).unwrap().len(), 1);

    // A fresh backup exists, so the next open does not take another one.
    LocalServer::open(&folder)
        .await
        .unwrap()
        .close()
        .await
        .unwrap();
    assert_eq!(list_backups(&file).unwrap().len(), 1);
}

#[tokio::test]
async fn creating_twice_in_same_place_fails() {
    let dir = tempfile::tempdir().unwrap();
    let first = LocalServer::create(dir.path(), "Twin").await.unwrap();
    first.close().await.unwrap();
    let err = LocalServer::create(dir.path(), "Twin").await.unwrap_err();
    assert_eq!(err.code, "already_exists");
}

#[tokio::test]
async fn opening_a_non_campaign_folder_is_a_user_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = LocalServer::open(dir.path()).await.unwrap_err();
    assert_eq!(err.code, "not_a_campaign");
}

#[tokio::test]
async fn corrupted_campaign_is_a_recoverable_error() {
    let dir = tempfile::tempdir().unwrap();
    let server = LocalServer::create(dir.path(), "Broken").await.unwrap();
    let folder = server.folder().to_owned();
    server.close().await.unwrap();
    std::fs::write(folder.join("campaign.sqlite"), b"definitely not sqlite").unwrap();

    let err = LocalServer::open(&folder).await.unwrap_err();
    assert_eq!(err.code, "campaign_corrupt");
    assert_eq!(err.kind, dnd_desktop_lib::IpcErrorKind::Recoverable);
}

#[test]
fn folder_names_are_safe() {
    assert_eq!(folder_name("Curse of Strahd"), "Curse of Strahd");
    assert_eq!(folder_name("a/b\\c:d*e?"), "a_b_c_d_e_");
    assert_eq!(folder_name("  trailing dots... "), "trailing dots");
    assert_eq!(folder_name("..."), "campaign");
    assert_eq!(folder_name(""), "campaign");
    assert_eq!(folder_name(&"x".repeat(100)).len(), 64);
}

/// Regenerates `src/shared/ipc/bindings.ts`. CI fails if the committed file differs.
#[test]
fn export_bindings() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(dnd_desktop_lib::BINDINGS_PATH);
    dnd_desktop_lib::export_bindings(&path).unwrap();
}

/// The release-only config (merged by `release.yml` via `--config`) must stay valid for the
/// updater plugin and keep producing signed updater artifacts.
#[test]
fn release_config_enables_signed_updates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.release.conf.json");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(config["bundle"]["createUpdaterArtifacts"], true);
    let updater: tauri_plugin_updater::Config =
        serde_json::from_value(config["plugins"]["updater"].clone()).unwrap();
    assert_eq!(updater.endpoints.len(), 1);
    assert_eq!(updater.endpoints[0].scheme(), "https");
    assert!(
        updater.endpoints[0]
            .path()
            .ends_with("/releases/latest/download/latest.json")
    );
}

/// Dev builds must not carry an updater config (the plugin would then need a real key).
#[test]
fn dev_config_has_no_updater() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(config["plugins"]["updater"].is_null());
}
