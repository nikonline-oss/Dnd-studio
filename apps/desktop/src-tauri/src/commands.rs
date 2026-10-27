//! IPC commands. Thin wrappers over [`LocalServer`]; all validation happens in Rust.

use std::path::PathBuf;
use std::sync::Arc;

use dnd_protocol::{
    ChatMessageDto, ChatMessageId, EventBatch, IntentEnvelope, IntentOutcome, JournalEntryDto,
    JournalEntryId, SessionId, Snapshot,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;
use tokio::sync::Mutex;

use crate::error::IpcError;
use crate::local_server::{CAMPAIGN_FILE, CampaignInfo, LocalServer, folder_name};
use crate::logging::{LogBuffer, LogLevel, LogRecord};
use crate::remote::{
    HostedSession, OsKeychain, RemoteBook, RemoteEntry, RemoteEvent, RemoteInfo, RemoteSession,
    RemoteStatus, Secrets,
};

/// Shared application state. At most one of `server` (local play) and `remote` (a session
/// server, ADR-0022) is open.
#[derive(Debug)]
pub struct AppState {
    pub(crate) server: Arc<Mutex<Option<LocalServer>>>,
    pub(crate) remote: Arc<Mutex<Option<RemoteSession>>>,
    pub(crate) secrets: Arc<dyn Secrets>,
    /// `remote_sessions.json` and the asset cache, known once the app has started.
    pub(crate) dirs: std::sync::OnceLock<Dirs>,
    /// Folder whose open failed as corrupted; the only target `campaign_restore_backup` accepts.
    pub(crate) last_corrupt: Mutex<Option<PathBuf>>,
    pub(crate) logs: LogBuffer,
}

/// Folders of the app.
#[derive(Debug)]
pub struct Dirs {
    pub book: PathBuf,
    pub asset_cache: PathBuf,
}

impl AppState {
    #[must_use]
    pub fn new(logs: LogBuffer) -> Self {
        Self {
            server: Arc::default(),
            remote: Arc::default(),
            secrets: Arc::new(OsKeychain),
            dirs: std::sync::OnceLock::new(),
            last_corrupt: Mutex::default(),
            logs,
        }
    }
}

/// Emitted after every accepted intent with the events it produced — only once they are
/// committed to the campaign file (ADR-0014).
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GameEventsEmitted(pub EventBatch);

/// The server sent a whole new state (connect, reconnect, a visibility change): it replaces the
/// snapshot the UI has (ADR-0022).
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GameSnapshotReplaced(pub Snapshot);

/// The connection to the session server changed.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct RemoteStatusChanged(pub RemoteStatus);

/// Static application information for the status bar.
#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct AppInfo {
    pub version: String,
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// Asks the user for a parent folder (native dialog, run from Rust so the webview never supplies
/// paths) and creates the campaign there. `None` if the dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn campaign_create<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    name: String,
    system: Option<String>,
) -> Result<Option<CampaignInfo>, IpcError> {
    let Some(parent) = pick_folder(&app).await else {
        return Ok(None);
    };
    let mut server = LocalServer::create(&parent, &name).await?;
    if let Some(system) = system {
        // A new campaign with a bundled rule system (ADR-0023): installed and chosen before the
        // UI ever sees the campaign, so no events need to reach it.
        let package = crate::bundled::package(&system)
            .ok_or_else(|| IpcError::user("not_found", format!("no bundled plugin {system}")))?;
        crate::plugins::install(&mut server, package).await?;
        let envelope = IntentEnvelope {
            intent_id: dnd_protocol::IntentId::new_v7(),
            expected_version: None,
            intent: dnd_protocol::Intent::SetSystemPlugin { id: Some(system) },
        };
        server.submit(&envelope).await?;
    }
    replace_open(&state, server).await.map(Some)
}

/// Asks the user for a campaign folder and opens it. `None` if the dialog was cancelled.
/// A corrupted file yields a `Recoverable` `campaign_corrupt` error; the UI then offers
/// [`campaign_restore_backup`].
#[tauri::command]
#[specta::specta]
pub async fn campaign_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<CampaignInfo>, IpcError> {
    let Some(folder) = pick_folder(&app).await else {
        return Ok(None);
    };
    match LocalServer::open(&folder).await {
        Ok(server) => replace_open(&state, server).await.map(Some),
        Err(e) => {
            if e.code == "campaign_corrupt" {
                *state.last_corrupt.lock().await = Some(folder);
            }
            Err(e)
        }
    }
}

