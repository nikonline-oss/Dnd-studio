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
    let (outcome, batch) = server.submit(&create_map(map_id)).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert_eq!(batch.unwrap().seq, Seq(1));

    let token_id = TokenId::new_v7();
    server
        .submit(&envelope(
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
        ))
        .await
        .unwrap();
    server
        .submit(&envelope(
            Intent::MoveToken {
                id: token_id,
                x: 70.0,
                y: 140.0,
            },
            None,
        ))
        .await
        .unwrap();

    // A stale edit is rejected without touching state.
    let (outcome, batch) = server
        .submit(&envelope(
            Intent::UpdateMap {
                id: map_id,
                name: Some("Stale".into()),
                grid: None,
                visible_to_players: None,
            },
            Some(Version(7)),
        ))
        .await
        .unwrap();
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

/// ADR-0014: an accepted batch is on disk by the time `submit` returns (before any broadcast).
#[tokio::test]
async fn accepted_batch_is_committed_before_submit_returns() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Write-through")
        .await
        .unwrap();

    server.submit(&create_map(MapId::new_v7())).await.unwrap();
    assert_eq!(stored_seq(&server).await, Seq(1));
    server.submit(&create_map(MapId::new_v7())).await.unwrap();
    assert_eq!(stored_seq(&server).await, Seq(2));

    // A rejected intent writes nothing.
    server.submit(&create_map(MapId::new_v7())).await.unwrap();
    let (outcome, _) = server
        .submit(&envelope(
            Intent::DeleteMap {
                id: MapId::new_v7(),
            },
            Some(Version(1)),
        ))
        .await
        .unwrap();
    assert!(matches!(outcome, IntentOutcome::Rejected { .. }));
    assert_eq!(stored_seq(&server).await, Seq(3));
}

/// A failing commit leaves memory, disk and the sequence counter untouched, and the next intent
/// continues from the same `seq` once storage works again.
#[tokio::test]
async fn storage_failure_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Faulty").await.unwrap();
    server.submit(&create_map(MapId::new_v7())).await.unwrap();
    let before = server.snapshot();

    // Simulate a storage failure from a second connection.
    let (db, _) = CampaignDb::open(&server.folder().join("campaign.sqlite"), 0)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_writes BEFORE INSERT ON event_log \
         BEGIN SELECT RAISE(ABORT, 'disk full'); END",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let err = server
        .submit(&create_map(MapId::new_v7()))
        .await
        .unwrap_err();
    assert_eq!(err.code, "storage_error");
    assert_eq!(server.snapshot(), before);
    assert_eq!(stored_seq(&server).await, Seq(1));

    sqlx::query("DROP TRIGGER fail_writes")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await.unwrap();
    let (outcome, _) = server.submit(&create_map(MapId::new_v7())).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(2) });
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

/// ADR-0015: a repeated intent (reconnect, `pending_ops` replay) returns the original outcome
/// and is not applied or written twice.
#[tokio::test]
async fn repeated_intent_returns_original_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Idempotent").await.unwrap();

    let create = create_map(MapId::new_v7());
    let (first, batch) = server.submit(&create).await.unwrap();
    assert_eq!(first, IntentOutcome::Accepted { seq: Seq(1) });
    assert!(batch.is_some());

    let (again, batch) = server.submit(&create).await.unwrap();
    assert_eq!(again, first);
    assert!(batch.is_none(), "nothing is re-broadcast");
    assert_eq!(server.snapshot().maps.len(), 1);
    assert_eq!(stored_seq(&server).await, Seq(1));

    // A rejection is remembered too, even if a retry would now succeed.
    let map_id = MapId::new_v7();
    let early = envelope(
        Intent::UpdateMap {
            id: map_id,
            name: Some("Too early".into()),
            grid: None,
            visible_to_players: None,
        },
        Some(Version(1)),
    );
    let (rejected, _) = server.submit(&early).await.unwrap();
    assert_eq!(
        rejected,
        IntentOutcome::Rejected {
            reason: RejectReason::NotFound
        }
    );
    server.submit(&create_map(map_id)).await.unwrap();
    let (again, batch) = server.submit(&early).await.unwrap();
    assert_eq!(again, rejected);
    assert!(batch.is_none());
}

