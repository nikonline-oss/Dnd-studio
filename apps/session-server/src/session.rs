//! One running session: a task that owns the campaign's [`CampaignEngine`], takes commands over a
//! channel and broadcasts every committed batch — in full and as players see it (ADR-0022).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Context as _;
use dnd_db::{
    ARCHIVE_FORMAT, ArchiveManifest, CampaignDb, CampaignEngine, LocalUser, StoredEvent,
    write_archive,
};
use dnd_game_core::{
    Actor, ForPlayers, GameState, IntentWindow, PlayersSee, evolve, filter_for_players, join,
    may_change_visibility, sees_everything, snapshot_for,
};
use dnd_protocol::{
    AssetUsage, ChatMessageDto, ChatMessageId, Event, EventBatch, Intent, IntentEnvelope,
    IntentOutcome, JournalEntryDto, JournalEntryId, JournalVisibility, RejectReason, Role, Seq,
    SessionId, Snapshot, UserDto, UserId,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::now_ms;

/// Close reason: the session is over for good (stopped by the GM, expired).
pub const SESSION_CLOSED: &str = "session_closed";
/// Close reason: the server is going down; clients reconnect.
pub const SERVER_RESTART: &str = "server_restart";

/// How far back a reconnect may be served from the event log instead of a snapshot (ADR-0053).
/// The checkpoint that makes this cheap is refreshed only once the window has grown past this
/// many events, so a clone is paid once per window rather than once per command.
pub const DELTA_WINDOW: i64 = 4096;

/// How often the session looks for a new real-time deadline when none is scheduled (ADR-0056).
pub const DEADLINE_POLL: Duration = Duration::from_secs(1);

/// Beyond this the replayed delta is larger than the snapshot it replaces (measured, see
/// `docs/perf-baseline.md`), so the recipient gets the snapshot instead (ADR-0053).
pub const MAX_DELTA_BYTES: usize = 128 * 1024;

/// A committed batch for everyone: GM roles take `full`, the others `players`.
#[derive(Debug)]
pub struct Broadcast {
    pub full: EventBatch,
    pub players: ForPlayers,
    pub request_owners: std::collections::BTreeMap<dnd_protocol::RollRequestId, Option<UserId>>,
}

enum Command {
    RollStatistics {
        role: Role,
        reply: oneshot::Sender<anyhow::Result<Vec<dnd_protocol::RollStatisticsDto>>>,
    },
    Submit {
        envelope: Box<IntentEnvelope>,
        actor: Actor,
        reply: oneshot::Sender<anyhow::Result<IntentOutcome>>,
    },
    Snapshot {
        role: Role,
        user: Option<UserId>,
        reply: oneshot::Sender<Snapshot>,
    },
    /// Events after `from`, filtered for one recipient (ADR-0053). `None` means the history is
    /// unavailable or the recipient's access changed: send a snapshot instead.
    Delta {
        from: Seq,
        role: Role,
        user: Option<UserId>,
        reply: oneshot::Sender<Option<EventBatch>>,
    },
    JournalEntry {
        id: JournalEntryId,
        role: Role,
        reply: oneshot::Sender<Option<JournalEntryDto>>,
    },
    ChatHistory {
        before: Option<ChatMessageId>,
        limit: usize,
        role: Role,
        reply: oneshot::Sender<anyhow::Result<Vec<ChatMessageDto>>>,
    },
    Join {
        user: UserDto,
        reply: oneshot::Sender<anyhow::Result<Result<(), RejectReason>>>,
    },
    AssetVisible {
        sha256: String,
        role: Role,
        reply: oneshot::Sender<bool>,
    },
    PluginVisible {
        sha256: String,
        reply: oneshot::Sender<bool>,
    },
    Export {
        out: PathBuf,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    Stop {
        reason: &'static str,
        reply: oneshot::Sender<()>,
    },
}

/// Commands to a session task. Cheap to clone.
#[derive(Clone, Debug)]
pub struct SessionHandle {
    pub id: SessionId,
    /// The campaign's GM (the user who created it), whom the GM key logs in as.
    pub gm: UserDto,
    pub name: String,
    tx: mpsc::Sender<Command>,
    events: broadcast::Sender<Arc<Broadcast>>,
    /// Why the session task ended, once it has.
    closed: watch::Receiver<Option<&'static str>>,
    connections: Arc<AtomicUsize>,
    last_activity: Arc<AtomicI64>,
}

/// The session task is gone (stopped or failed).
#[derive(Debug, thiserror::Error)]
#[error("the session has ended")]
pub struct SessionGone;

impl SessionHandle {
    /// Loads the campaign file and starts the session task.
    pub async fn start(id: SessionId, campaign: &Path) -> anyhow::Result<Self> {
        let (db, _) = CampaignDb::open(campaign, now_ms()).await?;
        let gm = match db.local_user().await {
            Ok(Some(LocalUser { id, name, role })) => UserDto { id, name, role },
            Ok(None) => {
                db.close().await.ok();
                anyhow::bail!("the campaign has no GM user");
            }
            Err(e) => {
                db.close().await.ok();
                return Err(e.into());
            }
        };
        let rng = dnd_db::OsRng::new().context("the OS random source is unavailable")?;
        let engine =
            CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(rng), now_ms()).await?;
        let name = engine.state().campaign.name.clone();
        let assets = campaign
            .parent()
            .map(|p| p.join("assets"))
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel(256);
        let (events, _) = broadcast::channel(256);
        let (closing, closed) = watch::channel(None);
        let handle = Self {
            id,
            gm,
            name,
            tx,
            events: events.clone(),
            closed,
            connections: Arc::new(AtomicUsize::new(0)),
            last_activity: Arc::new(AtomicI64::new(now_ms())),
        };
        let last_activity = handle.last_activity.clone();
        tokio::spawn(async move {
            let reason = run(engine, rx, events, last_activity, assets).await;
            let _ = closing.send(Some(reason));
        });
        tracing::info!(session = %id, "session loaded");
        Ok(handle)
    }

    async fn ask<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> Command,
    ) -> Result<T, SessionGone> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(command(reply))
            .await
            .map_err(|_| SessionGone)?;
        rx.await.map_err(|_| SessionGone)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Broadcast>> {
        self.events.subscribe()
    }

    /// Resolves once the session has ended, with the reason for its connections.
    pub async fn ended(&self) -> &'static str {
        let mut closed = self.closed.clone();
        let _ = closed.wait_for(Option::is_some).await;
        self.end_reason()
    }

    /// Why the session ended (a task that vanished without saying is treated as a restart).
    #[must_use]
    pub fn end_reason(&self) -> &'static str {
        self.closed.borrow().unwrap_or(SERVER_RESTART)
    }

    pub async fn submit(
        &self,
        envelope: IntentEnvelope,
        actor: Actor,
    ) -> Result<anyhow::Result<IntentOutcome>, SessionGone> {
        self.ask(|reply| Command::Submit {
            envelope: Box::new(envelope),
            actor,
            reply,
        })
        .await
    }

    /// What `role` sees now.
    pub async fn snapshot(&self, role: Role) -> Result<Snapshot, SessionGone> {
        self.ask(|reply| Command::Snapshot {
            role,
            user: None,
            reply,
        })
        .await
    }

    pub async fn snapshot_for_user(
        &self,
        role: Role,
        user: UserId,
    ) -> Result<Snapshot, SessionGone> {
        self.ask(|reply| Command::Snapshot {
            role,
            user: Some(user),
            reply,
        })
        .await
    }

    /// What the recipient missed since `from`, or `None` when a snapshot is required (ADR-0053).
    pub async fn delta(
        &self,
        from: Seq,
        role: Role,
        user: Option<UserId>,
    ) -> Result<Option<EventBatch>, SessionGone> {
        self.ask(|reply| Command::Delta {
            from,
            role,
            user,
            reply,
        })
        .await
    }

    /// The entry's text if `role` may read it.
    pub async fn journal_entry(
        &self,
        id: JournalEntryId,
        role: Role,
    ) -> Result<Option<JournalEntryDto>, SessionGone> {
        self.ask(|reply| Command::JournalEntry { id, role, reply })
            .await
    }

    pub async fn roll_statistics(
        &self,
        role: Role,
    ) -> Result<anyhow::Result<Vec<dnd_protocol::RollStatisticsDto>>, SessionGone> {
        self.ask(|reply| Command::RollStatistics { role, reply })
            .await
    }

    pub async fn chat_history(
        &self,
        before: Option<ChatMessageId>,
        limit: usize,
        role: Role,
    ) -> Result<anyhow::Result<Vec<ChatMessageDto>>, SessionGone> {
        self.ask(|reply| Command::ChatHistory {
            before,
            limit,
            role,
            reply,
        })
        .await
    }

    /// Makes `user` a user of the campaign (no event when nothing changes).
    pub async fn join(
        &self,
        user: UserDto,
    ) -> Result<anyhow::Result<Result<(), RejectReason>>, SessionGone> {
        self.ask(|reply| Command::Join { user, reply }).await
    }

    /// Whether `role` may download the served file with this hash.
    pub async fn asset_visible(&self, sha256: String, role: Role) -> Result<bool, SessionGone> {
        self.ask(|reply| Command::AssetVisible {
            sha256,
            role,
            reply,
        })
        .await
    }

    pub async fn plugin_visible(&self, sha256: String) -> Result<bool, SessionGone> {
        self.ask(|reply| Command::PluginVisible { sha256, reply })
            .await
    }

    /// Writes the campaign archive (database + served assets) to `out`.
    pub async fn export(&self, out: PathBuf) -> Result<anyhow::Result<()>, SessionGone> {
        self.ask(|reply| Command::Export { out, reply }).await
    }

    /// Closes the campaign file; connections close with `reason`.
    pub async fn stop(&self, reason: &'static str) {
        self.ask(|reply| Command::Stop { reason, reply }).await.ok();
    }

    /// Counts a WS connection for as long as the guard lives.
    pub fn connected(&self) -> ConnectionGuard {
        self.connections.fetch_add(1, Ordering::SeqCst);
        self.touch();
        ConnectionGuard {
            connections: self.connections.clone(),
            last_activity: self.last_activity.clone(),
        }
    }

    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    #[must_use]
    pub fn last_activity(&self) -> i64 {
        self.last_activity.load(Ordering::SeqCst)
    }

    pub fn touch(&self) {
        self.last_activity.fetch_max(now_ms(), Ordering::SeqCst);
    }
}