/// Replaces the corrupted campaign file (from the last failed open) with its newest backup and
/// opens it. The damaged file is kept as `campaign.sqlite.corrupt`.
#[tauri::command]
#[specta::specta]
pub async fn campaign_restore_backup(state: State<'_, AppState>) -> Result<CampaignInfo, IpcError> {
    let folder =
        state.last_corrupt.lock().await.take().ok_or_else(|| {
            IpcError::user("nothing_to_restore", "no corrupted campaign to restore")
        })?;
    let backup = dnd_db::restore_latest_backup(&folder.join(CAMPAIGN_FILE))?;
    tracing::warn!(folder = %folder.display(), backup = %backup.display(), "restored from backup");
    let server = LocalServer::open(&folder).await?;
    replace_open(&state, server).await
}

/// Asks where to save and writes the open campaign's archive there (CLAUDE.md §6), originals
/// included. Returns the file written, or `None` if the dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn campaign_export<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<String>, IpcError> {
    let name = {
        let guard = state.server.lock().await;
        let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
        folder_name(&server.info().name)
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("DndStudio", &["zip"])
        .set_file_name(format!("{name}.zip"))
        .save_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    let Some(out) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let guard = state.server.lock().await;
    let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
    server.export(&out, true).await?;
    Ok(Some(out.display().to_string()))
}

/// Asks for an archive and a parent folder, unpacks the campaign into a new folder there and
/// opens it. `None` if a dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn campaign_import<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<CampaignInfo>, IpcError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("DndStudio", &["zip"])
        .pick_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    let Some(archive) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let Some(parent) = pick_folder(&app).await else {
        return Ok(None);
    };
    let folder = crate::local_server::import(&archive, &parent).await?;
    let server = LocalServer::open(&folder).await?;
    replace_open(&state, server).await.map(Some)
}

#[tauri::command]
#[specta::specta]
pub async fn campaign_close(state: State<'_, AppState>) -> Result<(), IpcError> {
    if let Some(remote) = state.remote.lock().await.take() {
        remote.close();
    }
    if let Some(server) = state.server.lock().await.take() {
        server.close().await?;
    }
    Ok(())
}

/// Ctrl+S: forces a WAL checkpoint, so `campaign.sqlite` alone is fully up to date (e.g. before
/// copying it). Game data is already durable without it (write-through, ADR-0014).
#[tauri::command]
#[specta::specta]
pub async fn campaign_checkpoint(state: State<'_, AppState>) -> Result<(), IpcError> {
    // On a session server the data is the server's to keep.
    if state.remote.lock().await.is_some() {
        return Ok(());
    }
    let guard = state.server.lock().await;
    guard
        .as_ref()
        .ok_or_else(IpcError::no_campaign)?
        .checkpoint()
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn campaign_current(
    state: State<'_, AppState>,
) -> Result<Option<CampaignInfo>, IpcError> {
    if let Some(remote) = state.remote.lock().await.as_ref() {
        return Ok(Some(remote.campaign_info()));
    }
    Ok(state.server.lock().await.as_ref().map(LocalServer::info))
}

/// Older chat messages (oldest first) before `before`, at most `limit` (capped at 200).
#[tauri::command]
#[specta::specta]
pub async fn chat_history(
    state: State<'_, AppState>,
    before: Option<ChatMessageId>,
    limit: u32,
) -> Result<Vec<ChatMessageDto>, IpcError> {
    // Paging through the server comes with stage 4; the snapshot carries the recent chat.
    if state.remote.lock().await.is_some() {
        return Ok(Vec::new());
    }
    let guard = state.server.lock().await;
    let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
    server
        .chat_history(before, usize::try_from(limit).unwrap_or(usize::MAX))
        .await
}

/// Text of one journal entry. Visibility filtering per recipient arrives with the server (stage 4);
/// locally the GM sees everything.
#[tauri::command]
#[specta::specta]
pub async fn journal_entry(
    state: State<'_, AppState>,
    id: JournalEntryId,
) -> Result<JournalEntryDto, IpcError> {
    let remote = state.remote.lock().await.clone();
    if let Some(remote) = remote {
        return remote.journal_entry(id).await;
    }
    let guard = state.server.lock().await;
    let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
    server
        .journal_entry(&id)
        .ok_or_else(|| IpcError::user("not_found", "journal entry not found"))
}

#[tauri::command]
#[specta::specta]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, IpcError> {
    if let Some(remote) = state.remote.lock().await.as_ref() {
        return remote.snapshot();
    }
    state
        .server
        .lock()
        .await
        .as_ref()
        .map(LocalServer::snapshot)
        .ok_or_else(IpcError::no_campaign)
}

