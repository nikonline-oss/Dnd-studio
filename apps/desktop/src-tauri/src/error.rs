//! The single error type of the IPC boundary (CLAUDE.md §9).

use dnd_db::DbError;
use serde::Serialize;

/// How the UI should treat an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorKind {
    /// The user can fix it with an offered action (e.g. restore from backup).
    Recoverable,
    /// Caused by user input or choice; show the message.
    User,
    /// A bug or an unexpected environment problem; log and show a generic message.
    Internal,
    /// The application cannot continue.
    Fatal,
}

/// Every command returns `Result<T, IpcError>`. `code` is stable and used by the UI for
/// localized messages; `message` is an English diagnostic for logs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type, thiserror::Error)]
#[error("{code}: {message}")]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(kind: IpcErrorKind, code: &str, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.to_owned(),
            message: message.into(),
        }
    }

    pub fn user(code: &str, message: impl Into<String>) -> Self {
        Self::new(IpcErrorKind::User, code, message)
    }

    pub fn internal(code: &str, message: impl Into<String>) -> Self {
        Self::new(IpcErrorKind::Internal, code, message)
    }

    pub fn no_campaign() -> Self {
        Self::user("no_campaign_open", "no campaign is open")
    }
}

impl From<DbError> for IpcError {
    fn from(e: DbError) -> Self {
        let message = e.to_string();
        match e {
            DbError::Corrupt { .. } => {
                Self::new(IpcErrorKind::Recoverable, "campaign_corrupt", message)
            }
            DbError::SchemaTooNew => Self::user("campaign_too_new", message),
            DbError::AlreadyExists(_) => Self::user("already_exists", message),
            DbError::NotFound(_) => Self::user("not_found", message),
            DbError::NoBackup(_) => Self::user("no_backup", message),
            DbError::InvalidArchive { .. } => Self::user("invalid_archive", message),
            DbError::Sqlx(_)
            | DbError::Migrate(_)
            | DbError::InvalidData { .. }
            | DbError::Json(_)
            | DbError::Io(_) => {
                tracing::error!(error = %message, "storage error");
                Self::internal("storage_error", message)
            }
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!(error = %e, "I/O error");
        Self::internal("io_error", e.to_string())
    }
}
