//! SQLite schema, migrations, repositories, projection of events into the database.
//!
//! One schema serves both the desktop campaign file and the server's session file (ADR-0002).
//! Only runtime-checked queries are used, so building needs no database (see README).

mod backup;
mod campaign;
mod codec;
mod connect;
mod engine;
mod error;

pub use backup::{MAX_BACKUPS, backup_dir, create_backup, list_backups, restore_latest_backup};
pub use campaign::{BatchRecord, CampaignDb, LocalUser, OpenReport, RecentIntent};
pub use connect::{
    CAMPAIGN_MIGRATOR, SERVER_MIGRATOR, has_pending_migrations, migrate, open_server_db,
    quick_check,
};
pub use engine::{CampaignEngine, EngineError};
pub use error::{DbError, DbResult};