/// Applies an intent through the game core and commits the resulting events. Only then are they
/// broadcast as [`GameEventsEmitted`], so every view updates the same way for local and remote
/// play and never shows an event that is not on disk.
#[tauri::command]
#[specta::specta]
pub async fn submit_intent<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    envelope: IntentEnvelope,
) -> Result<IntentOutcome, IpcError> {
    // Cloned out of the lock: the round trip to the server must not block other commands.
    let remote = state.remote.lock().await.clone();
    if let Some(remote) = remote {
        // The batch comes back from the server like everyone else's.
        return remote.submit(envelope).await;
    }
    let mut guard = state.server.lock().await;
    let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
    let (outcome, batch) = server.submit(&envelope).await?;
    // Emit while still holding the lock, so events reach the UI in sequence order.
    if let Some(batch) = batch
        && let Err(e) = GameEventsEmitted(batch).emit(&app)
    {
        tracing::error!(error = %e, "failed to emit game events");
    }
    drop(guard);
    Ok(outcome)
}

/// Recent log records for the log panel (live ones arrive as [`LogRecord`] events).
#[tauri::command]
#[specta::specta]
pub fn logs_recent(state: State<'_, AppState>) -> Vec<LogRecord> {
    state.logs.recent()
}

/// Lets the UI write to the application log (e.g. a crashed feature in an error boundary).
#[tauri::command]
#[specta::specta]
pub fn log_frontend(level: LogLevel, message: String) {
    let message: String = message.chars().take(4000).collect();
    match level {
        LogLevel::Error => tracing::error!(target: "frontend", "{message}"),
        LogLevel::Warn => tracing::warn!(target: "frontend", "{message}"),
        LogLevel::Info => tracing::info!(target: "frontend", "{message}"),
        LogLevel::Debug | LogLevel::Trace => tracing::debug!(target: "frontend", "{message}"),
    }
}

async fn replace_open(state: &AppState, server: LocalServer) -> Result<CampaignInfo, IpcError> {
    if let Some(remote) = state.remote.lock().await.take() {
        remote.close();
    }
    let info = server.info();
    let previous = state.server.lock().await.replace(server);
    if let Some(previous) = previous {
        previous.close().await?;
    }
    Ok(info)
}

async fn pick_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.and_then(|f| f.into_path().ok()));
    });
    rx.await.ok().flatten()
}

// ---------- Session server (ADR-0022) ----------

fn dirs(state: &AppState) -> Result<&Dirs, IpcError> {
    state
        .dirs
        .get()
        .ok_or_else(|| IpcError::internal("not_ready", "the app is still starting"))
}

/// Uploads the open campaign to a session server. An empty `host_token` uses the saved one.
#[tauri::command]
#[specta::specta]
pub async fn remote_host(
    state: State<'_, AppState>,
    server: String,
    host_token: String,
) -> Result<HostedSession, IpcError> {
    let book = RemoteBook::new(dirs(&state)?.book.clone());
    let guard = state.server.lock().await;
    let local = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
    crate::remote::host(local, &server, &host_token, state.secrets.as_ref(), &book).await
}

/// Joins a session as a player with an invite link, the access code and a name, and plays there.
#[tauri::command]
#[specta::specta]
pub async fn remote_join<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    invite: String,
    access_code: String,
    name: String,
) -> Result<CampaignInfo, IpcError> {
    let book = RemoteBook::new(dirs(&state)?.book.clone());
    let (entry, token) =
        crate::remote::join(&invite, &access_code, &name, state.secrets.as_ref(), &book).await?;
    go_remote(&app, &state, entry, token).await
}

/// Connects to a saved session (hosted or joined before).
#[tauri::command]
#[specta::specta]
pub async fn remote_connect<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: String,
    session_id: SessionId,
) -> Result<CampaignInfo, IpcError> {
    let book = RemoteBook::new(dirs(&state)?.book.clone());
    let (entry, token) =
        crate::remote::token_for(&server, session_id, state.secrets.as_ref(), &book).await?;
    go_remote(&app, &state, entry, token).await
}

/// Sessions hosted or joined from this app, newest first.
#[tauri::command]
#[specta::specta]
pub async fn remote_sessions(state: State<'_, AppState>) -> Result<Vec<RemoteEntry>, IpcError> {
    let book = RemoteBook::new(dirs(&state)?.book.clone());
    Ok(crate::remote::sessions(state.secrets.as_ref(), &book))
}

/// Forgets a saved session and its secrets.
#[tauri::command]
#[specta::specta]
pub async fn remote_forget(
    state: State<'_, AppState>,
    server: String,
    session_id: SessionId,
) -> Result<(), IpcError> {
    let book = RemoteBook::new(dirs(&state)?.book.clone());
    crate::remote::forget(&server, session_id, state.secrets.as_ref(), &book)
}

