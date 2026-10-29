//! One running session: a task that owns the campaign's [`CampaignEngine`], takes commands over a
//! channel and broadcasts every committed batch — in full and as players see it (ADR-0022).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use dnd_db::{
    ARCHIVE_FORMAT, ArchiveManifest, CampaignDb, CampaignEngine, LocalUser, write_archive,
};
use dnd_game_core::{
    Actor, ForPlayers, IntentWindow, PlayersSee, filter_for_players, join, may_change_visibility,
    sees_everything, snapshot_for,
};
use dnd_protocol::{
    AssetUsage, ChatMessageDto, ChatMessageId, EventBatch, Intent, IntentEnvelope, IntentOutcome,
    JournalEntryDto, JournalEntryId, JournalVisibility, RejectReason, Role, SessionId, Snapshot,
    UserDto, UserId,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::now_ms;

/// Close reason: the session is over for good (stopped by the GM, expired).
pub const SESSION_CLOSED: &str = "session_closed";
/// Close reason: the server is going down; clients reconnect.
pub const SERVER_RESTART: &str = "server_restart";

/// A committed batch for everyone: GM roles take `full`, the others `players`.
#[derive(Debug)]
pub struct Broadcast {
    pub full: EventBatch,
    pub players: ForPlayers,
}

enum Command {
    Submit {
        envelope: IntentEnvelope,
        actor: Actor,
        reply: oneshot::Sender<anyhow::Result<IntentOutcome>>,
    },
    Snapshot {
        role: Role,
        reply: oneshot::Sender<Snapshot>,
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
        let seed = u64::from_le_bytes(id.0.as_bytes()[8..16].try_into().unwrap_or_default());
        let engine = CampaignEngine::load(db, IntentWindow::DEFAULT, seed, now_ms()).await?;
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
            envelope,
            actor,
            reply,
        })
        .await
    }

    /// What `role` sees now.
    pub async fn snapshot(&self, role: Role) -> Result<Snapshot, SessionGone> {
        self.ask(|reply| Command::Snapshot { role, reply }).await
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
    while let Some(command) = rx.recv().await {
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
                let outcome = match result {
                    Ok((outcome, Some(batch))) => {
                        let players = match &before {
                            Some(before) => filter_for_players(before, engine.state(), &batch),
                            None if !may_change_visibility(&batch) => {
                                filter_for_players(engine.state(), engine.state(), &batch)
                            }
                            None => ForPlayers::Resnapshot,
                        };
                        // No receivers is fine: nobody is connected.
                        let _ = events.send(Arc::new(Broadcast {
                            full: batch,
                            players,
                        }));
                        Ok(outcome)
                    }
                    Ok((outcome, None)) => Ok(outcome),
                    Err(e) => Err(e.into()),
                };
                let _ = reply.send(outcome);
            }
            Command::Snapshot { role, reply } => {
                let _ = reply.send(snapshot_for(
                    role,
                    engine.state(),
                    engine.last_seq(),
                    engine.recent_chat(),
                ));
            }
            Command::JournalEntry { id, role, reply } => {
                let entry = engine.journal_entry(&id).filter(|e| {
                    sees_everything(role) || e.visibility == JournalVisibility::Players
                });
                let _ = reply.send(entry);
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
                            let players =
                                filter_for_players(engine.state(), engine.state(), &batch);
                            let _ = events.send(Arc::new(Broadcast {
                                full: batch,
                                players,
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
                    .any(|a| a.usage == AssetUsage::Audio && a.sha256 == sha256);
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
