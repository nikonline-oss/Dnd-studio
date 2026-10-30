//! The in-process "server" for local play (ADR-0001): the campaign folder around a
//! [`CampaignEngine`] — the same write-through intent pipeline the session server runs (ADR-0014).
//! Independent of Tauri so it can be tested directly.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dnd_db::{
    ArchiveLimits, ArchiveManifest, ArchiveStats, CampaignDb, CampaignEngine, EngineError,
    LocalUser, list_backups,
};
use dnd_game_core::{Actor, IntentWindow};
use dnd_protocol::{
    ChatMessageDto, ChatMessageId, EventBatch, IntentEnvelope, IntentOutcome, JournalEntryDto,
    JournalEntryId, Role, Snapshot, UserId,
};
use serde::Serialize;

use crate::error::IpcError;

/// File name of the database inside a campaign folder.
pub const CAMPAIGN_FILE: &str = "campaign.sqlite";
pub(crate) const REPLICA_NEW: &str = "campaign.sqlite.sync-new";
pub(crate) const REPLICA_OLD: &str = "campaign.sqlite.sync-old";
/// Asset folder inside a campaign folder.
pub const ASSETS_DIR: &str = "assets";
/// A backup is taken on open when the newest one is older than this.
pub const BACKUP_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// What the UI needs to know about the open campaign.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct CampaignInfo {
    pub name: String,
    /// The campaign folder.
    pub path: String,
    pub user_id: UserId,
    pub role: Role,
}

#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// Makes a folder name out of a campaign name: no path separators, reserved characters or
/// trailing dots/spaces (Windows), at most 64 characters.
#[must_use]
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .take(64)
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_owned();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '.') {
        "campaign".to_owned()
    } else {
        trimmed
    }
}

/// An open campaign and its authoritative state.
#[derive(Debug)]
pub struct LocalServer {
    engine: CampaignEngine,
    folder: PathBuf,
    /// The local user: every intent of this app is theirs.
    actor: Actor,
}

impl From<EngineError> for IpcError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Db(e) => e.into(),
            EngineError::SeqExhausted => Self::internal("seq_exhausted", e.to_string()),
        }
    }
}

