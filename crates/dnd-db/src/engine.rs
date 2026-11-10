//! The authoritative intent pipeline over a campaign file, shared by local play (desktop) and the
//! session server (ADR-0001, ADR-0022).
//!
//! Persistence is write-through (ADR-0014): every accepted batch is committed before the in-memory
//! state changes and before anyone is told about it. Time comes in as `now_ms` (ADR-0008).

use std::collections::VecDeque;

use dnd_game_core::{
    Actor, ExecutionTrace, GameRng, GameState, IntentWindow, Recall, RecentIntents, apply_traced,
    evolve_all,
};
use dnd_protocol::{
    CHAT_SNAPSHOT_LEN, ChatMessageDto, ChatMessageId, Event, EventBatch, IntentEnvelope,
    IntentOutcome, JournalEntryDto, JournalEntryId, PayloadHash, RejectReason, Seq, Snapshot,
    UserId,
};

use crate::campaign::{BatchRecord, CampaignDb};
use crate::error::DbError;

/// Errors of [`CampaignEngine`]. Rejections by the rules are outcomes, not errors.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Db(#[from] DbError),
    /// The event counter reached the largest integer JavaScript represents exactly.
    #[error("event sequence reached 2^53-1")]
    SeqExhausted,
}

/// An open campaign file and its authoritative state.
pub struct CampaignEngine {
    db: CampaignDb,
    state: GameState,
    next_seq: Seq,
    /// Working randomness (ADR-0050): the OS CSPRNG in play, a seeded generator in tests.
    rng: Box<dyn GameRng + Send + Sync>,
    /// Outcomes of recent intents by `intent_id` (ADR-0015).
    recent: RecentIntents<IntentOutcome>,
    /// The last `CHAT_SNAPSHOT_LEN` chat messages for snapshots (older ones are paged from the DB).
    chat: VecDeque<ChatMessageDto>,
}

impl std::fmt::Debug for CampaignEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CampaignEngine")
            .field("next_seq", &self.next_seq)
            .field("chat", &self.chat.len())
            .finish_non_exhaustive()
    }
}

impl CampaignEngine {
    /// Loads the state, the idempotency window and the recent chat from `db`. On failure the
    /// database is closed before returning, so the file is released at once (Windows keeps it
    /// locked otherwise).
    pub async fn load(
        db: CampaignDb,
        window: IntentWindow,
        rng: Box<dyn GameRng + Send + Sync>,
        now_ms: i64,
    ) -> Result<Self, DbError> {
        match Self::read(&db, window, now_ms).await {
            Ok((state, last_seq, recent, chat)) => Ok(Self {
                db,
                state,
                next_seq: last_seq.next(),
                rng,
                recent,
                chat: chat.into(),
            }),
            Err(e) => {
                db.close().await.ok();
                Err(e)
            }
        }
    }

    async fn read(
        db: &CampaignDb,
        window: IntentWindow,
        now_ms: i64,
    ) -> Result<
        (
            GameState,
            Seq,
            RecentIntents<IntentOutcome>,
            Vec<ChatMessageDto>,
        ),
        DbError,
    > {
        let (state, last_seq) = db.load_state().await?;
        // Accepted intents survive restarts through `event_log`; rejections are remembered per run.
        let mut recent = RecentIntents::new(window);
        for r in db
            .recent_intents(window.max_count, window.since_ms(now_ms))
            .await?
        {
            recent.insert(
                r.intent_id,
                r.intent_hash,
                IntentOutcome::Accepted { seq: r.last_seq },
                r.at_ms,
            );
        }
        recent.prune(now_ms);
        let chat = db.chat_history(None, CHAT_SNAPSHOT_LEN).await?;
        Ok((state, last_seq, recent, chat))
    }

    /// Opens the campaign without its rules (ADR-0057): plugins, triggers and the saved rules
    /// process are dropped *in memory only*. The file is untouched, so a normal reopen restores
    /// them; edits made in this mode are ordinary events and stay.
    ///
    /// Intended for recovery: a broken plugin cannot be uninstalled or a sheet fixed if opening
    /// the campaign executes the same rules again.
    pub fn enter_safe_mode(&mut self) {
        self.state.plugins.clear();
        self.state.system_plugin = None;
        self.state.triggers.clear();
        self.state.plugin_runtime = Default::default();
    }

    #[must_use]
    pub fn state(&self) -> &GameState {
        &self.state
    }

    #[must_use]
    pub fn db(&self) -> &CampaignDb {
        &self.db
    }

    /// Sequence number of the last committed event (0 = none yet).
    #[must_use]
    pub fn last_seq(&self) -> Seq {
        Seq(self.next_seq.0 - 1)
    }

