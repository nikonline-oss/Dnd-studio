//! Playing on a session server (ADR-0022). The UI talks to the same commands as in local play
//! (ADR-0001): intents go to the server, its snapshots and batches come back as the same Tauri
//! events. Secrets (host token, GM key, participant tokens, access codes) live in the OS keychain;
//! the list of sessions (`remote_sessions.json` in the app data folder) holds no secrets.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use dnd_game_core::{GameState, evolve_all};
use dnd_net::{ConnectOptions, Connection, HttpClient, Incoming, NetError, Status};
use dnd_protocol::{
    CHAT_SNAPSHOT_LEN, ChatMessageDto, Event, EventBatch, IntentEnvelope, IntentOutcome,
    JournalEntryDto, JournalEntryId, Role, Seq, SessionId, Snapshot, UserId,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{IpcError, IpcErrorKind};
use crate::local_server::{CampaignInfo, LocalServer, now_ms};

impl From<NetError> for IpcError {
    fn from(e: NetError) -> Self {
        let message = e.to_string();
        match e {
            NetError::Protocol(_) => {
                tracing::error!(error = %message, "session server protocol error");
                Self::internal("protocol_error", message)
            }
            other => Self::new(IpcErrorKind::User, other.code(), message),
        }
    }
}

// ---------- Secrets ----------

/// Where secrets are kept. The OS keychain in the app; memory in tests.
pub trait Secrets: Send + Sync + std::fmt::Debug {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<(), IpcError>;
    fn delete(&self, key: &str);
}

/// The OS credential store (Windows Credential Manager, macOS Keychain, Secret Service).
#[derive(Debug, Default)]
pub struct OsKeychain;

const KEYCHAIN_SERVICE: &str = "DndStudio";

impl Secrets for OsKeychain {
    fn get(&self, key: &str) -> Option<String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, key)
            .and_then(|e| e.get_password())
            .ok()
    }

    fn set(&self, key: &str, value: &str) -> Result<(), IpcError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, key)
            .and_then(|e| e.set_password(value))
            .map_err(|e| {
                tracing::error!(error = %e, "keychain write failed");
                IpcError::internal("keychain_unavailable", e.to_string())
            })
    }

    fn delete(&self, key: &str) {
        if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, key) {
            let _ = entry.delete_credential();
        }
    }
}

/// Secrets in memory (tests).
#[derive(Debug, Default)]
pub struct MemorySecrets(StdMutex<HashMap<String, String>>);

impl Secrets for MemorySecrets {
    fn get(&self, key: &str) -> Option<String> {
        self.0.lock().ok()?.get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) -> Result<(), IpcError> {
        if let Ok(mut map) = self.0.lock() {
            map.insert(key.to_owned(), value.to_owned());
        }
        Ok(())
    }

    fn delete(&self, key: &str) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(key);
        }
    }
}

fn host_key(server: &str) -> String {
    format!("host:{server}")
}
fn gm_key(server: &str, session: SessionId) -> String {
    format!("gm:{server}/{session}")
}
fn code_key(server: &str, session: SessionId) -> String {
    format!("code:{server}/{session}")
}
fn token_key(server: &str, session: SessionId) -> String {
    format!("token:{server}/{session}")
}

// ---------- The list of sessions ----------

/// A session this app hosted or joined (no secrets).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RemoteEntry {
    /// Local campaign uploaded by this GM; absent for joined and older saved sessions.
    #[serde(default)]
    pub local_campaign: Option<String>,
    pub server: String,
    pub session_id: SessionId,
    pub session_name: String,
    /// Uploaded from this app (its GM key is in the keychain).
    pub hosted: bool,
    /// The participant's name, role and campaign user once joined.
    pub name: Option<String>,
    pub role: Option<Role>,
    pub user_id: Option<UserId>,
    pub invite_url: Option<String>,
    /// The access code, for the GM to pass on (from the keychain, never stored in the list).
    pub access_code: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub saved_at_ms: i64,
}

