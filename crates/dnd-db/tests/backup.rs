//! Integrity check, backups before migration, rotation and restore (CLAUDE.md §10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::*;
use dnd_protocol::{Role, UserId};
use uuid::Uuid;

fn gm() -> LocalUser {
    LocalUser {
        id: UserId(Uuid::from_u128(1)),
        name: "GM".into(),
        role: Role::Gm,
    }
}

#[tokio::test]
async fn corrupted_file_is_detected_and_restored_from_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Crypt.sqlite");
    let db = CampaignDb::create(&path, "Crypt", &gm(), 0).await.unwrap();
    let backup = db.backup("manual", 1).await.unwrap();
    assert!(backup.starts_with(backup_dir(&path)));
    db.close().await.unwrap();

    // Overwrite the middle of the file with garbage.
    let mut bytes = std::fs::read(&path).unwrap();
    let len = bytes.len();
    for b in &mut bytes[len / 4..len] {
        *b = 0xAB;
    }
    std::fs::write(&path, &bytes).unwrap();

    let err = CampaignDb::open(&path, 2).await.unwrap_err();
    assert!(matches!(err, DbError::Corrupt { .. }), "{err:?}");

    let restored_from = restore_latest_backup(&path).unwrap();
    assert_eq!(restored_from, backup);
    assert!(dir.path().join("Crypt.sqlite.corrupt").exists());

    let (db, _) = CampaignDb::open(&path, 3).await.unwrap();
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.name, "Crypt");
}

#[tokio::test]
async fn non_database_file_is_reported_as_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.sqlite");
    std::fs::write(&path, b"this is not a database at all, just some text").unwrap();
    assert!(matches!(
        quick_check(&path).await,
        Err(DbError::Corrupt { .. })
    ));
}

#[tokio::test]
async fn restore_without_backups_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("none.sqlite");
    assert!(matches!(
        restore_latest_backup(&path),
        Err(DbError::NoBackup(_))
    ));
}

#[tokio::test]
async fn backups_are_rotated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.sqlite");
    let db = CampaignDb::create(&path, "C", &gm(), 0).await.unwrap();
    let mut made = Vec::new();
    for t in 0..8 {
        made.push(db.backup("manual", 100 + t).await.unwrap());
    }
    let kept = list_backups(&path).unwrap();
    assert_eq!(kept.len(), MAX_BACKUPS);
    assert_eq!(kept, made[made.len() - MAX_BACKUPS..]);
}

#[tokio::test]
async fn pending_migrations_trigger_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.sqlite");
    // A valid but empty SQLite database stands in for a file from an older schema version.
    {
        use sqlx::Connection;
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&opts).await.unwrap();
        sqlx::query("CREATE TABLE legacy (x INTEGER)")
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.unwrap();
    }
    let (db, report) = CampaignDb::open(&path, 42).await.unwrap();
    let backup = report
        .pre_migration_backup
        .expect("backup before migration");
    assert!(backup.to_string_lossy().contains("pre-migration"));
    assert!(
        !has_pending_migrations(db.pool(), &CAMPAIGN_MIGRATOR)
            .await
            .unwrap()
    );
}
