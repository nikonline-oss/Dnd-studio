//! The in-process "server" for local play (ADR-0001): the same intent pipeline as the session
//! server, backed by the campaign's SQLite file. Independent of Tauri so it can be tested directly.
//!
//! Persistence is write-through (CLAUDE.md §10, ADR-0014): every accepted batch is committed before
//! the in-memory state changes and before anyone is told about it.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dnd_db::{BatchRecord, CampaignDb, LocalUser, list_backups};
use dnd_game_core::{
    Actor, GameState, IntentWindow, Recall, RecentIntents, SplitMix64, apply, evolve_all,
};
use dnd_protocol::{
    CHAT_SNAPSHOT_LEN, ChatMessageDto, ChatMessageId, Event, EventBatch, IntentEnvelope,
    IntentOutcome, JournalEntryDto, JournalEntryId, RejectReason, Role, Seq, Snapshot, UserId,
};
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
    /// The last `CHAT_SNAPSHOT_LEN` chat messages for snapshots (older ones are paged from the DB).
    chat: VecDeque<ChatMessageDto>,
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
        match Self::load(&db, window).await {
            Ok((user, state, last_seq, recent, chat)) => Ok(Self {
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
                chat: chat.into(),
            }),
            Err(e) => {
                db.close().await.ok();
                Err(e)
            }
        }
    }

    async fn load(
        db: &CampaignDb,
        window: IntentWindow,
    ) -> Result<
        (
            LocalUser,
            GameState,
            Seq,
            RecentIntents<IntentOutcome>,
            Vec<ChatMessageDto>,
        ),
        IpcError,
    > {
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
                r.intent_hash,
                IntentOutcome::Accepted { seq: r.last_seq },
                r.at_ms,
            );
        }
        recent.prune(now);
        let chat = db.chat_history(None, CHAT_SNAPSHOT_LEN).await?;
        Ok((user, state, last_seq, recent, chat))
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
        let mut snapshot = self.state.to_snapshot(Seq(self.next_seq.0 - 1));
        snapshot.chat = self.chat.iter().cloned().collect();
        snapshot
    }

    /// Older chat messages, oldest first, before message `before` (or the newest).
    pub async fn chat_history(
        &self,
        before: Option<ChatMessageId>,
        limit: usize,
    ) -> Result<Vec<ChatMessageDto>, IpcError> {
        Ok(self
            .db
            .chat_history(before, limit.min(CHAT_SNAPSHOT_LEN))
            .await?)
    }

    /// Full text of a journal entry (the snapshot carries only the index).
    #[must_use]
    pub fn journal_entry(&self, id: &JournalEntryId) -> Option<JournalEntryDto> {
        self.state.journal.get(id).cloned()
    }

    /// Runs the intent pipeline write-through (ADR-0014): `apply` → commit the batch (`event_log` +
    /// projection, one transaction) → `evolve`. Memory changes only after the commit, so a storage
    /// failure leaves memory, disk and `next_seq` untouched; the caller then broadcasts the batch.
    ///
    /// Idempotent by `intent_id` (ADR-0015): a repeat returns the original outcome and no batch;
    /// the same id with a different payload is rejected with `IntentIdReused`.
    pub async fn submit(
        &mut self,
        envelope: &IntentEnvelope,
    ) -> Result<(IntentOutcome, Option<EventBatch>), IpcError> {
        let hash = envelope.payload_hash();
        match self.recent.recall(&envelope.intent_id, &hash, now_ms()) {
            Recall::New => {}
            Recall::Repeat(outcome) => {
                tracing::debug!(intent_id = %envelope.intent_id, "duplicate intent");
                return Ok((outcome.clone(), None));
            }
            Recall::Reused => {
                tracing::warn!(intent_id = %envelope.intent_id, "intent_id reused with other data");
                let reason = RejectReason::IntentIdReused;
                return Ok((IntentOutcome::Rejected { reason }, None));
            }
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
                    .insert(envelope.intent_id, Some(hash), outcome.clone(), now_ms());
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
        let at_ms = now_ms();
        self.db
            .project(&[BatchRecord {
                batch: batch.clone(),
                intent_hash: Some(hash),
                actor: Some(self.actor.user_id),
                at_ms,
            }])
            .await?;
        evolve_all(&mut self.state, &batch.events);
        for event in &batch.events {
            if let Event::ChatMessagePosted { message } = event {
                self.chat.push_back(ChatMessageDto {
                    at_ms: Some(at_ms),
                    ..message.clone()
                });
                while self.chat.len() > CHAT_SNAPSHOT_LEN {
                    self.chat.pop_front();
                }
            }
        }
        self.next_seq = last.next();
        let outcome = IntentOutcome::Accepted { seq: last };
        self.recent
            .insert(envelope.intent_id, Some(hash), outcome.clone(), now_ms());
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