/// `remote_sessions.json`.
#[derive(Debug)]
pub struct RemoteBook {
    path: PathBuf,
}

impl RemoteBook {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Newest first.
    #[must_use]
    pub fn list(&self) -> Vec<RemoteEntry> {
        let mut entries: Vec<RemoteEntry> = std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        entries.sort_by_key(|e| std::cmp::Reverse(e.saved_at_ms));
        entries
    }

    #[must_use]
    pub fn find(&self, server: &str, session: SessionId) -> Option<RemoteEntry> {
        self.list()
            .into_iter()
            .find(|e| e.server == server && e.session_id == session)
    }

    pub fn save(&self, entry: RemoteEntry) -> Result<(), IpcError> {
        let mut entries: Vec<RemoteEntry> = self
            .list()
            .into_iter()
            .filter(|e| !(e.server == entry.server && e.session_id == entry.session_id))
            .collect();
        entries.push(RemoteEntry {
            access_code: None,
            ..entry
        });
        self.write(&entries)
    }

    pub fn remove(&self, server: &str, session: SessionId) -> Result<(), IpcError> {
        let entries: Vec<RemoteEntry> = self
            .list()
            .into_iter()
            .filter(|e| !(e.server == server && e.session_id == session))
            .collect();
        self.write(&entries)
    }

    fn write(&self, entries: &[RemoteEntry]) -> Result<(), IpcError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(entries)
            .map_err(|e| IpcError::internal("storage_error", e.to_string()))?;
        let partial = self.path.with_extension("json.partial");
        std::fs::write(&partial, json)?;
        std::fs::rename(partial, &self.path)?;
        Ok(())
    }
}

// ---------- Hosting and joining ----------

/// What the GM sees after uploading a campaign.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct HostedSession {
    pub server: String,
    pub session_id: SessionId,
    pub name: String,
    pub access_code: String,
    pub invite_url: String,
}

/// Uploads the open campaign to `server`. An empty `host_token` uses the one saved for this
/// server; a given one is saved for next time.
pub async fn host(
    local: &LocalServer,
    server: &str,
    host_token: &str,
    secrets: &dyn Secrets,
    book: &RemoteBook,
) -> Result<HostedSession, IpcError> {
    if local.db().remote_binding().await?.is_some() {
        return Err(IpcError::user(
            "campaign_on_server",
            "stop the existing hosted session before hosting this campaign again",
        ));
    }
    let http = HttpClient::new(server)?;
    let base = http.base().to_owned();
    let host_token = match host_token.trim() {
        "" => secrets
            .get(&host_key(&base))
            .ok_or_else(|| IpcError::user("no_host_token", "enter the server's host token"))?,
        given => given.to_owned(),
    };
    let archive =
        std::env::temp_dir().join(format!("dndstudio-upload-{}.zip", uuid::Uuid::now_v7()));
    // Originals stay home: the server serves derived files only.
    let exported = local.export(&archive, false).await;
    let created = match exported {
        Ok(_) => http.create_session(&host_token, &archive).await,
        Err(e) => {
            std::fs::remove_file(&archive).ok();
            return Err(e);
        }
    };
    std::fs::remove_file(&archive).ok();
    let created = created?;
    secrets.set(&host_key(&base), &host_token)?;
    secrets.set(&gm_key(&base, created.session_id), &created.gm_key)?;
    secrets.set(&code_key(&base, created.session_id), &created.access_code)?;
    book.save(RemoteEntry {
        local_campaign: Some(local.folder().display().to_string()),
        server: base.clone(),
        session_id: created.session_id,
        session_name: created.name.clone(),
        hosted: true,
        name: None,
        role: Some(Role::Gm),
        user_id: None,
        invite_url: Some(created.invite_url.clone()),
        access_code: None,
        saved_at_ms: now_ms(),
    })?;
    local
        .db()
        .set_remote_binding(Some(&format!("{base}/{}", created.session_id)))
        .await?;
    tracing::info!(server = %base, session = %created.session_id, "campaign uploaded");
    Ok(HostedSession {
        server: base,
        session_id: created.session_id,
        name: created.name,
        access_code: created.access_code,
        invite_url: created.invite_url,
    })
}

