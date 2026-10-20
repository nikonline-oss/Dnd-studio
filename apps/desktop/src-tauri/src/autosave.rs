//! Autosave scheduling (CLAUDE.md §10, ADR-0011): debounce 2 s after the last change (but no more
//! than 10 s of continuous edits), periodic flush + WAL checkpoint every 5 minutes.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Runtime};
use tokio::sync::{Mutex, Notify};
use tokio::time::{Instant, MissedTickBehavior, interval, timeout};

use crate::commands::{emit_save_status, flush_open};
use crate::local_server::LocalServer;

pub const DEBOUNCE: Duration = Duration::from_secs(2);
/// Upper bound on unsaved time during continuous edits (e.g. dragging tokens).
pub const MAX_DELAY: Duration = Duration::from_secs(10);
pub const PERIODIC: Duration = Duration::from_secs(5 * 60);

/// Waits until `dirty` has been quiet for `debounce`, or `max_delay` passed since `start`.
pub async fn settle(dirty: &Notify, start: Instant, debounce: Duration, max_delay: Duration) {
    loop {
        let remaining = max_delay.saturating_sub(start.elapsed());
        let wait = debounce.min(remaining);
        if wait.is_zero() || timeout(wait, dirty.notified()).await.is_err() {
            return;
        }
    }
}

/// Runs for the lifetime of the app.
pub async fn run<R: Runtime>(
    app: AppHandle<R>,
    server: Arc<Mutex<Option<LocalServer>>>,
    dirty: Arc<Notify>,
) {
    let mut periodic = interval(PERIODIC);
    periodic.set_missed_tick_behavior(MissedTickBehavior::Delay);
    periodic.tick().await; // the first tick fires immediately
    loop {
        tokio::select! {
            () = dirty.notified() => {
                settle(&dirty, Instant::now(), DEBOUNCE, MAX_DELAY).await;
                // Errors are logged and reported to the UI by flush_open; the queue is kept.
                let _ = flush_open(&app, &server).await;
            }
            _ = periodic.tick() => {
                let mut guard = server.lock().await;
                if let Some(open) = guard.as_mut() {
                    match open.flush_and_checkpoint().await {
                        Ok(()) => emit_save_status(&app, false, None),
                        Err(e) => {
                            tracing::error!(error = %e, "periodic save failed");
                            emit_save_status(&app, true, Some(e.message));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn settles_after_quiet_period() {
        let dirty = Notify::new();
        let start = Instant::now();
        settle(&dirty, start, DEBOUNCE, MAX_DELAY).await;
        assert_eq!(start.elapsed(), DEBOUNCE);
    }

    #[tokio::test(start_paused = true)]
    async fn continuous_changes_are_capped_by_max_delay() {
        let dirty = Arc::new(Notify::new());
        let pinger = dirty.clone();
        let ping = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                pinger.notify_one();
            }
        });
        let start = Instant::now();
        settle(&dirty, start, DEBOUNCE, MAX_DELAY).await;
        ping.abort();
        assert!(start.elapsed() >= MAX_DELAY && start.elapsed() < MAX_DELAY + DEBOUNCE);
    }
}
