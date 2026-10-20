//! Durability housekeeping (ADR-0014). Events are written through on every accepted intent, so
//! nothing here saves game data; it only moves the WAL into the main file every 5 minutes so the
//! `-wal` file stays small and an external copy of `campaign.sqlite` is close to current.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{MissedTickBehavior, interval};

use crate::local_server::LocalServer;

pub const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Runs for the lifetime of the app.
pub async fn run(server: Arc<Mutex<Option<LocalServer>>>) {
    let mut ticks = interval(CHECKPOINT_INTERVAL);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticks.tick().await; // the first tick fires immediately
    loop {
        ticks.tick().await;
        if let Some(open) = server.lock().await.as_ref()
            && let Err(e) = open.checkpoint().await
        {
            tracing::warn!(error = %e, "WAL checkpoint failed");
        }
    }
}