/// Joins with an invite link, an access code and a name (a player), and remembers the token.
pub async fn join(
    invite: &str,
    access_code: &str,
    name: &str,
    secrets: &dyn Secrets,
    book: &RemoteBook,
) -> Result<(RemoteEntry, String), IpcError> {
    let (server, session) = dnd_net::parse_invite(invite)?;
    let http = HttpClient::new(&server)?;
    let joined = http.join_player(session, access_code, name).await?;
    secrets.set(&token_key(&server, session), &joined.token)?;
    let entry = RemoteEntry {
        local_campaign: None,
        server,
        session_id: session,
        session_name: joined.session_name,
        hosted: false,
        name: Some(joined.name),
        role: Some(joined.role),
        user_id: Some(joined.user_id),
        invite_url: Some(invite.trim().to_owned()),
        access_code: None,
        saved_at_ms: now_ms(),
    };
    book.save(entry.clone())?;
    Ok((entry, joined.token))
}

/// The token to connect to a saved session with: the participant token, or — for a hosted
/// session the GM has not joined yet — a fresh one from the GM key.
pub async fn token_for(
    server: &str,
    session: SessionId,
    secrets: &dyn Secrets,
    book: &RemoteBook,
) -> Result<(RemoteEntry, String), IpcError> {
    let mut entry = book
        .find(server, session)
        .ok_or_else(|| IpcError::user("not_found", "unknown session"))?;
    if let Some(token) = secrets.get(&token_key(server, session)) {
        return Ok((entry, token));
    }
    let key = secrets
        .get(&gm_key(server, session))
        .ok_or_else(|| IpcError::user("unauthorized", "no saved access to this session"))?;
    let joined = HttpClient::new(server)?.join_gm(session, &key).await?;
    secrets.set(&token_key(server, session), &joined.token)?;
    entry.name = Some(joined.name);
    entry.role = Some(joined.role);
    entry.user_id = Some(joined.user_id);
    entry.saved_at_ms = now_ms();
    book.save(entry.clone())?;
    Ok((entry, joined.token))
}

/// Saved sessions with the access codes of hosted ones filled in from the keychain.
#[must_use]
pub fn sessions(secrets: &dyn Secrets, book: &RemoteBook) -> Vec<RemoteEntry> {
    book.list()
        .into_iter()
        .map(|e| RemoteEntry {
            access_code: e
                .hosted
                .then(|| secrets.get(&code_key(&e.server, e.session_id)))
                .flatten(),
            ..e
        })
        .collect()
}

/// Forgets a saved session and its secrets.
pub fn forget(
    server: &str,
    session: SessionId,
    secrets: &dyn Secrets,
    book: &RemoteBook,
) -> Result<(), IpcError> {
    for key in [
        gm_key(server, session),
        code_key(server, session),
        token_key(server, session),
    ] {
        secrets.delete(&key);
    }
    book.remove(server, session)
}

// ---------- The live session ----------

/// Connection state for the status bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RemoteStatus {
    Connecting,
    Online,
    Reconnecting {
        attempt: u32,
        #[specta(type = specta_typescript::Number)]
        retry_in_ms: u64,
    },
    Closed {
        reason: String,
    },
}

impl From<&Status> for RemoteStatus {
    fn from(s: &Status) -> Self {
        match s {
            Status::Connecting => Self::Connecting,
            Status::Online => Self::Online,
            Status::Reconnecting { attempt, retry_in } => Self::Reconnecting {
                attempt: *attempt,
                retry_in_ms: u64::try_from(retry_in.as_millis()).unwrap_or(u64::MAX),
            },
            Status::Closed { reason } => Self::Closed {
                reason: reason.clone(),
            },
        }
    }
}