/// See [`SessionHandle::connected`].
#[derive(Debug)]
pub struct ConnectionGuard {
    connections: Arc<AtomicUsize>,
    last_activity: Arc<AtomicI64>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.connections.fetch_sub(1, Ordering::SeqCst);
        self.last_activity.fetch_max(now_ms(), Ordering::SeqCst);
    }
}

/// Intents whose batches never change visibility (ADR-0022): no copy of the state is kept for
/// them. If one still produced such a batch, players get a fresh snapshot instead.
fn keeps_visibility(intent: &Intent) -> bool {
    matches!(
        intent,
        Intent::MoveToken { .. }
            | Intent::SendMessage { .. }
            | Intent::RollDice { .. }
            | Intent::ApplyFog { .. }
    )
}

async fn run(
    mut engine: CampaignEngine,
    mut rx: mpsc::Receiver<Command>,
    events: broadcast::Sender<Arc<Broadcast>>,
    last_activity: Arc<AtomicI64>,
    assets: PathBuf,
) -> &'static str {
    // A state old enough to replay a reconnect from (ADR-0053); refreshed only once the delta
    // window has expired, so its clone is amortized over thousands of commands.
    let mut checkpoint: Option<(Seq, GameState)> =
        Some((engine.last_seq(), engine.state().clone()));
    loop {
        // Wake up for the nearest real-time effect deadline (ADR-0056); with nothing scheduled
        // poll once a second so a newly created effect is noticed promptly.
        let wait = match engine.next_deadline() {
            Some(deadline) => {
                let remaining = deadline.saturating_sub(now_ms()).max(0);
                Duration::from_millis(u64::try_from(remaining).unwrap_or(u64::MAX))
                    .min(DEADLINE_POLL)
            }
            None => DEADLINE_POLL,
        };
        let sleep = tokio::time::sleep(wait);
        tokio::pin!(sleep);
        let command = tokio::select! {
            command = rx.recv() => command,
            () = &mut sleep => {
                expire_and_broadcast(&mut engine, &events, &mut checkpoint).await;
                continue;
            }
        };
        let Some(command) = command else { break };
        match command {
            Command::Submit {
                envelope,
                actor,
                reply,
            } => {
                last_activity.fetch_max(now_ms(), Ordering::SeqCst);
                if matches!(&envelope.intent, Intent::InstallPlugin { .. }) {
                    match engine.recall_intent(&envelope, now_ms()).await {
                        Ok(Some(outcome)) => {
                            let _ = reply.send(Ok(outcome));
                            continue;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let _ = reply.send(Err(error.into()));
                            continue;
                        }
                    }
                }
                let before = (!keeps_visibility(&envelope.intent)).then(|| engine.state().clone());
                let precondition =
                    match plugin_precondition(&engine, &assets, &envelope, actor.role).await {
                        Ok(result) => result,
                        Err(error) => {
                            let _ = reply.send(Err(error));
                            continue;
                        }
                    };
                let result = engine
                    .submit_with_precondition(&envelope, &actor, now_ms(), precondition)
                    .await;
                refresh_checkpoint(&engine, &mut checkpoint);
                let outcome = match result {
                    Ok((outcome, Some(batch))) => {
                        let players = match &before {
                            Some(before) => filter_for_players(before, engine.state(), &batch),
                            None if !may_change_visibility(&batch) => {
                                filter_for_players(engine.state(), engine.state(), &batch)
                            }
                            None => ForPlayers::Resnapshot,
                        };
                        let request_owners = batch
                            .events
                            .iter()
                            .filter_map(|event| match event {
                                dnd_protocol::Event::RollRequested { request }
                                | dnd_protocol::Event::RollRequestUpdated { request } => Some((
                                    request.id,
                                    engine
                                        .state()
                                        .characters
                                        .get(&request.character)
                                        .and_then(|c| c.owner),
                                )),
                                _ => None,
                            })
                            .collect();
                        // No receivers is fine: nobody is connected.
                        let _ = events.send(Arc::new(Broadcast {
                            full: batch,
                            players,
                            request_owners,
                        }));
                        Ok(outcome)
                    }
                    Ok((outcome, None)) => Ok(outcome),
                    Err(e) => Err(e.into()),
                };
                let _ = reply.send(outcome);
            }
            Command::Snapshot { role, user, reply } => {
                let snapshot = if let Some(user) = user {
                    dnd_game_core::snapshot_for_user(
                        role,
                        user,
                        engine.state(),
                        engine.last_seq(),
                        engine.recent_chat(),
                    )
                } else {
                    snapshot_for(
                        role,
                        engine.state(),
                        engine.last_seq(),
                        engine.recent_chat(),
                    )
                };
                let _ = reply.send(snapshot);
            }
            Command::Delta {
                from,
                role,
                user,
                reply,
            } => {
                let delta = delta(&engine, checkpoint.as_ref(), from, role, user).await;
                let _ = reply.send(delta);
            }
            Command::JournalEntry { id, role, reply } => {
                let entry = engine.journal_entry(&id).filter(|e| {
                    sees_everything(role) || e.visibility == JournalVisibility::Players
                });
                let _ = reply.send(entry);
            }
            Command::RollStatistics { role, reply } => {
                let result = engine.db().roll_statistics(role).await.map_err(Into::into);
                let _ = reply.send(result);
            }
            Command::ChatHistory {
                before,
                limit,
                role,
                reply,
            } => {
                let result = engine
                    .db()
                    .chat_history_for(before, limit, role)
                    .await
                    .map_err(Into::into);
                let _ = reply.send(result);
            }
            Command::Join { user, reply } => {
                let result = match join(engine.state(), user.id, &user.name, user.role) {
                    Err(reason) => Ok(Err(reason)),
                    Ok(joined) if joined.is_empty() => Ok(Ok(())),
                    Ok(joined) => match engine.commit_events(joined, Some(user.id), now_ms()).await
                    {
                        Ok(batch) => {
                            refresh_checkpoint(&engine, &mut checkpoint);
                            let players =
                                filter_for_players(engine.state(), engine.state(), &batch);
                            let _ = events.send(Arc::new(Broadcast {
                                full: batch,
                                players,
                                request_owners: Default::default(),
                            }));
                            Ok(Ok(()))
                        }
                        Err(e) => Err(e.into()),
                    },
                };
                let _ = reply.send(result);
            }
            Command::AssetVisible {
                sha256,
                role,
                reply,
            } => {
                let state = engine.state();
                let ids = if sees_everything(role) {
                    None
                } else {
                    Some(PlayersSee::of(state).assets)
                };
                // A sound's hash is the right to fetch it: players learn it only from a
                // `SoundPlayed` event (ADR-0026).
                let sound = state
                    .assets
                    .values()
                    .any(|a| a.usage == AssetUsage::Audio && a.sha256 == sha256)
                    || state
                        .playlists
                        .values()
                        .any(|p| p.tracks.iter().any(|t| t.sha256 == sha256));
                let visible = sound
                    || state
                        .assets
                        .values()
                        .filter(|a| ids.as_ref().is_none_or(|ids| ids.contains(&a.id)))
                        .any(|a| {
                            a.thumbnail_sha256 == sha256
                                || a.display_sha256.as_deref() == Some(sha256.as_str())
                                || a.tiles.iter().any(|t| t.sha256 == sha256)
                        });
                let _ = reply.send(visible);
            }
            Command::Export { out, reply } => {
                let _ = reply.send(export(&engine, &assets, &out).await);
            }
            Command::PluginVisible { sha256, reply } => {
                let installed = engine
                    .state()
                    .plugins
                    .values()
                    .any(|p| p.archive_sha256 == sha256);
                let _ = reply.send(installed);
            }
            Command::Stop { reason, reply } => {
                if let Err(e) = engine.close().await {
                    tracing::error!(error = %e, "closing a session failed");
                }
                let _ = reply.send(());
                return reason;
            }
        }
    }
    // Every handle is gone without a stop (server shutting down).
    engine.close().await.ok();
    SERVER_RESTART
}

