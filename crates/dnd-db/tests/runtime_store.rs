//! The saved rules process across versions (ADR-0051): an old campaign keeps its pending action,
//! a newer one is refused with a clear error instead of an empty runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::Path;

use dnd_db::{CampaignDb, DbError, LocalUser};
use dnd_protocol::{IntentId, MovementMode, PendingMovement, PluginRuntime, Role, TokenId, UserId};
use uuid::Uuid;

fn gm() -> LocalUser {
    LocalUser {
        id: UserId(Uuid::from_u128(1)),
        name: "GM".into(),
        role: Role::Gm,
    }
}

async fn campaign(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("campaign.sqlite");
    let db = CampaignDb::create(&path, "Rules", &gm(), 0).await.unwrap();
    db.close().await.unwrap();
    path
}

async fn write_runtime(path: &Path, dto: &str) {
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    sqlx::query("INSERT INTO plugin_runtime (id, dto) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET dto = excluded.dto")
        .bind(dto)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn read_runtime(path: &Path) -> String {
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    let dto: String = sqlx::query_scalar("SELECT dto FROM plugin_runtime WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    dto
}

fn pending() -> PluginRuntime {
    PluginRuntime {
        movements: vec![PendingMovement {
            id: IntentId::new_v7(),
            token: TokenId::new_v7(),
            destination_x: 32,
            destination_y: 64,
            mode: MovementMode::Voluntary,
            offers: Vec::new(),
            handled: Vec::new(),
        }],
        ..PluginRuntime::default()
    }
}

#[tokio::test]
async fn an_unversioned_campaign_opens_with_its_pending_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = campaign(dir.path()).await;
    let runtime = pending();
    write_runtime(&path, &serde_json::to_string(&runtime).unwrap()).await;

    let (db, _) = CampaignDb::open(&path, 1).await.unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.plugin_runtime, runtime);
    db.close().await.unwrap();
}

#[tokio::test]
async fn a_newer_runtime_is_refused_not_masked_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = campaign(dir.path()).await;
    write_runtime(
        &path,
        &format!(
            "{{\"version\": {}, \"runtime\": {{}}}}",
            dnd_db::RUNTIME_VERSION + 1
        ),
    )
    .await;

    let (db, _) = CampaignDb::open(&path, 1).await.unwrap();
    assert!(matches!(
        db.load_state().await,
        Err(DbError::RuntimeTooNew { .. })
    ));
    db.close().await.unwrap();
}

#[tokio::test]
async fn a_saved_runtime_carries_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = campaign(dir.path()).await;
    let runtime = pending();
    let stored = dnd_db::encode_runtime(&runtime).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        value["version"].as_u64(),
        Some(u64::from(dnd_db::RUNTIME_VERSION))
    );
    write_runtime(&path, &stored).await;
    assert_eq!(read_runtime(&path).await, stored);

    let (db, _) = CampaignDb::open(&path, 1).await.unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.plugin_runtime, runtime);
    db.close().await.unwrap();
}