/// The connected session, for the status bar and the hosting screen.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct RemoteInfo {
    pub server: String,
    pub session_id: SessionId,
    pub session_name: String,
    pub role: Role,
    pub hosted: bool,
    pub status: RemoteStatus,
}

/// What the session sends the UI.
#[derive(Debug)]
pub enum RemoteEvent {
    Snapshot(Box<Snapshot>),
    Events(EventBatch),
    Status(RemoteStatus),
}

/// The players' copy of the game kept from snapshots and batches, so `get_snapshot` answers
/// at once.
#[derive(Debug)]
struct View {
    state: GameState,
    seq: Seq,
    chat: Vec<ChatMessageDto>,
}

impl View {
    fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.state.to_snapshot(self.seq);
        snapshot.chat = self.chat.clone();
        snapshot
    }
}

/// A connected session. Clones share the connection.
#[derive(Clone, Debug)]
pub struct RemoteSession {
    entry: RemoteEntry,
    user_id: UserId,
    role: Role,
    token: String,
    http: HttpClient,
    connection: Arc<Connection>,
    view: Arc<StdMutex<Option<View>>>,
    status: Arc<StdMutex<RemoteStatus>>,
}

impl RemoteSession {
    /// Connects with `token`; everything the server sends reaches `on_event` (in order).
    pub fn connect(
        entry: RemoteEntry,
        token: String,
        options: ConnectOptions,
        on_event: impl Fn(RemoteEvent) + Send + 'static,
    ) -> Result<Self, IpcError> {
        let role = entry.role.unwrap_or(Role::Player);
        let user_id = entry
            .user_id
            .ok_or_else(|| IpcError::internal("no_user", "the session entry has no user"))?;
        let http = HttpClient::new(&entry.server)?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let connection = Arc::new(Connection::open(
            &entry.server,
            entry.session_id,
            &token,
            options,
            tx,
        )?);
        let view: Arc<StdMutex<Option<View>>> = Arc::default();
        let status = Arc::new(StdMutex::new(RemoteStatus::Connecting));
        let (view_w, status_w, conn) = (view.clone(), status.clone(), Arc::downgrade(&connection));
        tokio::spawn(async move {
            while let Some(incoming) = rx.recv().await {
                match incoming {
                    Incoming::Snapshot(snapshot) => {
                        if let Ok(mut view) = view_w.lock() {
                            *view = Some(View {
                                seq: snapshot.seq,
                                chat: snapshot.chat.clone(),
                                state: GameState::from_snapshot((*snapshot).clone()),
                            });
                        }
                        on_event(RemoteEvent::Snapshot(snapshot));
                    }
                    Incoming::Events(batch) => {
                        let gap = view_w.lock().map_or(true, |mut view| match view.as_mut() {
                            Some(view) => apply(view, &batch),
                            None => true,
                        });
                        if gap && let Some(conn) = conn.upgrade() {
                            conn.resync();
                        }
                        on_event(RemoteEvent::Events(batch));
                    }
                    Incoming::Status(s) => {
                        let s = RemoteStatus::from(&s);
                        if let Ok(mut status) = status_w.lock() {
                            *status = s.clone();
                        }
                        on_event(RemoteEvent::Status(s));
                    }
                }
            }
        });
        Ok(Self {
            entry,
            user_id,
            role,
            token,
            http,
            connection,
            view,
            status,
        })
    }

    /// The session as the UI's "campaign". `path` is unique per server and session (the UI keys
    /// per-campaign settings with it).
    #[must_use]
    pub fn campaign_info(&self) -> CampaignInfo {
        CampaignInfo {
            name: self.entry.session_name.clone(),
            path: format!("{}/sessions/{}", self.entry.server, self.entry.session_id),
            user_id: self.user_id,
            role: self.role,
        }
    }