/// Package I/O stays in the adapter. The engine records validation rejections after checking
/// receipts, while all permission/rule checks still run in the deterministic core.
async fn plugin_precondition(
    engine: &CampaignEngine,
    assets: &Path,
    envelope: &IntentEnvelope,
    role: Role,
) -> anyhow::Result<Result<(), RejectReason>> {
    let Intent::InstallPlugin { plugin } = &envelope.intent else {
        return Ok(Ok(()));
    };
    if !sees_everything(role) {
        return Ok(Ok(()));
    }
    if !dnd_assets::is_sha256_hex(&plugin.archive_sha256) {
        return Ok(Err(RejectReason::invalid("archive_sha256")));
    }
    let expected = plugin.as_ref().clone();
    let versions: Vec<_> = engine
        .state()
        .plugins
        .values()
        .map(|p| (p.id.clone(), p.version.clone()))
        .collect();
    let path = assets
        .join("plugins")
        .join(format!("{}.dndplugin", plugin.archive_sha256));
    tokio::task::spawn_blocking(move || -> anyhow::Result<Result<(), RejectReason>> {
        let invalid = || Ok(Err(RejectReason::invalid("plugin.package")));
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return invalid(),
            Err(error) => return Err(error.into()),
        };
        if metadata.len() > dnd_plugin::PackageLimits::DEFAULT.max_package_bytes {
            return invalid();
        }
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return invalid(),
            Err(error) => return Err(error.into()),
        };
        let package = match dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default()) {
            Ok(package) => package,
            Err(_) => return invalid(),
        };
        if package.to_dto() != expected {
            return invalid();
        }
        let app = dnd_plugin::semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        if let Err(error) = dnd_plugin::check_app_compat(&package.manifest, &app).and_then(|()| {
            dnd_plugin::check_dependency_versions(
                &package.manifest,
                versions
                    .iter()
                    .map(|(id, version)| (id.as_str(), version.as_str())),
            )
        }) {
            return Ok(Err(RejectReason::invalid(&format!(
                "plugin.{}",
                error.code()
            ))));
        }
        Ok(Ok(()))
    })
    .await?
}