/// Stops the connected session on the server (GM) and leaves it.
#[tauri::command]
#[specta::specta]
pub async fn remote_stop(state: State<'_, AppState>) -> Result<(), IpcError> {
    let remote = state
        .remote
        .lock()
        .await
        .clone()
        .ok_or_else(|| IpcError::user("not_in_remote_session", "not connected to a server"))?;
    remote.stop().await?;
    if let Some(remote) = state.remote.lock().await.take() {
        remote.close();
    }
    Ok(())
}

/// The connected session, if any.
#[tauri::command]
#[specta::specta]
pub async fn remote_current(state: State<'_, AppState>) -> Result<Option<RemoteInfo>, IpcError> {
    Ok(state.remote.lock().await.as_ref().map(RemoteSession::info))
}

/// Leaves local play (the local campaign is closed, untouched) and connects to the session.
async fn go_remote<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    entry: RemoteEntry,
    token: String,
) -> Result<CampaignInfo, IpcError> {
    let emitter = app.clone();
    let session = RemoteSession::connect(
        entry,
        token,
        dnd_net::ConnectOptions::default(),
        move |event| {
            let sent = match event {
                RemoteEvent::Snapshot(snapshot) => GameSnapshotReplaced(*snapshot).emit(&emitter),
                RemoteEvent::Events(batch) => GameEventsEmitted(batch).emit(&emitter),
                RemoteEvent::Status(status) => RemoteStatusChanged(status).emit(&emitter),
            };
            if let Err(e) = sent {
                tracing::error!(error = %e, "failed to emit a session event");
            }
        },
    )?;
    let info = session.campaign_info();
    if let Some(local) = state.server.lock().await.take() {
        local.close().await?;
    }
    if let Some(previous) = state.remote.lock().await.replace(session) {
        previous.close();
    }
    tracing::info!(campaign = %info.path, role = ?info.role, "connected to a session server");
    Ok(info)
}

/// Remembers the app's folders (called once at startup).
pub fn init_dirs<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Result<(), IpcError> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| IpcError::internal("no_data_dir", e.to_string()))?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| IpcError::internal("no_cache_dir", e.to_string()))?;
    let _ = state.dirs.set(Dirs {
        book: data.join("remote_sessions.json"),
        asset_cache: cache.join("remote-assets"),
    });
    Ok(())
}

// ---------- Plugins (ADR-0023) ----------

/// The plugins shipped with the app.
#[tauri::command]
#[specta::specta]
pub fn plugin_bundled() -> Vec<crate::plugins::BundledPlugin> {
    crate::plugins::bundled()
}

/// Installs a bundled plugin into the open campaign.
#[tauri::command]
#[specta::specta]
pub async fn plugin_install_bundled<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    id: String,
) -> Result<IntentOutcome, IpcError> {
    let package = crate::bundled::package(&id)
        .ok_or_else(|| IpcError::user("not_found", format!("no bundled plugin {id}")))?;
    install_package(&app, &state, package).await
}

/// Asks for a `.dndplugin` file and installs it into the open campaign. `None` if cancelled.
#[tauri::command]
#[specta::specta]
pub async fn plugin_install_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<IntentOutcome>, IpcError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("DndStudio plugin", &["dndplugin"])
        .pick_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let limit = dnd_plugin::PackageLimits::default().max_package_bytes;
    if std::fs::metadata(&path)?.len() > limit {
        return Err(IpcError::user(
            "too_large",
            "the plugin package is too large",
        ));
    }
    let bytes = std::fs::read(&path)?;
    install_package(&app, &state, &bytes).await.map(Some)
}

async fn install_package<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    package: &[u8],
) -> Result<IntentOutcome, IpcError> {
    // Packages travel with the campaign file; uploading them to a running session: stage 4.
    if state.remote.lock().await.is_some() {
        return Err(IpcError::user(
            "not_in_remote_session",
            "plugins are installed in the local campaign",
        ));
    }
    let mut guard = state.server.lock().await;
    let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
    let (outcome, batch) = crate::plugins::install(server, package).await?;
    if let Some(batch) = batch
        && let Err(e) = GameEventsEmitted(batch).emit(app)
    {
        tracing::error!(error = %e, "failed to emit game events");
    }
    Ok(outcome)
}

/// What a sheet shows besides its data (computed fields, resources) under the campaign's system;
/// `None` without one. Evaluated by the core's rules, locally or on the session's state.
#[tauri::command]
#[specta::specta]
pub async fn sheet_view(
    state: State<'_, AppState>,
    sheet_json: String,
) -> Result<Option<crate::plugins::SheetView>, IpcError> {
    if let Some(remote) = state.remote.lock().await.as_ref() {
        return Ok(remote
            .with_state(|s| crate::plugins::sheet_view(s, &sheet_json))
            .flatten());
    }
    let guard = state.server.lock().await;
    let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
    Ok(crate::plugins::sheet_view(server.state(), &sheet_json))
}