    #[must_use]
    pub fn info(&self) -> RemoteInfo {
        RemoteInfo {
            server: self.entry.server.clone(),
            session_id: self.entry.session_id,
            session_name: self.entry.session_name.clone(),
            role: self.role,
            hosted: self.entry.hosted,
            status: self.status(),
        }
    }

    #[must_use]
    pub fn status(&self) -> RemoteStatus {
        self.status
            .lock()
            .map_or(RemoteStatus::Connecting, |s| s.clone())
    }

    /// Runs `f` on the latest state (`None` until the first snapshot has arrived).
    pub fn with_state<T>(&self, f: impl FnOnce(&GameState) -> T) -> Option<T> {
        self.view
            .lock()
            .ok()
            .and_then(|v| v.as_ref().map(|v| f(&v.state)))
    }

    /// The latest state, or an error until the first snapshot has arrived.
    pub fn snapshot(&self) -> Result<Snapshot, IpcError> {
        self.view
            .lock()
            .ok()
            .and_then(|v| v.as_ref().map(View::snapshot))
            .ok_or_else(|| IpcError::user("connecting", "waiting for the server"))
    }

    pub async fn submit(&self, envelope: IntentEnvelope) -> Result<IntentOutcome, IpcError> {
        Ok(self.connection.submit(envelope).await?)
    }

    /// Replays durable offline edits in their original order. Transport failures leave the
    /// current operation on disk; the same intent id makes a retry idempotent (ADR-0015).
    pub async fn sync_pending(&self, db: &dnd_db::CampaignDb) -> Result<(), IpcError> {
        if self.role != Role::Gm {
            return Err(IpcError::user(
                "forbidden",
                "only the GM may replay offline edits",
            ));
        }
        check_replica(
            db,
            self.user_id,
            &format!("{}/{}", self.entry.server, self.entry.session_id),
        )
        .await?;
        for operation in db.pending_ops().await? {
            if operation.status == "conflict" {
                break;
            }
            let assets = db
                .path()
                .parent()
                .ok_or_else(|| {
                    IpcError::internal("invalid_campaign", "campaign has no parent directory")
                })?
                .join("assets");
            match &operation.envelope.intent {
                dnd_protocol::Intent::RegisterAsset { asset } => {
                    if asset.usage == dnd_protocol::AssetUsage::Audio {
                        self.upload_local_file(&assets, &asset.sha256, "audio")
                            .await?;
                    } else {
                        let mut hashes = std::collections::BTreeSet::new();
                        hashes.insert(asset.thumbnail_sha256.as_str());
                        if let Some(display) = &asset.display_sha256 {
                            hashes.insert(display.as_str());
                        }
                        hashes.extend(asset.tiles.iter().map(|tile| tile.sha256.as_str()));
                        for hash in hashes {
                            self.upload_local_file(&assets, hash, "webp").await?;
                        }
                    }
                }
                dnd_protocol::Intent::InstallPlugin { plugin } => {
                    self.upload_local_file(&assets, &plugin.archive_sha256, "plugin")
                        .await?;
                }
                _ => {}
            }
            match self.submit(operation.envelope).await? {
                IntentOutcome::Accepted { .. } => db.delete_pending_op(operation.id).await?,
                IntentOutcome::Rejected { reason } => {
                    db.mark_pending_conflict(operation.id, &reason).await?;
                    // Later operations can depend on this edit. Keep their original order
                    // until the GM resolves the rejected operation.
                    break;
                }
            }
        }
        Ok(())
    }

