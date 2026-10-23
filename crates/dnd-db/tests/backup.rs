//! Integrity check, backups before migration, rotation and restore (CLAUDE.md §10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::*;
use dnd_protocol::{JournalEntryId, JournalFolderId, Role, UserId};
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

/// Migration 0005 (ADR-0016): maps get 4000×3000, float positions become Units16, rounded and
/// clamped onto the map.
#[tokio::test]
async fn float_positions_are_converted_to_units16() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v4.sqlite");
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        CAMPAIGN_MIGRATOR.run_to(4, &pool).await.unwrap();
        for sql in [
            "INSERT INTO campaign_meta (key, value) VALUES ('campaign.name', 'Old')",
            "INSERT INTO maps (id, name, grid_kind, grid_cell_size, visible_to_players) \
             VALUES ('0190a000-0000-7000-8000-00000000000f', 'Map', 'square', 70.0, 1)",
            "INSERT INTO tokens (id, map_id, x, y, layer, visible) VALUES ('0190a000-0000-7000-8000-00000000000a', '0190a000-0000-7000-8000-00000000000f', 70.4, 35.53, 'tokens', 1)",
            "INSERT INTO tokens (id, map_id, x, y, layer, visible) VALUES ('0190a000-0000-7000-8000-00000000000b', '0190a000-0000-7000-8000-00000000000f', -3011.6, 99999.0, 'gm', 0)",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool.close().await;
    }

    let (db, report) = CampaignDb::open(&path, 7).await.unwrap();
    assert!(report.pre_migration_backup.is_some());
    let pool = db.pool();
    let size: (i64, i64) = sqlx::query_as("SELECT width, height FROM maps")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, (4000, 3000));
    let positions: Vec<(String, i64, i64)> =
        sqlx::query_as("SELECT id, x, y FROM tokens ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        positions,
        [
            ("0190a000-0000-7000-8000-00000000000a".into(), 1126, 568),
            ("0190a000-0000-7000-8000-00000000000b".into(), 0, 3000 * 16)
        ]
    );
    // The rebuilt table keeps its constraints and indexes.
    assert!(
        sqlx::query("UPDATE tokens SET x = -1")
            .execute(pool)
            .await
            .is_err()
    );
    let (state, _) = db.load_state().await.unwrap();
    assert_eq!(state.tokens.len(), 2);
}

/// Migration 0009 (ADR-0021): folder paths of existing entries become nested folder rows; every
/// entry points at its folder; root entries stay at the root.
#[tokio::test]
async fn folder_paths_become_folder_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v8.sqlite");
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
        CAMPAIGN_MIGRATOR.run_to(8, &pool).await.unwrap();
        for (n, folder) in [
            (1, "Lore"),
            (2, "Lore/Places/Towns"),
            (3, ""),
            (4, "Lore/Places"),
        ] {
            sqlx::query(
                "INSERT INTO journal_entries (id, folder, title, created_at, updated_at) \
                 VALUES (?, ?, ?, 0, 0)",
            )
            .bind(format!("0190a000-0000-7000-8000-00000000000{n}"))
            .bind(folder)
            .bind(format!("Entry {n}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;
    }

    let (db, report) = CampaignDb::open(&path, 7).await.unwrap();
    assert!(report.pre_migration_backup.is_some());
    let (state, _) = db.load_state().await.unwrap();

    // Lore > Places > Towns: three folders, each prefix once.
    let path_of = |id: JournalFolderId| {
        let mut names = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            let folder = &state.journal_folders[&id];
            names.push(folder.name.clone());
            current = folder.parent;
        }
        names.reverse();
        names.join("/")
    };
    let mut paths: Vec<_> = state
        .journal_folders
        .keys()
        .map(|&id| path_of(id))
        .collect();
    paths.sort();
    assert_eq!(paths, ["Lore", "Lore/Places", "Lore/Places/Towns"]);
    for folder in state.journal_folders.values() {
        assert!(dnd_protocol::is_uuid_v7(&folder.id.0), "{}", folder.id);
    }

    let folder_of = |n: u128| {
        let id = JournalEntryId(Uuid::from_u128(
            0x0190_a000_0000_7000_8000_0000_0000_0000 + n,
        ));
        state.journal[&id].folder_id.map(path_of)
    };
    assert_eq!(folder_of(1).as_deref(), Some("Lore"));
    assert_eq!(folder_of(2).as_deref(), Some("Lore/Places/Towns"));
    assert_eq!(folder_of(3), None);
    assert_eq!(folder_of(4).as_deref(), Some("Lore/Places"));
}
