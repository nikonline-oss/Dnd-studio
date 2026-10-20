//! IPC commands. Thin wrappers over [`LocalServer`]; all validation happens in Rust.

use std::path::PathBuf;

use dnd_protocol::{EventBatch, IntentEnvelope, Snapshot};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;
use tokio::sync::Mutex;

use crate::error::IpcError;
use crate::local_server::{CampaignInfo, IntentOutcome, LocalServer};

/// Shared application state.
#[derive(Debug, Default)]
pub struct AppState {
    pub(crate) server: Mutex<Option<LocalServer>>,
}

/// Emitted after every accepted intent with the events it produced.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct GameEventsEmitted(pub EventBatch);

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
#[tauri::command]
#[specta::specta]
pub async fn campaign_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Option<CampaignInfo>, IpcError> {
    let Some(folder) = pick_folder(&app).await else {
        return Ok(None);
    };
    let server = LocalServer::open(&folder).await?;
    replace_open(&state, server).await.map(Some)
}

#[tauri::command]
#[specta::specta]
pub async fn campaign_close(state: State<'_, AppState>) -> Result<(), IpcError> {
    if let Some(server) = state.server.lock().await.take() {
        server.close().await?;
    }
    Ok(())
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
    let (outcome, batch) = server.submit(envelope).await?;
    if let Some(batch) = batch
        && let Err(e) = GameEventsEmitted(batch).emit(&app)
    {
        tracing::error!(error = %e, "failed to emit game events");
    }
    Ok(outcome)
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