    async fn upload_local_file(
        &self,
        assets: &Path,
        sha: &str,
        kind: &str,
    ) -> Result<(), IpcError> {
        if !dnd_assets::is_sha256_hex(sha) {
            return Err(IpcError::user("invalid_asset", "invalid asset hash"));
        }
        let path = match kind {
            "webp" => assets.join(format!("{sha}.webp")),
            "audio" => assets.join(format!("{sha}.audio")),
            "plugin" => assets.join("plugins").join(format!("{sha}.dndplugin")),
            _ => return Err(IpcError::user("invalid_asset", "unsupported asset kind")),
        };
        if tokio::fs::metadata(&path).await?.len() > 100 * 1024 * 1024 {
            return Err(IpcError::user("too_large", "asset is too large"));
        }
        let bytes = tokio::fs::read(path).await?;
        self.http
            .upload_asset(self.entry.session_id, &self.token, sha, kind, bytes)
            .await?;
        Ok(())
    }

    /// Refresh the GM's offline copy from a validated server archive. Pending edits survive
    /// the replacement; originals remain in the local asset directory.
    pub async fn refresh_replica(&self, local: &mut Option<LocalServer>) -> Result<(), IpcError> {
        if self.role != Role::Gm || !self.entry.hosted {
            return Err(IpcError::user("forbidden", "no GM replica"));
        }
        let Some(current) = local.as_ref() else {
            return Ok(());
        };
        check_replica(
            current.db(),
            self.user_id,
            &format!("{}/{}", self.entry.server, self.entry.session_id),
        )
        .await?;
        let folder = current.folder().to_owned();
        let pending = current.db().pending_ops().await?;
        let binding = format!("{}/{}", self.entry.server, self.entry.session_id);
        let staging = folder.join(format!(".session-sync-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&staging)?;
        let _cleanup = ReplicaStaging {
            path: staging.clone(),
            parent: folder.clone(),
        };
        let archive = staging.join("session.zip");
        self.http
            .export(self.entry.session_id, &self.token, &archive)
            .await?;
        let extracted = staging.join("validated");
        dnd_db::read_archive(&archive, &extracted, dnd_db::ArchiveLimits::DEFAULT)?;
        let incoming = extracted.join(crate::local_server::CAMPAIGN_FILE);
        let (db, _) = dnd_db::CampaignDb::open(&incoming, now_ms()).await?;
        if db
            .local_user()
            .await?
            .is_none_or(|owner| owner.id != self.user_id)
        {
            db.close().await?;
            return Err(IpcError::user(
                "replica_mismatch",
                "server archive belongs to another campaign",
            ));
        }
        db.set_remote_binding(Some(&binding)).await?;
        db.clear_speculative_marker().await?;
        // Never import another machine's pending queue from a campaign export.
        for op in db.pending_ops().await? {
            db.delete_pending_op(op.id).await?;
        }
        for op in pending {
            let id = db.enqueue_pending_op(&op.envelope, now_ms()).await?;
            if let Some(reason) = op.conflict {
                db.mark_pending_conflict(id, &reason).await?;
            }
        }
        let (base, _) = db.load_state().await?;
        if let Ok(mut view) = self.view.lock()
            && let Some(view) = view.as_mut()
        {
            for (id, entry) in base.journal {
                if view
                    .state
                    .journal
                    .get(&id)
                    .is_some_and(|current| current.version == entry.version)
                {
                    view.state.journal.insert(id, entry);
                }
            }
        }
        db.close().await?;
        copy_replica_assets(&extracted.join("assets"), &folder.join("assets"))?;
        current.db().backup("before-session-sync", now_ms()).await?;
        current.checkpoint().await?;
        let file = folder.join(crate::local_server::CAMPAIGN_FILE);
        let previous = folder.join(crate::local_server::REPLICA_OLD);
        let prepared = folder.join(crate::local_server::REPLICA_NEW);
        std::fs::copy(&incoming, &prepared)?;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&prepared)?
            .sync_all()?;
        if let Some(current) = local.take() {
            current.close().await?;
        }
        let replaced = async {
            std::fs::rename(&file, &previous)?;
            std::fs::rename(&prepared, &file)?;
            LocalServer::open_replaced(&folder).await
        }
        .await;
        match replaced {
            Ok(reopened) => {
                *local = Some(reopened);
                std::fs::remove_file(&previous).ok();
            }
            Err(error) => {
                *local = Some(LocalServer::open(&folder).await?);
                return Err(error);
            }
        }
        // The staging directory was generated directly inside this campaign.
        Ok(())
    }

    pub async fn journal_entry(&self, id: JournalEntryId) -> Result<JournalEntryDto, IpcError> {
        self.connection
            .journal_entry(id)
            .await?
            .ok_or_else(|| IpcError::user("not_found", "journal entry not found"))
    }

    /// An asset file from the session, through `cache` (files are content-addressed, so a cached
    /// one never goes stale).
    pub async fn asset(&self, sha256: &str, cache: &Path, mime: &str) -> Result<Vec<u8>, IpcError> {
        let extension = match mime {
            "audio/mpeg" => "mp3",
            "audio/ogg" => "ogg",
            "audio/wav" | "audio/x-wav" => "wav",
            "audio/flac" => "flac",
            "audio/mp4" => "m4a",
            "audio/webm" => "webm",
            "image/webp" => "webp",
            _ => return Err(IpcError::user("invalid_asset", "unsupported asset type")),
        };
        let path = cache.join(format!("{sha256}.{extension}"));
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }
        let bytes = self
            .http
            .asset(self.entry.session_id, &self.token, sha256)
            .await?;
        if tokio::fs::create_dir_all(cache).await.is_ok() {
            let partial = cache.join(format!("{sha256}.partial"));
            if tokio::fs::write(&partial, &bytes).await.is_ok() {
                tokio::fs::rename(&partial, &path).await.ok();
            }
        }
        Ok(bytes)
    }

