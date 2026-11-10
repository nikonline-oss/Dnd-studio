//! Crash tracking for the Safe Mode suggestion (ADR-0057).
//!
//! A run that ends without the graceful shutdown path leaves a counter incremented. Three such
//! runs in a row mean the app is probably failing during start-up or campaign loading, so the UI
//! offers to open the campaign without its rules. The counter is deliberately tiny and lives next
//! to the session list: it is a hint, not a diagnostic log.

use std::path::Path;

/// Consecutive unclean runs that trigger the suggestion.
pub const CRASH_HINT: u32 = 3;

const FILE: &str = "launch.json";

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Counter {
    #[serde(default)]
    unclean: u32,
}

/// Records that a run started and returns the number of consecutive unclean runs before it.
///
/// A missing or unreadable file counts as zero: a broken counter must not nag the user.
pub fn begin(dir: &Path) -> u32 {
    let path = dir.join(FILE);
    let counter: Counter = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let previous = counter.unclean;
    write(
        &path,
        Counter {
            unclean: previous.saturating_add(1),
        },
    );
    previous
}

/// Records a clean shutdown.
pub fn mark_clean(dir: &Path) {
    write(&dir.join(FILE), Counter::default());
}

fn write(path: &Path, counter: Counter) {
    if let Ok(text) = serde_json::to_string(&counter)
        && let Err(error) = std::fs::write(path, text)
    {
        tracing::warn!(%error, "launch counter could not be written");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn unclean_runs_accumulate_until_a_clean_exit() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(begin(dir.path()), 0);
        assert_eq!(begin(dir.path()), 1);
        assert_eq!(begin(dir.path()), 2);
        mark_clean(dir.path());
        assert_eq!(begin(dir.path()), 0);
    }

    #[test]
    fn a_broken_counter_does_not_nag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert_eq!(begin(dir.path()), 0);
        assert_eq!(begin(dir.path()), 1);
    }
}
