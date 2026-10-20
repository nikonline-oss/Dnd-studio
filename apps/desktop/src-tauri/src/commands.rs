//! IPC commands. Thin wrappers over [`LocalServer`]; all validation happens in Rust.

use std::path::PathBuf;
use std::sync::Arc;

use dnd_protocol::{EventBatch, IntentEnvelope, Snapshot};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;
use tokio::sync::{Mutex, Notify};

use crate::error::{IpcError, IpcErrorKind};
use crate::local_server::{CAMPAIGN_FILE, CampaignInfo, IntentOutcome, LocalServer};
use crate::logging::{LogBuffer, LogLevel, LogRecord};

/// Shared application state.
#[derive(Debug)]
pub struct AppState {
    pub(crate) server: Arc<Mutex<Option<LocalServer>>>,
    /// Signalled on every accepted intent; drives the autosave debounce.
    pub(crate) dirty: Arc<Notify>,
    /// Folder whose open failed as corrupted; the only target `campaign_restore_backup` accepts.
    pub(crate) last_corrupt: Mutex<Option<PathBuf>>,
    pub(crate) logs: LogBuffer,
}

impl AppState {
    #[must_use]
    pub fn new(logs: LogBuffer) -> Self {
        Self {
            server: Arc::default(),
            dirty: Arc::default(),
            last_corrupt: Mutex::default(),
            logs,
        }
    }
}

/// Emitted after every accepted intent with the events it produced.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GameEventsEmitted(pub EventBatch);

/// Whether the open campaign has unsaved changes, and the last save error if any.
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event,
)]
pub struct SaveStatusChanged {
    pub dirty: bool,
    pub error: Option<String>,
}

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
) -> Result<Option<CampaignInfo>, IpcError> {
    let Some(parent) = pick_folder(&app).await else {
        return Ok(None);
    };
    let server = LocalServer::create(&parent, &name).await?;
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

#[tauri::command]
#[specta::specta]
pub async fn campaign_close<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    if let Some(server) = state.server.lock().await.take() {
        server.close().await?;
    }
    emit_save_status(&app, false, None);
    Ok(())
}

/// Saves immediately (also happens automatically, see ADR-0011).
#[tauri::command]
#[specta::specta]
pub async fn campaign_save<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    flush_open(&app, &state.server).await
}

#[tauri::command]
#[specta::specta]
pub async fn campaign_current(
    state: State<'_, AppState>,
) -> Result<Option<CampaignInfo>, IpcError> {
    Ok(state.server.lock().await.as_ref().map(LocalServer::info))
}

#[tauri::command]
#[specta::specta]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, IpcError> {
    state
        .server
        .lock()
        .await
        .as_ref()
        .map(LocalServer::snapshot)
        .ok_or_else(IpcError::no_campaign)
}

/// Applies an intent through the game core. Accepted events are also broadcast as
/// [`GameEventsEmitted`], so every view updates the same way for local and remote play.
#[tauri::command]
#[specta::specta]
pub async fn submit_intent<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    envelope: IntentEnvelope,
) -> Result<IntentOutcome, IpcError> {
    let mut guard = state.server.lock().await;
    let server = guard.as_mut().ok_or_else(IpcError::no_campaign)?;
    let (outcome, batch) = server.submit(&envelope);
    drop(guard);
    if let Some(batch) = batch {
        state.dirty.notify_one();
        emit_save_status(&app, true, None);
        if let Err(e) = GameEventsEmitted(batch).emit(&app) {
            tracing::error!(error = %e, "failed to emit game events");
        }
    }
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

/// Flushes the open campaign, if any, and reports the save status.
pub(crate) async fn flush_open<R: Runtime>(
    app: &AppHandle<R>,
    server: &Mutex<Option<LocalServer>>,
) -> Result<(), IpcError> {
    let mut guard = server.lock().await;
    let Some(server) = guard.as_mut() else {
        return Ok(());
    };
    match server.flush().await {
        Ok(()) => {
            emit_save_status(app, false, None);
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "autosave failed");
            emit_save_status(app, true, Some(e.message.clone()));
            Err(IpcError::new(
                IpcErrorKind::Recoverable,
                "save_failed",
                e.message,
            ))
        }
    }
}

pub(crate) fn emit_save_status<R: Runtime>(app: &AppHandle<R>, dirty: bool, error: Option<String>) {
    if let Err(e) = (SaveStatusChanged { dirty, error }).emit(app) {
        tracing::warn!(error = %e, "failed to emit save status");
    }
}

async fn replace_open(state: &AppState, server: LocalServer) -> Result<CampaignInfo, IpcError> {
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
