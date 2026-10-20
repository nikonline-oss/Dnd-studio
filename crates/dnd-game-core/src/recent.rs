//! Idempotency of intents (ADR-0015): `intent_id` is the key. A repeated intent — after a
//! reconnect, or when replaying `pending_ops` (ADR-0004) — gets the original outcome back and is
//! never applied twice, as long as it is inside the window.
//!
//! The core stays pure: the caller passes the current time in, like the RNG.

use std::collections::{HashMap, VecDeque};

use dnd_protocol::IntentId;

/// How much history is remembered: the last `max_count` intents that are also younger than
/// `max_age_ms` — whichever is smaller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentWindow {
    pub max_count: usize,
    pub max_age_ms: i64,
}

impl IntentWindow {
    /// 10 000 intents or 24 hours.
    pub const DEFAULT: Self = Self {
        max_count: 10_000,
        max_age_ms: 24 * 60 * 60 * 1000,
    };

    /// Oldest timestamp still inside the window at `now_ms` (inclusive).
    #[must_use]
    pub fn since_ms(&self, now_ms: i64) -> i64 {
        now_ms.saturating_sub(self.max_age_ms).saturating_add(1)
    }

    fn contains(&self, at_ms: i64, now_ms: i64) -> bool {
        at_ms >= self.since_ms(now_ms)
    }
}

impl Default for IntentWindow {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Debug)]
struct Entry<T> {
    outcome: T,
    at_ms: i64,
}

/// Bounded memory of recent intent outcomes; the oldest entries are forgotten first.
#[derive(Clone, Debug)]
pub struct RecentIntents<T> {
    window: IntentWindow,
    entries: HashMap<IntentId, Entry<T>>,
    /// Insertion order, used for eviction.
    order: VecDeque<IntentId>,
}

impl<T> Default for RecentIntents<T> {
    fn default() -> Self {
        Self::new(IntentWindow::DEFAULT)
    }
}

impl<T> RecentIntents<T> {
    #[must_use]
    pub fn new(window: IntentWindow) -> Self {
        Self {
            window: IntentWindow {
                max_count: window.max_count.max(1),
                max_age_ms: window.max_age_ms.max(1),
            },
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn window(&self) -> IntentWindow {
        self.window
    }

    /// The outcome recorded for `id`, if it is still inside the window at `now_ms`.
    #[must_use]
    pub fn get(&self, id: &IntentId, now_ms: i64) -> Option<&T> {
        self.entries
            .get(id)
            .filter(|e| self.window.contains(e.at_ms, now_ms))
            .map(|e| &e.outcome)
    }

    /// Records the outcome of `id` at `at_ms`. The first outcome wins: recording a remembered id
    /// again is a no-op. Expired and excess entries are dropped.
    pub fn insert(&mut self, id: IntentId, outcome: T, at_ms: i64) {
        self.prune(at_ms);
        if self.entries.contains_key(&id) {
            return;
        }
        while self.order.len() >= self.window.max_count {
            self.evict_oldest();
        }
        self.order.push_back(id);
        self.entries.insert(id, Entry { outcome, at_ms });
    }

    /// Drops entries that fell out of the window at `now_ms`.
    pub fn prune(&mut self, now_ms: i64) {
        while let Some(oldest) = self.order.front()
            && self
                .entries
                .get(oldest)
                .is_none_or(|e| !self.window.contains(e.at_ms, now_ms))
        {
            self.evict_oldest();
        }
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest) = self.order.pop_front() {
            self.entries.remove(&oldest);
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnd_protocol::arbitrary::v7;

    fn id(n: u128) -> IntentId {
        IntentId(v7(n))
    }

    fn window(max_count: usize, max_age_ms: i64) -> IntentWindow {
        IntentWindow {
            max_count,
            max_age_ms,
        }
    }

    #[test]
    fn count_limit_evicts_oldest() {
        let mut recent = RecentIntents::new(window(2, 1_000));
        recent.insert(id(1), "a", 0);
        recent.insert(id(2), "b", 0);
        recent.insert(id(3), "c", 0);
        assert_eq!(recent.get(&id(1), 0), None);
        assert_eq!(recent.get(&id(2), 0), Some(&"b"));
        assert_eq!(recent.get(&id(3), 0), Some(&"c"));
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn age_limit_boundary_is_exact() {
        let mut recent = RecentIntents::new(window(10, 1_000));
        recent.insert(id(1), "a", 5_000);
        // Inside the window up to (and including) 999 ms later…
        assert_eq!(recent.get(&id(1), 5_999), Some(&"a"));
        // …and forgotten at exactly max_age.
        assert_eq!(recent.get(&id(1), 6_000), None);
        assert_eq!(IntentWindow::DEFAULT.since_ms(10), 10 - 86_400_000 + 1);
    }

    #[test]
    fn whichever_limit_is_smaller_wins() {
        // Count is the tighter limit.
        let mut recent = RecentIntents::new(window(1, 1_000_000));
        recent.insert(id(1), "a", 0);
        recent.insert(id(2), "b", 1);
        assert_eq!(recent.get(&id(1), 1), None);
        // Age is the tighter limit.
        let mut recent = RecentIntents::new(window(100, 10));
        recent.insert(id(1), "a", 0);
        recent.insert(id(2), "b", 20);
        assert_eq!(recent.len(), 1, "expired entry pruned on insert");
        assert_eq!(recent.get(&id(2), 20), Some(&"b"));
    }

    #[test]
    fn first_outcome_wins_and_expired_id_is_new_again() {
        let mut recent = RecentIntents::new(window(4, 100));
        recent.insert(id(1), "accepted", 0);
        recent.insert(id(1), "something else", 50);
        assert_eq!(recent.get(&id(1), 50), Some(&"accepted"));
        assert_eq!(recent.len(), 1);
        recent.insert(id(1), "later", 200);
        assert_eq!(recent.get(&id(1), 200), Some(&"later"));
    }
}
