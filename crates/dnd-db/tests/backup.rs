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

/// A campaign saved by the previous schema (0001) is backed up and upgraded on open.
#[tokio::test]
async fn campaign_from_previous_schema_is_backed_up_and_upgraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.sqlite");
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        CAMPAIGN_MIGRATOR.run_to(1, &pool).await.unwrap();
        sqlx::query("INSERT INTO campaign_meta (key, value) VALUES ('campaign.name', 'Old')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }
    let (db, report) = CampaignDb::open(&path, 7).await.unwrap();
    assert!(report.pre_migration_backup.is_some());
    assert!(
        !has_pending_migrations(db.pool(), &CAMPAIGN_MIGRATOR)
            .await
            .unwrap()
    );
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.campaign.name, "Old");
}

/// Migration 0003 backfills author names of existing chat messages and dice rolls, the names
/// survive deleting the user and cannot be edited.
#[tokio::test]
async fn author_names_are_backfilled_kept_and_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.sqlite");
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        CAMPAIGN_MIGRATOR.run_to(2, &pool).await.unwrap();
        for sql in [
            "INSERT INTO campaign_meta (key, value) VALUES ('campaign.name', 'Old')",
            "INSERT INTO users (id, name, role, created_at) VALUES ('u1', 'Ann', 'player', 0)",
            "INSERT INTO chat_messages (id, sender_user_id, kind, body, created_at) VALUES ('m1', 'u1', 'text', 'hi', 0)",
            "INSERT INTO chat_messages (id, sender_user_id, kind, body, created_at) VALUES ('m2', NULL, 'system', 'start', 0)",
            "INSERT INTO dice_rolls (id, user_id, expression, result, total, created_at) VALUES ('r1', 'u1', '1d20', '[7]', 7, 0)",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool.close().await;
    }

    let (db, report) = CampaignDb::open(&path, 7).await.unwrap();
    assert!(report.pre_migration_backup.is_some());
    let pool = db.pool();
    let names: Vec<(String, String)> =
        sqlx::query_as("SELECT id, sender_name FROM chat_messages ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        names,
        [("m1".into(), "Ann".into()), ("m2".into(), String::new())]
    );

    assert!(
        sqlx::query("UPDATE chat_messages SET sender_name = 'Bob'")
            .execute(pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE dice_rolls SET user_name = 'Bob'")
            .execute(pool)
            .await
            .is_err()
    );

    sqlx::query("DELETE FROM users WHERE id = 'u1'")
        .execute(pool)
        .await
        .unwrap();
    let (user, name): (Option<String>, String) =
        sqlx::query_as("SELECT user_id, user_name FROM dice_rolls")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((user, name.as_str()), (None, "Ann"));
    let (sender, name): (Option<String>, String) =
        sqlx::query_as("SELECT sender_user_id, sender_name FROM chat_messages WHERE id = 'm1'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((sender, name.as_str()), (None, "Ann"));
}
