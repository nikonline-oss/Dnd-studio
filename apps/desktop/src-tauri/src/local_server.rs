//! The in-process "server" for local play (ADR-0001): the same intent pipeline as the session
//! server, backed by the campaign's SQLite file. Independent of Tauri so it can be tested directly.
//!
//! Persistence is write-through (CLAUDE.md §10, ADR-0014): every accepted batch is committed before
//! the in-memory state changes and before anyone is told about it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dnd_db::{BatchRecord, CampaignDb, LocalUser, list_backups};
use dnd_game_core::{Actor, GameState, IntentWindow, RecentIntents, SplitMix64, apply, evolve_all};
use dnd_protocol::{EventBatch, IntentEnvelope, IntentOutcome, Role, Seq, Snapshot, UserId};
use serde::Serialize;

use crate::error::IpcError;

/// File name of the database inside a campaign folder.
pub const CAMPAIGN_FILE: &str = "campaign.sqlite";
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
    db: CampaignDb,
    folder: PathBuf,
    state: GameState,
    next_seq: Seq,
    actor: Actor,
    rng: SplitMix64,
    /// Outcomes of recent intents by `intent_id` (ADR-0015).
    recent: RecentIntents<IntentOutcome>,
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
        if !file.exists() {
            return Err(IpcError::user(
                "not_a_campaign",
                format!("{} has no {CAMPAIGN_FILE}", folder.display()),
            ));
        }
        let (db, report) = CampaignDb::open(&file, now_ms()).await?;
        if let Some(backup) = report.pre_migration_backup {
            tracing::info!(backup = %backup.display(), "campaign migrated");
        } else if backup_is_stale(&file) {
            db.backup("on-open", now_ms()).await?;
        }
        tracing::info!(folder = %folder.display(), "campaign opened");
        Self::from_db(db, folder.to_owned(), window).await
    }

    async fn from_db(
        db: CampaignDb,
        folder: PathBuf,
        window: IntentWindow,
    ) -> Result<Self, IpcError> {
        let user = db
            .local_user()
            .await?
            .ok_or_else(|| IpcError::internal("no_local_user", "campaign has no local user"))?;
        let (state, last_seq) = db.load_state().await?;
        // Accepted intents survive restarts through `event_log`; rejections are remembered per run.
        let mut recent = RecentIntents::new(window);
        let now = now_ms();
        for r in db
            .recent_intents(window.max_count, window.since_ms(now))
            .await?
        {
            recent.insert(
                r.intent_id,
                IntentOutcome::Accepted { seq: r.last_seq },
                r.at_ms,
            );
        }
        recent.prune(now);
        Ok(Self {
            db,
            folder,
            state,
            next_seq: last_seq.next(),
            actor: Actor {
                user_id: user.id,
                role: user.role,
            },
            rng: SplitMix64::new(seed()),
            recent,
        })
    }

    #[must_use]
    pub fn info(&self) -> CampaignInfo {
        CampaignInfo {
            name: self.state.campaign.name.clone(),
            path: self.folder.display().to_string(),
            user_id: self.actor.user_id,
            role: self.actor.role,
        }
    }

    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        self.state.to_snapshot(Seq(self.next_seq.0 - 1))
    }

    /// Runs the intent pipeline write-through (ADR-0014): `apply` → commit the batch (`event_log` +
    /// projection, one transaction) → `evolve`. Memory changes only after the commit, so a storage
    /// failure leaves memory, disk and `next_seq` untouched; the caller then broadcasts the batch.
    ///
    /// Idempotent by `intent_id` (ADR-0015): a repeat returns the original outcome and no batch.
    pub async fn submit(
        &mut self,
        envelope: &IntentEnvelope,
    ) -> Result<(IntentOutcome, Option<EventBatch>), IpcError> {
        if let Some(outcome) = self.recent.get(&envelope.intent_id, now_ms()) {
            tracing::debug!(intent_id = %envelope.intent_id, "duplicate intent");
            return Ok((outcome.clone(), None));
        }
        let events = match apply(&self.state, envelope, &self.actor, &mut self.rng) {
            Ok(events) => events,
            Err(reason) => {
                tracing::debug!(
                    intent = envelope.intent.kind(),
                    code = reason.code(),
                    "intent rejected"
                );
                let outcome = IntentOutcome::Rejected { reason };
                self.recent
                    .insert(envelope.intent_id, outcome.clone(), now_ms());
                return Ok((outcome, None));
            }
        };
        let batch = EventBatch {
            seq: self.next_seq,
            caused_by: Some(envelope.intent_id),
            events,
        };
        let last = batch.last_seq();
        if !dnd_protocol::is_js_safe(last.0) {
            return Err(IpcError::internal(
                "seq_exhausted",
                "event sequence reached 2^53-1",
            ));
        }
        self.db
            .project(&[BatchRecord {
                batch: batch.clone(),
                actor: Some(self.actor.user_id),
                at_ms: now_ms(),
            }])
            .await?;
        evolve_all(&mut self.state, &batch.events);
        self.next_seq = last.next();
        let outcome = IntentOutcome::Accepted { seq: last };
        self.recent
            .insert(envelope.intent_id, outcome.clone(), now_ms());
        Ok((outcome, Some(batch)))
    }

    /// Moves the WAL into the main file (periodic housekeeping, not needed for durability).
    pub async fn checkpoint(&self) -> Result<(), IpcError> {
        self.db.checkpoint().await?;
        Ok(())
    }

    /// Checkpoints and closes the database.
    pub async fn close(self) -> Result<(), IpcError> {
        self.db.close().await?;
        tracing::info!(folder = %self.folder.display(), "campaign closed");
        Ok(())
    }
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