async fn export(engine: &CampaignEngine, assets: &Path, out: &Path) -> anyhow::Result<()> {
    engine.checkpoint().await?;
    let copy = out.with_extension("sqlite");
    engine.db().copy_to(&copy).await?;
    let manifest = ArchiveManifest {
        format: ARCHIVE_FORMAT,
        name: engine.state().campaign.name.clone(),
        exported_at_ms: now_ms(),
        generator: format!("dnd-session-server {}", env!("CARGO_PKG_VERSION")),
    };
    let (assets, out_path, copy_path) = (assets.to_owned(), out.to_owned(), copy.clone());
    let written = tokio::task::spawn_blocking(move || {
        write_archive(&out_path, &copy_path, &assets, &manifest, false)
    })
    .await;
    std::fs::remove_file(&copy).ok();
    written??;
    Ok(())
}

/// A participant's identity in the campaign.
#[must_use]
pub fn actor(user_id: UserId, role: Role) -> Actor {
    Actor { user_id, role }
}

/// Runs the engine's internal expiry command and broadcasts the committed batch (ADR-0056).
/// The pre-state is cloned only when something is actually due.
async fn expire_and_broadcast(
    engine: &mut CampaignEngine,
    events: &broadcast::Sender<Arc<Broadcast>>,
    checkpoint: &mut Option<(Seq, GameState)>,
) {
    let now = now_ms();
    if engine.next_deadline().is_none_or(|deadline| deadline > now) {
        return;
    }
    let before = engine.state().clone();
    match engine.expire_due(now).await {
        Ok(Some(batch)) => {
            refresh_checkpoint(engine, checkpoint);
            let players = filter_for_players(&before, engine.state(), &batch);
            // Expiry removes effects some players can see; a fresh view would also be correct,
            // but a filtered batch keeps the numbering and costs one event.
            let _ = events.send(Arc::new(Broadcast {
                full: batch,
                players,
                request_owners: Default::default(),
            }));
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "real-time effect expiry failed"),
    }
}

