//! Migration tests: the resulting schema is snapshotted, so any schema change is reviewed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use dnd_db::{CAMPAIGN_MIGRATOR, SERVER_MIGRATOR, has_pending_migrations, migrate};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

async fn memory_pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            "sqlite::memory:"
                .parse::<SqliteConnectOptions>()
                .unwrap()
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

/// `CREATE` statements of user objects, whitespace-normalized, in a stable order.
async fn schema_dump(pool: &SqlitePool) -> String {
    let rows = sqlx::query(
        "SELECT type, name, sql FROM sqlite_schema \
         WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' \
         ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, name",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    rows.iter()
        .map(|r| {
            let sql: String = r.get("sql");
            sql.split_whitespace().collect::<Vec<_>>().join(" ") + ";"
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn campaign_schema() {
    let pool = memory_pool().await;
    assert!(
        has_pending_migrations(&pool, &CAMPAIGN_MIGRATOR)
            .await
            .unwrap()
    );
    migrate(&pool, &CAMPAIGN_MIGRATOR).await.unwrap();
    assert!(
        !has_pending_migrations(&pool, &CAMPAIGN_MIGRATOR)
            .await
            .unwrap()
    );
    insta::assert_snapshot!(schema_dump(&pool).await);
}

#[tokio::test]
async fn server_schema() {
    let pool = memory_pool().await;
    migrate(&pool, &SERVER_MIGRATOR).await.unwrap();
    insta::assert_snapshot!(schema_dump(&pool).await);
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let pool = memory_pool().await;
    migrate(&pool, &CAMPAIGN_MIGRATOR).await.unwrap();
    let before = schema_dump(&pool).await;
    migrate(&pool, &CAMPAIGN_MIGRATOR).await.unwrap();
    assert_eq!(schema_dump(&pool).await, before);
}

#[tokio::test]
async fn foreign_keys_hold_after_migration() {
    let pool = memory_pool().await;
    migrate(&pool, &CAMPAIGN_MIGRATOR).await.unwrap();
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(violations.is_empty());
}

#[tokio::test]
async fn append_only_tables_reject_updates() {
    let pool = memory_pool().await;
    migrate(&pool, &CAMPAIGN_MIGRATOR).await.unwrap();
    sqlx::query("INSERT INTO event_log (seq, event, created_at) VALUES (1, '{\"kind\":\"x\"}', 0)")
        .execute(&pool)
        .await
        .unwrap();
    let err = sqlx::query("UPDATE event_log SET created_at = 1")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("append-only"), "{err}");
}
