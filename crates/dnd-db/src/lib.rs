//! SQLite schema, migrations, repositories, projection of events into the database.
//!
//! One schema serves both the desktop campaign file and the server's session file (ADR-0002).
//! Only runtime-checked queries are used, so building needs no database (see README).

mod archive;
mod backup;
mod campaign;
mod character_store;
mod codec;
mod connect;
mod engine;
mod error;
mod rng;
mod runtime_store;
mod server;

pub use archive::{
    ARCHIVE_DB, ARCHIVE_FORMAT, ArchiveLimits, ArchiveManifest, ArchiveStats, read_archive,
    write_archive,
};
pub use backup::{MAX_BACKUPS, backup_dir, create_backup, list_backups, restore_latest_backup};
pub use campaign::{
    BatchRecord, CampaignDb, LocalUser, OpenReport, PendingOp, RecentIntent, StoredEvent,
};
pub use character_store::StoredCharacter;
pub use connect::{
    CAMPAIGN_MIGRATOR, SERVER_MIGRATOR, has_pending_migrations, migrate, open_server_db,
    quick_check,
};
pub use engine::{CampaignEngine, EngineError};
pub use error::{DbError, DbResult};
pub use rng::OsRng;
pub use runtime_store::{RUNTIME_VERSION, decode as decode_runtime, encode as encode_runtime};
pub use server::{ParticipantRow, ParticipantStatus, ServerDb, SessionRow, SessionState};