/// Keeps the replay base within the delta window (ADR-0053): cloning once per window beats
/// cloning once per command, and the delta itself is bounded by the same window.
fn refresh_checkpoint(engine: &CampaignEngine, checkpoint: &mut Option<(Seq, GameState)>) {
    let now = engine.last_seq();
    let stale = checkpoint
        .as_ref()
        .is_none_or(|(seq, _)| now.0.saturating_sub(seq.0) > DELTA_WINDOW);
    if stale {
        *checkpoint = Some((now, engine.state().clone()));
    }
}

/// What one recipient missed since `from`, built from the event log and the replay checkpoint
/// (ADR-0053). `None` means the caller must send a snapshot: the history is too old, the
/// checkpoint is behind the request, or the recipient's access changed in between.
async fn delta(
    engine: &CampaignEngine,
    checkpoint: Option<&(Seq, GameState)>,
    from: Seq,
    role: Role,
    user: Option<UserId>,
) -> Option<EventBatch> {
    let now = engine.last_seq();
    if from > now {
        return None;
    }
    let start = from.next();
    if start > now {
        return Some(EventBatch {
            seq: start,
            caused_by: None,
            events: Vec::new(),
        });
    }
    let (base_seq, base) = checkpoint?;
    if from < *base_seq || now.0.saturating_sub(from.0) > DELTA_WINDOW {
        return None;
    }
    let stored = engine.db().events_between(*base_seq, now).await.ok()?;
    let mut state = base.clone();
    for stored_event in stored.iter().filter(|event| event.seq <= from) {
        evolve(&mut state, &stored_event.event);
    }
    let mut out: Vec<Event> = Vec::new();
    for batch in group_batches(
        stored
            .into_iter()
            .filter(|event| event.seq > from)
            .collect::<Vec<_>>(),
    ) {
        let before = state;
        let mut after = before.clone();
        dnd_game_core::evolve_all(&mut after, &batch.events);
        if sees_everything(role) {
            out.extend(batch.events.iter().cloned());
        } else {
            match filter_for_players(&before, &after, &batch) {
                ForPlayers::Batch(filtered) => {
                    let owner = user?;
                    let request_owners = batch
                        .events
                        .iter()
                        .filter_map(|event| match event {
                            Event::RollRequested { request }
                            | Event::RollRequestUpdated { request } => Some((
                                request.id,
                                after
                                    .characters
                                    .get(&request.character)
                                    .and_then(|c| c.owner),
                            )),
                            _ => None,
                        })
                        .collect();
                    let broadcast = Broadcast {
                        full: batch.clone(),
                        players: ForPlayers::Batch(filtered.clone()),
                        request_owners,
                    };
                    let personal = crate::ws::personal_events(&filtered, &broadcast, role, owner);
                    out.extend(personal.events);
                }
                // Access changed in between: patches cannot express it, the client needs a view.
                ForPlayers::Resnapshot => return None,
            }
        }
        state = after;
    }
    let batch = EventBatch {
        seq: start,
        caused_by: None,
        events: out,
    };
    // Replaying is only worth it while it is smaller than the view it replaces.
    let size = serde_json::to_vec(&batch).map(|bytes| bytes.len()).ok()?;
    (size <= MAX_DELTA_BYTES).then_some(batch)
}