    /// Stops the session on the server (GM).
    pub async fn stop(&self) -> Result<(), IpcError> {
        if self.role != Role::Gm {
            return Err(IpcError::user(
                "forbidden",
                "only the GM may stop the session",
            ));
        }
        self.http.stop(self.entry.session_id, &self.token).await?;
        Ok(())
    }

    pub fn close(&self) {
        self.connection.close();
    }
}

/// Guard both the session association and the campaign's original local owner before replay.
pub async fn check_replica(
    db: &dnd_db::CampaignDb,
    owner: UserId,
    binding: &str,
) -> Result<(), IpcError> {
    if db.remote_binding().await?.as_deref() != Some(binding)
        || db.local_user().await?.is_none_or(|user| user.id != owner)
    {
        return Err(IpcError::user(
            "replica_mismatch",
            "offline copy belongs to another campaign or session",
        ));
    }
    Ok(())
}

struct ReplicaStaging {
    path: PathBuf,
    parent: PathBuf,
}

impl Drop for ReplicaStaging {
    fn drop(&mut self) {
        if self.path.parent() == Some(self.parent.as_path()) {
            std::fs::remove_dir_all(&self.path).ok();
        }
    }
}

fn copy_replica_assets(from: &Path, to: &Path) -> Result<(), IpcError> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_replica_assets(&entry.path(), &to.join(entry.file_name()))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        } else {
            return Err(IpcError::user("invalid_archive", "unsupported asset entry"));
        }
    }
    Ok(())
}

/// Applies a batch to the view; `true` on a gap (the caller asks for a snapshot).
fn apply(view: &mut View, batch: &EventBatch) -> bool {
    let last = batch.last_seq();
    if last <= view.seq {
        return false;
    }
    if batch.seq.0 != view.seq.0 + 1 {
        return true;
    }
    evolve_all(&mut view.state, &batch.events);
    for event in &batch.events {
        if let Event::ChatMessagePosted { message } = event {
            view.chat.push(ChatMessageDto {
                at_ms: message.at_ms.or(Some(now_ms())),
                ..message.clone()
            });
        }
    }
    let excess = view.chat.len().saturating_sub(CHAT_SNAPSHOT_LEN);
    view.chat.drain(..excess);
    view.seq = last;
    false
}