impl LocalServer {
    /// Creates `<parent>/<folder_name(name)>/` with a new campaign file and an assets folder.
    pub async fn create(parent: &Path, name: &str) -> Result<Self, IpcError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(IpcError::user("invalid_name", "campaign name is empty"));
        }
        let folder = parent.join(folder_name(name));
        if folder.exists() {
            return Err(IpcError::user(
                "already_exists",
                format!("{} already exists", folder.display()),
            ));
        }
        std::fs::create_dir_all(folder.join(ASSETS_DIR))?;
        let gm = LocalUser {
            id: UserId::new_v7(),
            name: "Game Master".to_owned(),
            role: Role::Gm,
        };
        let db = CampaignDb::create(&folder.join(CAMPAIGN_FILE), name, &gm, now_ms()).await?;
        tracing::info!(folder = %folder.display(), "campaign created");
        Self::from_db(db, folder, IntentWindow::DEFAULT).await
    }

    /// Opens the campaign in `folder`: integrity check, migrations (with a backup before them),
    /// and a fresh backup when the newest one is older than [`BACKUP_MAX_AGE`].
    pub async fn open(folder: &Path) -> Result<Self, IpcError> {
        Self::open_with(folder, IntentWindow::DEFAULT).await
    }

    /// [`open`](Self::open) with a custom idempotency window (ADR-0015).
    pub async fn open_with(folder: &Path, window: IntentWindow) -> Result<Self, IpcError> {
        let file = folder.join(CAMPAIGN_FILE);
        let previous = folder.join(REPLICA_OLD);
        if previous.exists() && (!file.exists() || dnd_db::quick_check(&file).await.is_err()) {
            restore_replica_file(folder)?;
        }
        let result = Self::open_existing(folder, window).await;
        let opened = match result {
            Ok(local) => local,
            Err(_) if previous.exists() => {
                restore_replica_file(folder)?;
                Self::open_existing(folder, window).await?
            }
            Err(error) => return Err(error),
        };
        if previous.exists() {
            std::fs::remove_file(previous).ok();
        }
        let prepared = folder.join(REPLICA_NEW);
        if prepared.exists() {
            std::fs::remove_file(prepared).ok();
        }
        Ok(opened)
    }

    /// Used during an active replacement; recovery files remain until the caller commits it.
    pub(crate) async fn open_replaced(folder: &Path) -> Result<Self, IpcError> {
        Self::open_existing(folder, IntentWindow::DEFAULT).await
    }

    async fn open_existing(folder: &Path, window: IntentWindow) -> Result<Self, IpcError> {
        let file = folder.join(CAMPAIGN_FILE);
        if !file.exists() {
            return Err(IpcError::user(
                "not_a_campaign",
                format!("{} has no {CAMPAIGN_FILE}", folder.display()),
            ));
        }
        let (db, report) = CampaignDb::open(&file, now_ms()).await?;
        if let Some(backup) = report.pre_migration_backup {
            tracing::info!(backup = %backup.display(), "campaign migrated");
        } else if backup_is_stale(&file)
            && let Err(e) = db.backup("on-open", now_ms()).await
        {
            db.close().await.ok();
            return Err(e.into());
        }
        tracing::info!(folder = %folder.display(), "campaign opened");
        Self::from_db(db, folder.to_owned(), window).await
    }

    /// Loads the campaign; on failure the database is closed before returning, so the file is
    /// released at once (Windows keeps it locked otherwise).
    async fn from_db(
        db: CampaignDb,
        folder: PathBuf,
        window: IntentWindow,
    ) -> Result<Self, IpcError> {
        let user = match db.local_user().await {
            Ok(Some(user)) => user,
            Ok(None) => {
                db.close().await.ok();
                return Err(IpcError::internal(
                    "no_local_user",
                    "campaign has no local user",
                ));
            }
            Err(e) => {
                db.close().await.ok();
                return Err(e.into());
            }
        };
        let mut engine = CampaignEngine::load(db, window, seed(), now_ms()).await?;
        let actor = Actor {
            user_id: user.id,
            role: user.role,
        };
        if let Err(error) = engine.materialize_pending(&actor, now_ms()).await {
            engine.close().await.ok();
            return Err(error.into());
        }
        Ok(Self {
            engine,
            folder,
            actor,
        })
    }

    #[must_use]
    pub fn info(&self) -> CampaignInfo {
        CampaignInfo {
            name: self.engine.state().campaign.name.clone(),
            path: self.folder.display().to_string(),
            user_id: self.actor.user_id,
            role: self.actor.role,
        }
    }

    /// The authoritative game state.
    #[must_use]
    pub fn state(&self) -> &dnd_game_core::GameState {
        self.engine.state()
    }

    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.engine.snapshot()
    }

    /// Older chat messages, oldest first, before message `before` (or the newest).
    pub async fn chat_history(
        &self,
        before: Option<ChatMessageId>,
        limit: usize,
    ) -> Result<Vec<ChatMessageDto>, IpcError> {
        Ok(self.engine.chat_history(before, limit).await?)
    }

    /// The asset registered for an original upload with this SHA-256, if any (dedup on import).
    #[must_use]
    pub fn asset_by_sha(
        &self,
        usage: dnd_protocol::AssetUsage,
        sha256: &str,
    ) -> Option<dnd_protocol::AssetId> {
        self.engine
            .state()
            .assets
            .values()
            .find(|a| a.usage == usage && a.sha256 == sha256)
            .map(|a| a.id)
    }

    /// Full text of a journal entry (the snapshot carries only the index).
    #[must_use]
    pub fn journal_entry(&self, id: &JournalEntryId) -> Option<JournalEntryDto> {
        self.engine.journal_entry(id)
    }

    /// Runs the intent pipeline as the local user (see [`CampaignEngine::submit`]); the caller
    /// broadcasts the returned batch.
    pub async fn submit(
        &mut self,
        envelope: &IntentEnvelope,
    ) -> Result<(IntentOutcome, Option<EventBatch>), IpcError> {
        if self.db().remote_binding().await?.is_some() {
            return self.submit_offline(envelope).await;
        }
        Ok(self.engine.submit(envelope, &self.actor, now_ms()).await?)
    }

    /// The local replica's durable offline queue (ADR-0004).
    pub fn db(&self) -> &CampaignDb {
        self.engine.db()
    }

    /// Follow authoritative batches only while the local copy has no speculative edits.
    pub async fn replicate(&mut self, batch: &EventBatch) -> Result<(), IpcError> {
        if batch.last_seq() <= self.engine.last_seq()
            || !self.db().pending_ops().await?.is_empty()
            || self.db().has_speculative_events().await?
        {
            return Ok(());
        }
        if batch.seq.0 != self.engine.last_seq().0 + 1 {
            return Err(IpcError::user(
                "replica_gap",
                "local replica requires a fresh server export",
            ));
        }
        self.engine
            .commit_events(batch.events.clone(), None, now_ms())
            .await?;
        Ok(())
    }

    /// Persist the replay operation and locally accepted events in one transaction.
    pub async fn submit_offline(
        &mut self,
        envelope: &IntentEnvelope,
    ) -> Result<(IntentOutcome, Option<EventBatch>), IpcError> {
        Ok(self
            .engine
            .submit_offline(envelope, &self.actor, now_ms())
            .await?)
    }

    /// Writes the campaign archive (CLAUDE.md §6) to `out`: a consistent copy of the database,
    /// the assets (with originals when `include_originals`) and the manifest.
    pub async fn export(
        &self,
        out: &Path,
        include_originals: bool,
    ) -> Result<ArchiveStats, IpcError> {
        let copy =
            std::env::temp_dir().join(format!("dndstudio-export-{}.sqlite", uuid::Uuid::now_v7()));
        self.engine.db().copy_to(&copy).await?;
        let manifest = ArchiveManifest {
            format: dnd_db::ARCHIVE_FORMAT,
            name: self.engine.state().campaign.name.clone(),
            exported_at_ms: now_ms(),
            generator: format!("dndstudio {}", env!("CARGO_PKG_VERSION")),
        };
        let assets = self.folder.join(ASSETS_DIR);
        let out = out.to_owned();
        let copy_path = copy.clone();
        let written = tokio::task::spawn_blocking(move || {
            dnd_db::write_archive(&out, &copy_path, &assets, &manifest, include_originals)
        })
        .await
        .map_err(|e| IpcError::internal("export_failed", e.to_string()));
        std::fs::remove_file(&copy).ok();
        let stats = written??;
        tracing::info!(
            files = stats.files,
            bytes = stats.bytes,
            "campaign exported"
        );
        Ok(stats)
    }

    /// Moves the WAL into the main file (periodic housekeeping, not needed for durability).
    pub async fn checkpoint(&self) -> Result<(), IpcError> {
        self.engine.checkpoint().await?;
        Ok(())
    }

    /// Checkpoints and closes the database.
    pub async fn close(self) -> Result<(), IpcError> {
        self.engine.close().await?;
        tracing::info!(folder = %self.folder.display(), "campaign closed");
        Ok(())
    }
}