/// Stored events as the batches they were committed in. The boundary comes from `batch_seq`
/// (ADR-0053); a row written before that column existed is its own batch, which can only make the
/// replay more conservative — never reveal what was hidden.
fn group_batches(events: Vec<StoredEvent>) -> Vec<EventBatch> {
    let mut batches: Vec<EventBatch> = Vec::new();
    for stored in events {
        let starts_new = match batches.last() {
            None => true,
            Some(batch) => match stored.batch {
                Some(batch_seq) => batch_seq != batch.seq,
                None => true,
            },
        };
        if starts_new {
            batches.push(EventBatch {
                seq: stored.batch.unwrap_or(stored.seq),
                caused_by: None,
                events: vec![stored.event],
            });
        } else if let Some(batch) = batches.last_mut() {
            batch.events.push(stored.event);
        }
    }
    batches
}

#[cfg(test)]
mod delta_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn event(n: i64) -> Event {
        Event::ChatMessagePosted {
            message: ChatMessageDto {
                id: ChatMessageId::new_v7(),
                kind: dnd_protocol::ChatKind::Text,
                sender: None,
                sender_name: "GM".into(),
                body: format!("{n}"),
                roll: None,
                system: None,
                visibility: dnd_protocol::ChatVisibility::All,
                at_ms: None,
                related_characters: Vec::new(),
            },
        }
    }

    #[test]
    fn batches_follow_the_stored_boundary_not_just_consecutive_seq() {
        let batches = group_batches(vec![
            StoredEvent {
                seq: Seq(5),
                batch: Some(Seq(5)),
                event: event(5),
            },
            StoredEvent {
                seq: Seq(6),
                batch: Some(Seq(5)),
                event: event(6),
            },
            // A separate commit: consecutive numbers, another batch.
            StoredEvent {
                seq: Seq(7),
                batch: Some(Seq(7)),
                event: event(7),
            },
        ]);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].seq, Seq(5));
        assert_eq!(batches[0].events.len(), 2);
        assert_eq!(batches[1].seq, Seq(7));
    }

    #[test]
    fn rows_without_a_recorded_boundary_are_their_own_batches() {
        let batches = group_batches(vec![
            StoredEvent {
                seq: Seq(5),
                batch: None,
                event: event(5),
            },
            StoredEvent {
                seq: Seq(6),
                batch: None,
                event: event(6),
            },
        ]);
        assert_eq!(
            batches.len(),
            2,
            "a guess could hide or reveal the wrong thing"
        );
        assert_eq!(batches[0].seq, Seq(5));
        assert_eq!(batches[1].seq, Seq(6));
    }
}