/// Accepted intents are recognised after a restart, from `event_log`.
#[tokio::test]
async fn idempotency_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Restart").await.unwrap();
    let create = create_map(MapId::new_v7());
    server.submit(&create).await.unwrap();
    server.submit(&create_map(MapId::new_v7())).await.unwrap();
    let folder = server.folder().to_owned();
    server.close().await.unwrap();

    let mut reopened = LocalServer::open(&folder).await.unwrap();
    let (outcome, batch) = reopened.submit(&create).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(1) });
    assert!(batch.is_none());
    assert_eq!(reopened.snapshot().maps.len(), 2);
    assert_eq!(stored_seq(&reopened).await, Seq(2));
}

/// Only the idempotency window is restored on open: with a window of 2 intents, the oldest
/// accepted intent is forgotten and a repeat is evaluated afresh (here: the id already exists).
#[tokio::test]
async fn only_the_window_is_restored_on_open() {
    use dnd_game_core::IntentWindow;
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Window").await.unwrap();
    let first = create_map(MapId::new_v7());
    server.submit(&first).await.unwrap();
    let second = create_map(MapId::new_v7());
    server.submit(&second).await.unwrap();
    let third = create_map(MapId::new_v7());
    server.submit(&third).await.unwrap();
    let folder = server.folder().to_owned();
    server.close().await.unwrap();

    let window = IntentWindow {
        max_count: 2,
        max_age_ms: IntentWindow::DEFAULT.max_age_ms,
    };
    let mut reopened = LocalServer::open_with(&folder, window).await.unwrap();
    // Inside the window: original outcome.
    let (outcome, _) = reopened.submit(&second).await.unwrap();
    assert_eq!(outcome, IntentOutcome::Accepted { seq: Seq(2) });
    // Outside the window: a new intent, rejected by the core because the map exists.
    let (outcome, _) = reopened.submit(&first).await.unwrap();
    assert_eq!(
        outcome,
        IntentOutcome::Rejected {
            reason: RejectReason::AlreadyExists
        }
    );
    assert_eq!(reopened.snapshot().maps.len(), 3);
}

/// ADR-0015: the same intent_id with different data is rejected explicitly — in memory and after
/// a restart (the payload hash is stored in event_log).
#[tokio::test]
async fn reused_intent_id_with_other_payload_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut server = LocalServer::create(dir.path(), "Reuse").await.unwrap();
    let map_id = MapId::new_v7();
    server.submit(&create_map(map_id)).await.unwrap();

    let rename = envelope(
        Intent::UpdateMap {
            id: map_id,
            name: Some("First".into()),
            grid: None,
            visible_to_players: None,
        },
        Some(Version(1)),
    );
    server.submit(&rename).await.unwrap();

    // Same id, new expected_version: a different intent → explicit rejection, nothing applied.
    let mut retried = rename.clone();
    retried.expected_version = Some(Version(2));
    let reused = IntentOutcome::Rejected {
        reason: RejectReason::IntentIdReused,
    };
    assert_eq!(
        server.submit(&retried).await.unwrap(),
        (reused.clone(), None)
    );
    assert_eq!(server.snapshot().maps[0].version, Version(2));
    // The genuine repeat still returns the original outcome.
    assert_eq!(
        server.submit(&rename).await.unwrap().0,
        IntentOutcome::Accepted { seq: Seq(2) }
    );

    let folder = server.folder().to_owned();
    server.close().await.unwrap();
    let mut reopened = LocalServer::open(&folder).await.unwrap();
    assert_eq!(reopened.submit(&retried).await.unwrap(), (reused, None));
    assert_eq!(
        reopened.submit(&rename).await.unwrap().0,
        IntentOutcome::Accepted { seq: Seq(2) }
    );
}
