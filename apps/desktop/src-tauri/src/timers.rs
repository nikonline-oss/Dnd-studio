//! Real-time effect deadlines in local play (ADR-0056).
//!
//! The core has no clock and removes expired effects lazily when an intent arrives. A quiet table
//! would therefore keep an expired effect until the next action. This task is the adapter that
//! closes the gap: it wakes at the nearest deadline, runs the engine's internal expiry command and
//! emits the committed batch, exactly as an intent's batch would be emitted.
//!
//! A campaign that is a replica of a hosted session is skipped: real time for it belongs to the
//! server session, and a local removal would diverge from the authority's sequence.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep, sleep_until};

use crate::commands::GameEventsEmitted;
use crate::local_server::{LocalServer, now_ms};

/// How often to look for a new deadline when nothing is scheduled.
pub const IDLE_POLL: Duration = Duration::from_secs(1);
/// Never sleep longer than this even with a far deadline: a new effect may be created meanwhile,
/// and it is discovered by the next poll.
const MAX_SLEEP: Duration = Duration::from_secs(1);

/// Runs for the lifetime of the app.
pub async fn run<R: Runtime>(server: Arc<Mutex<Option<LocalServer>>>, app: AppHandle<R>) {
    loop {
        let wait = {
            let guard = server.lock().await;
            match guard.as_ref() {
                Some(local) if !local.is_replica().await => match local.next_deadline() {
                    Some(deadline) => {
                        let remaining = deadline.saturating_sub(now_ms()).max(0);
                        Duration::from_millis(u64::try_from(remaining).unwrap_or(u64::MAX))
                            .min(MAX_SLEEP)
                    }
                    None => IDLE_POLL,
                },
                _ => IDLE_POLL,
            }
        };
        sleep_until(Instant::now() + wait).await;
        let batch = {
            let mut guard = server.lock().await;
            match guard.as_mut() {
                Some(local) if !local.is_replica().await => match local.expire_due().await {
                    Ok(batch) => batch,
                    Err(error) => {
                        tracing::warn!(code = error.code, "effect expiry failed");
                        None
                    }
                },
                _ => None,
            }
        };
        if let Some(batch) = batch
            && let Err(error) = GameEventsEmitted(batch).emit(&app)
        {
            tracing::error!(error = %error, "failed to emit expiry events");
        }
        // Yield so a stream of immediate deadlines cannot starve other tasks.
        sleep(Duration::from_millis(1)).await;
    }
}