#[cfg(test)]
mod expiry_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_db::LocalUser;
    use dnd_game_core::SplitMix64;
    use dnd_protocol::{EffectDuration, EffectId, TokenEffectDto, TokenId, UserId, Version};

    fn effect(until_ts: i64) -> TokenEffectDto {
        TokenEffectDto {
            id: EffectId::new_v7(),
            token: TokenId::new_v7(),
            label: "Bless".into(),
            condition: None,
            resource: None,
            dice: None,
            heal: false,
            reveal: Vec::new(),
            duration: EffectDuration::RealTime { until_ts },
            version: Version::INITIAL,
        }
    }

    /// The session adapter turns a due deadline into a committed, broadcast batch (ADR-0056).
    #[tokio::test]
    async fn a_due_deadline_is_committed_and_broadcast_without_a_command() {
        let dir = tempfile::tempdir().unwrap();
        let gm = LocalUser {
            id: UserId::new_v7(),
            name: "GM".into(),
            role: Role::Gm,
        };
        let db = CampaignDb::create(&dir.path().join("campaign.sqlite"), "Timers", &gm, 0)
            .await
            .unwrap();
        let mut engine =
            CampaignEngine::load(db, IntentWindow::DEFAULT, Box::new(SplitMix64::new(1)), 0)
                .await
                .unwrap();
        let now = now_ms();
        // One effect far in the future, one already overdue.
        engine
            .commit_events(
                vec![Event::TokenEffectApplied {
                    effect: effect(now + 60_000),
                }],
                Some(gm.id),
                0,
            )
            .await
            .unwrap();
        let (tx, mut rx) = broadcast::channel(4);
        let mut checkpoint = Some((engine.last_seq(), engine.state().clone()));

        // Nothing is due yet: no write, no broadcast.
        expire_and_broadcast(&mut engine, &tx, &mut checkpoint).await;
        assert!(rx.try_recv().is_err());
        assert_eq!(engine.state().effects.len(), 1);

        engine
            .commit_events(
                vec![Event::TokenEffectApplied {
                    effect: effect(now - 1),
                }],
                Some(gm.id),
                0,
            )
            .await
            .unwrap();
        let before = engine.last_seq();
        expire_and_broadcast(&mut engine, &tx, &mut checkpoint).await;
        let broadcast = rx.try_recv().expect("the expiry is broadcast");
        assert_eq!(broadcast.full.seq, before.next());
        assert!(
            broadcast
                .full
                .events
                .iter()
                .any(|event| matches!(event, Event::TokenEffectRemoved { expired: true, .. }))
        );
        assert_eq!(
            engine.state().effects.len(),
            1,
            "only the overdue effect is gone"
        );

        // A repeated signal produces nothing.
        expire_and_broadcast(&mut engine, &tx, &mut checkpoint).await;
        assert!(rx.try_recv().is_err());
        engine.close().await.unwrap();
    }
}
