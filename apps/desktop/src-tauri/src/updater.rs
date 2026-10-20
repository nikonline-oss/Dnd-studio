//! Auto-update (CLAUDE.md §10, ADR-0013): check shortly after start and then every 24 hours
//! against the stable channel (`latest.json` of the latest published GitHub release). Updates are
//! signed with minisign and verified by the plugin. Installing needs the user's confirmation.
//!
//! The updater is active only when `plugins.updater` is configured, i.e. in release builds made
//! with `tauri.release.conf.json`; in development every command reports `updater_disabled`.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;
use tokio::sync::Mutex;

use crate::commands::AppState;
use crate::error::{IpcError, IpcErrorKind};

pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// An update that can be installed.
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event,
)]
pub struct UpdateAvailable {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

/// The last update found by a check, waiting for the user's confirmation.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl std::fmt::Debug for PendingUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingUpdate").finish_non_exhaustive()
    }
}

/// Whether this build has an updater configuration.
pub fn is_configured<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.config().plugins.0.contains_key("updater")
}

fn disabled() -> IpcError {
    IpcError::user(
        "updater_disabled",
        "updates are not configured in this build",
    )
}

fn update_error(e: impl std::fmt::Display) -> IpcError {
    tracing::warn!(error = %e, "update check failed");
    IpcError::new(IpcErrorKind::Recoverable, "update_failed", e.to_string())
}

async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<Option<UpdateAvailable>, IpcError> {
    if !is_configured(app) {
        return Err(disabled());
    }
    let update = app
        .updater()
        .map_err(update_error)?
        .check()
        .await
        .map_err(update_error)?;
    let info = update.as_ref().map(|u| UpdateAvailable {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
    });
    *app.state::<PendingUpdate>().0.lock().await = update;
    if let Some(info) = &info {
        tracing::info!(version = %info.version, "update available");
        if let Err(e) = info.clone().emit(app) {
            tracing::warn!(error = %e, "failed to emit update event");
        }
    }
    Ok(info)
}

/// Background schedule: first check after [`FIRST_CHECK_DELAY`], then every [`CHECK_INTERVAL`].
pub async fn run<R: Runtime>(app: AppHandle<R>) {
    if !is_configured(&app) {
        tracing::debug!("updater not configured; automatic checks disabled");
        return;
    }
    tokio::time::sleep(FIRST_CHECK_DELAY).await;
    loop {
        // Failures (offline, GitHub down) are logged in `update_error` and retried next time.
        let _ = check(&app).await;
        tokio::time::sleep(CHECK_INTERVAL).await;
    }
}

/// Checks for an update now (menu "Check for updates").
#[tauri::command]
#[specta::specta]
pub async fn update_check<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<UpdateAvailable>, IpcError> {
    check(&app).await
}

/// Saves the open campaign, installs the pending update and restarts the app.
#[tauri::command]
#[specta::specta]
pub async fn update_install<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
) -> Result<(), IpcError> {
    if !is_configured(&app) {
        return Err(disabled());
    }
    let update = pending
        .0
        .lock()
        .await
        .take()
        .ok_or_else(|| IpcError::user("no_update", "no update to install"))?;
    if let Some(server) = state.server.lock().await.take() {
        server.close().await?;
    }
    tracing::info!(version = %update.version, "installing update");
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(update_error)?;
    app.restart();
}
