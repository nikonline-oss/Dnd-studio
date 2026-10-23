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

    pub async fn journal_entry(&self, id: JournalEntryId) -> Result<JournalEntryDto, IpcError> {
        self.connection
            .journal_entry(id)
            .await?
            .ok_or_else(|| IpcError::user("not_found", "journal entry not found"))
    }

    /// An asset file from the session, through `cache` (files are content-addressed, so a cached
    /// one never goes stale).
    pub async fn asset(&self, sha256: &str, cache: &Path) -> Result<Vec<u8>, IpcError> {
        let path = cache.join(format!("{sha256}.webp"));
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
