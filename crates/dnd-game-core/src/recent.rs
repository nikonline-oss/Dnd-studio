//! Idempotency of intents (ADR-0015): `intent_id` is the key. A repeated intent — after a
//! reconnect, or when replaying `pending_ops` (ADR-0004) — gets the original outcome back and is
//! never applied twice.

use std::collections::{HashMap, VecDeque};

use dnd_protocol::IntentId;

/// Default number of remembered intents.
pub const RECENT_INTENTS_CAPACITY: usize = 4096;

/// Bounded memory of recent intent outcomes; the oldest entry is forgotten first.
#[derive(Clone, Debug)]
pub struct RecentIntents<T> {
    capacity: usize,
    outcomes: HashMap<IntentId, T>,
    order: VecDeque<IntentId>,
}

impl<T> Default for RecentIntents<T> {
    fn default() -> Self {
        Self::new(RECENT_INTENTS_CAPACITY)
    }
}

impl<T> RecentIntents<T> {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            outcomes: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// The outcome recorded for `id`, if it is still remembered.
    #[must_use]
    pub fn get(&self, id: &IntentId) -> Option<&T> {
        self.outcomes.get(id)
    }

    /// Records the outcome of `id`. The first outcome wins: recording an id again is a no-op.
    pub fn insert(&mut self, id: IntentId, outcome: T) {
        if self.outcomes.contains_key(&id) {
            return;
        }
        if self.order.len() == self.capacity
            && let Some(oldest) = self.order.pop_front()
        {
            self.outcomes.remove(&oldest);
        }
        self.order.push_back(id);
        self.outcomes.insert(id, outcome);
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

    #[test]
    fn remembers_and_evicts_oldest() {
        let mut recent = RecentIntents::new(2);
        recent.insert(id(1), "a");
        recent.insert(id(2), "b");
        recent.insert(id(3), "c");
        assert_eq!(recent.get(&id(1)), None);
        assert_eq!(recent.get(&id(2)), Some(&"b"));
        assert_eq!(recent.get(&id(3)), Some(&"c"));
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn first_outcome_wins() {
        let mut recent = RecentIntents::new(4);
        recent.insert(id(1), "accepted");
        recent.insert(id(1), "something else");
        assert_eq!(recent.get(&id(1)), Some(&"accepted"));
        assert_eq!(recent.len(), 1);
    }
}
