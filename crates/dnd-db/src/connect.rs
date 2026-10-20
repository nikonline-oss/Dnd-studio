//! Connection setup, integrity check and migrations.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePool, SqlitePoolOptions,
    SqliteSynchronous,
};
use sqlx::{Connection, Row};

use crate::error::{DbError, DbResult};

/// Campaign schema (client campaign file and server session file).
pub static CAMPAIGN_MIGRATOR: Migrator = sqlx::migrate!("./migrations/campaign");
/// Session server index (`server.sqlite`).
pub static SERVER_MIGRATOR: Migrator = sqlx::migrate!("./migrations/server");

pub(crate) fn connect_options(path: &Path, create: bool) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
}

pub(crate) async fn pool(path: &Path, create: bool) -> DbResult<SqlitePool> {
    Ok(SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(connect_options(path, create))
        .await?)
}

/// Runs `PRAGMA quick_check` on an existing file. Any answer other than a single `ok`, or a file
/// that SQLite cannot read at all, is reported as [`DbError::Corrupt`].
pub async fn quick_check(path: &Path) -> DbResult<()> {
    if !path.exists() {
        return Err(DbError::NotFound(path.to_owned()));
    }
    // Read-only and without WAL changes, so the check itself never modifies a damaged file.
    let options = SqliteConnectOptions::from_str("sqlite://")?
        .filename(path)
        .read_only(true);
    let mut conn = SqliteConnection::connect_with(&options)
        .await
        .map_err(corrupt_or)?;
    let rows = sqlx::query("PRAGMA quick_check")
        .fetch_all(&mut conn)
        .await
        .map_err(corrupt_or)?;
    conn.close().await.ok();
    let messages: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>(0).ok())
        .collect();
    if messages == ["ok"] {
        Ok(())
    } else {
        Err(DbError::Corrupt {
            details: messages.join("; "),
        })
    }
}

/// SQLite reports damaged files as generic database errors; classify them as corruption.
fn corrupt_or(e: sqlx::Error) -> DbError {
    match &e {
        sqlx::Error::Database(db) => {
            let msg = db.message().to_ascii_lowercase();
            if msg.contains("malformed")
                || msg.contains("not a database")
                || msg.contains("corrupt")
            {
                DbError::Corrupt {
                    details: db.message().to_owned(),
                }
            } else {
                DbError::Sqlx(e)
            }
        }
        _ => DbError::Sqlx(e),
    }
}

/// Whether `migrator` has migrations not yet applied to `pool`.
pub async fn has_pending_migrations(pool: &SqlitePool, migrator: &Migrator) -> DbResult<bool> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if !table_exists {
        return Ok(migrator.iter().next().is_some());
    }
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
            .fetch_all(pool)
            .await?;
    Ok(migrator.iter().any(|m| !applied.contains(&m.version)))
}

/// Applies pending migrations (forward only).
pub async fn migrate(pool: &SqlitePool, migrator: &Migrator) -> DbResult<()> {
    match migrator.run(pool).await {
        Ok(()) => Ok(()),
        Err(MigrateError::VersionMissing(_)) => Err(DbError::SchemaTooNew),
        Err(e) => Err(e.into()),
    }
}

/// Opens (creating if needed) and migrates the session server index.
pub async fn open_server_db(path: &Path) -> DbResult<SqlitePool> {
    let pool = pool(path, true).await?;
    migrate(&pool, &SERVER_MIGRATOR).await?;
    Ok(pool)
}