    /// The full (unfiltered) state with the recent chat.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.state.to_snapshot(self.last_seq());
        snapshot.chat = self.chat.iter().cloned().collect();
        snapshot
    }

    /// The recent chat, oldest first (what snapshots carry).
    pub fn recent_chat(&self) -> impl Iterator<Item = &ChatMessageDto> {
        self.chat.iter()
    }

    /// Older chat messages, oldest first, before message `before` (or the newest).
    pub async fn chat_history(
        &self,
        before: Option<ChatMessageId>,
        limit: usize,
    ) -> Result<Vec<ChatMessageDto>, DbError> {
        self.db
            .chat_history(before, limit.min(CHAT_SNAPSHOT_LEN))
            .await
    }

    /// Full text of a journal entry (snapshots carry only the index).
    #[must_use]
    pub fn journal_entry(&self, id: &JournalEntryId) -> Option<JournalEntryDto> {
        self.state.journal.get(id).cloned()
    }

    /// Runs the intent pipeline for `actor`: `apply` → commit the batch (`event_log` + projection,
    /// one transaction) → `evolve`. Memory changes only after the commit, so a storage failure
    /// leaves memory, disk and the sequence counter untouched; the caller then broadcasts the batch.
    ///
    /// Idempotent by `intent_id` (ADR-0015): a repeat returns the original outcome and no batch;
    /// the same id with a different payload is rejected with `IntentIdReused`.
    pub async fn submit(
        &mut self,
        envelope: &IntentEnvelope,
        actor: &Actor,
        now_ms: i64,
    ) -> Result<(IntentOutcome, Option<EventBatch>), EngineError> {
        self.submit_recording(envelope, actor, now_ms, false, Ok(()))
            .await
    }

    /// Adapter checks (e.g. an uploaded package) can reject a new intent, never bypass core rules.
    /// Cached/durable receipts take precedence so external changes cannot alter a retry's result.
    pub async fn submit_with_precondition(
        &mut self,
        envelope: &IntentEnvelope,
        actor: &Actor,
        now_ms: i64,
        precondition: Result<(), RejectReason>,
    ) -> Result<(IntentOutcome, Option<EventBatch>), EngineError> {
        self.submit_recording(envelope, actor, now_ms, false, precondition)
            .await
    }

    /// Same pipeline with the replay operation committed in the event transaction.
    pub async fn submit_offline(
        &mut self,
        envelope: &IntentEnvelope,
        actor: &Actor,
        now_ms: i64,
    ) -> Result<(IntentOutcome, Option<EventBatch>), EngineError> {
        self.submit_recording(envelope, actor, now_ms, true, Ok(()))
            .await
    }

    /// Look up an original outcome before adapter I/O, preserving the payload-hash check.
    pub async fn recall_intent(
        &mut self,
        envelope: &IntentEnvelope,
        now_ms: i64,
    ) -> Result<Option<IntentOutcome>, EngineError> {
        let hash = envelope.payload_hash();
        match self.recent.recall(&envelope.intent_id, &hash, now_ms) {
            Recall::New => {}
            Recall::Repeat(outcome) => {
                tracing::debug!(intent_id = %envelope.intent_id, "duplicate intent");
                return Ok(Some(outcome.clone()));
            }
            Recall::Reused => {
                tracing::warn!(intent_id = %envelope.intent_id, "intent_id reused with other data");
                let reason = RejectReason::IntentIdReused;
                return Ok(Some(IntentOutcome::Rejected { reason }));
            }
        }
        if let Some(receipt) = self.db.accepted_intent(envelope.intent_id).await? {
            let accepted = IntentOutcome::Accepted {
                seq: receipt.last_seq,
            };
            self.recent.insert(
                envelope.intent_id,
                receipt.intent_hash,
                accepted.clone(),
                now_ms,
            );
            if receipt.intent_hash.is_some_and(|stored| stored != hash) {
                return Ok(Some(IntentOutcome::Rejected {
                    reason: RejectReason::IntentIdReused,
                }));
            }
            return Ok(Some(accepted));
        }
        Ok(None)
    }

    async fn submit_recording(
        &mut self,
        envelope: &IntentEnvelope,
        actor: &Actor,
        now_ms: i64,
        offline: bool,
        precondition: Result<(), RejectReason>,
    ) -> Result<(IntentOutcome, Option<EventBatch>), EngineError> {
        let hash = envelope.payload_hash();
        if let Some(outcome) = self.recall_intent(envelope, now_ms).await? {
            return Ok((outcome, None));
        }
        // The core has no clock: the server stamps the arrival time, and the core reads
        // `ts_ms` only from there (deadlines of real-time token effects, ADR-0024).
        let stamped = IntentEnvelope {
            ts_ms: now_ms,
            ..envelope.clone()
        };
        let (events, trace) = match precondition
            .and_then(|()| apply_traced(&self.state, &stamped, actor, &mut *self.rng))
        {
            Ok(result) => result,
            Err(reason) => {
                tracing::debug!(
                    intent = envelope.intent.kind(),
                    code = reason.code(),
                    "intent rejected"
                );
                let outcome = IntentOutcome::Rejected { reason };
                self.recent
                    .insert(envelope.intent_id, Some(hash), outcome.clone(), now_ms);
                return Ok((outcome, None));
            }
        };
        log_trace(envelope.intent.kind(), &trace);
        let batch = EventBatch {
            seq: self.next_seq,
            caused_by: Some(envelope.intent_id),
            events,
        };
        self.commit(
            batch.clone(),
            Some(hash),
            Some(actor.user_id),
            now_ms,
            offline.then_some(envelope),
            None,
        )
        .await?;
        let outcome = IntentOutcome::Accepted {
            seq: batch.last_seq(),
        };
        self.recent
            .insert(envelope.intent_id, Some(hash), outcome.clone(), now_ms);
        Ok((outcome, Some(batch)))
    }

    /// Rebuild the provisional local view without changing the envelopes sent to the server.
    pub async fn materialize_pending(
        &mut self,
        actor: &Actor,
        now_ms: i64,
    ) -> Result<(), EngineError> {
        for operation in self.db.pending_ops().await? {
            if operation.projected {
                continue;
            }
            let original = operation.envelope;
            if let Some(receipt) = self.db.accepted_intent(original.intent_id).await? {
                if receipt
                    .intent_hash
                    .is_none_or(|hash| hash == original.payload_hash())
                {
                    self.db.mark_pending_projected(operation.id).await?;
                }
                continue;
            }
            let mut preview = original.clone();
            preview.ts_ms = now_ms;
            if preview.intent.version_policy() == dnd_protocol::VersionPolicy::Checked {
                let Ok(version) = dnd_game_core::current_version(&self.state, &preview.intent)
                else {
                    continue;
                };
                preview.expected_version = Some(version);
            }
            let events = match apply_traced(&self.state, &preview, actor, &mut *self.rng) {
                Ok((events, trace)) => {
                    log_trace(preview.intent.kind(), &trace);
                    events
                }
                Err(reason) => {
                    tracing::warn!(
                        intent = preview.intent.kind(),
                        code = reason.code(),
                        "offline edit cannot be projected onto the current base"
                    );
                    continue;
                }
            };
            let batch = EventBatch {
                seq: self.next_seq,
                caused_by: Some(original.intent_id),
                events,
            };
            self.commit(
                batch,
                Some(original.payload_hash()),
                Some(actor.user_id),
                now_ms,
                None,
                Some(operation.id),
            )
            .await?;
        }
        Ok(())
    }

    /// Removes real-time effects whose deadline has passed (ADR-0056): the internal command of the
    /// shared pipeline that adapters schedule. Writes before returning the batch, so the change is
    /// durable before anyone is told; `None` means nothing was due.
    ///
    /// Safe to call repeatedly and concurrently with user intents: an effect that is already gone
    /// produces no event, and a command that runs first simply leaves this call nothing to do.
    pub async fn expire_due(&mut self, now_ms: i64) -> Result<Option<EventBatch>, EngineError> {
        let events = dnd_game_core::expire_due_effects(self.state(), now_ms);
        if events.is_empty() {
            return Ok(None);
        }
        self.commit_events(events, None, now_ms).await.map(Some)
    }

    /// The next instant a real-time effect must be removed (ADR-0056).
    #[must_use]
    pub fn next_deadline(&self) -> Option<i64> {
        dnd_game_core::next_real_deadline(self.state())
    }

    /// Commits events that no intent caused (the server's own, e.g. a participant joining) the
    /// same write-through way. Returns the batch to broadcast.
    pub async fn commit_events(
        &mut self,
        events: Vec<Event>,
        actor: Option<UserId>,
        now_ms: i64,
    ) -> Result<EventBatch, EngineError> {
        let batch = EventBatch {
            seq: self.next_seq,
            caused_by: None,
            events,
        };
        self.commit(batch.clone(), None, actor, now_ms, None, None)
            .await?;
        Ok(batch)
    }

    async fn commit(
        &mut self,
        batch: EventBatch,
        intent_hash: Option<PayloadHash>,
        actor: Option<UserId>,
        at_ms: i64,
        pending: Option<&IntentEnvelope>,
        projected_id: Option<i64>,
    ) -> Result<(), EngineError> {
        let last = batch.last_seq();
        if !dnd_protocol::is_js_safe(last.0) {
            return Err(EngineError::SeqExhausted);
        }
        self.db
            .project_with_pending(
                &[BatchRecord {
                    batch: batch.clone(),
                    intent_hash,
                    actor,
                    at_ms,
                }],
                pending,
                projected_id,
            )
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
        Ok(())
    }

    /// Moves the WAL into the main file (periodic housekeeping, not needed for durability).
    pub async fn checkpoint(&self) -> Result<(), DbError> {
        self.db.checkpoint().await
    }

    /// Checkpoints and closes the database.
    pub async fn close(self) -> Result<(), DbError> {
        self.db.close().await
    }
}

/// Reports an execution that had to cancel waiting states (ADR-0052). Hitting the limit is a
/// rule bug, not a user error: the batch is still committed, but the event is worth investigating.
fn log_trace(intent: &'static str, trace: &ExecutionTrace) {
    if trace.limit_reached {
        tracing::warn!(
            intent,
            rounds = trace.rounds,
            stages = ?trace.stages,
            "resolution limit reached; waiting states were cancelled"
        );
    } else {
        tracing::trace!(intent, rounds = trace.rounds, stages = ?trace.stages, "intent resolved");
    }
}
