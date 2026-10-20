use std::path::PathBuf;

/// Errors of the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// `PRAGMA quick_check` failed or the file is not a SQLite database.
    #[error("campaign file is corrupted: {details}")]
    Corrupt { details: String },
    /// The file was migrated by a newer version of the application.
    #[error("campaign was saved by a newer version of DndStudio; update the application")]
    SchemaTooNew,
    #[error("file already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("no backups found for {0}")]
    NoBackup(PathBuf),
    /// A value in the database does not match the expected format.
    #[error("invalid data in column `{column}`: {details}")]
    InvalidData {
        column: &'static str,
        details: String,
    },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DbResult<T> = Result<T, DbError>;