fn restore_replica_file(folder: &Path) -> Result<(), IpcError> {
    let broken = format!("{CAMPAIGN_FILE}.sync-broken-{}", uuid::Uuid::now_v7());
    for suffix in ["", "-wal", "-shm"] {
        let current = folder.join(format!("{CAMPAIGN_FILE}{suffix}"));
        if current.exists() {
            std::fs::rename(current, folder.join(format!("{broken}{suffix}")))?;
        }
    }
    std::fs::rename(folder.join(REPLICA_OLD), folder.join(CAMPAIGN_FILE))?;
    Ok(())
}

/// Unpacks a campaign archive into a new folder under `parent`, named after the campaign (with
/// " (2)", " (3)"… when taken), and returns that folder. The caller opens it.
pub async fn import(archive: &Path, parent: &Path) -> Result<PathBuf, IpcError> {
    let staging = parent.join(format!(".dndstudio-import-{}", uuid::Uuid::now_v7()));
    let archive = archive.to_owned();
    let target = staging.clone();
    let (manifest, stats) = tokio::task::spawn_blocking(move || {
        dnd_db::read_archive(&archive, &target, ArchiveLimits::DEFAULT)
    })
    .await
    .map_err(|e| IpcError::internal("import_failed", e.to_string()))??;
    let base = folder_name(&manifest.name);
    let mut folder = parent.join(&base);
    let mut n = 2;
    while folder.exists() {
        folder = parent.join(format!("{base} ({n})"));
        n += 1;
    }
    if let Err(e) = std::fs::rename(&staging, &folder) {
        std::fs::remove_dir_all(&staging).ok();
        return Err(e.into());
    }
    tracing::info!(folder = %folder.display(), files = stats.files, "campaign imported");
    Ok(folder)
}

/// True when there is no backup or the newest is older than [`BACKUP_MAX_AGE`].
fn backup_is_stale(campaign_file: &Path) -> bool {
    let newest = list_backups(campaign_file)
        .ok()
        .and_then(|b| b.last().cloned());
    let Some(newest) = newest else {
        return true;
    };
    std::fs::metadata(newest)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > BACKUP_MAX_AGE)
}
