//! Offline synchronization of a hosted campaign (ADR-0004, ADR-0055).
//!
//! One explicit machine and one classification decide what may happen while the GM's own session
//! is unreachable. The point of both is that nothing is guessed: a command either has a durable
//! place in the replay queue, belongs to the transport's short-lived queue, or is refused with a
//! message the GM can act on.
//!
//! The machine:
//!
//! ```text
//! Offline ──connect──▶ Synced
//! Synced ──lost──────▶ Reconnecting(1)
//! Reconnecting(n) ──lost──▶ Reconnecting(n+1)
//! Reconnecting(n) ──connect──▶ Replaying(total) | Synced (queue empty)
//! Replaying ──all accepted──▶ Synced
//! Replaying ──rejected──────▶ Conflict(pending)
//! Replaying ──lost──────────▶ Reconnecting(n+1)   (the queue is untouched)
//! Conflict ──resolved───────▶ Replaying(total) | Synced
//! any ──closed by the server──▶ Offline (no reconnect)
//! ```

use dnd_protocol::Intent;

/// What the GM's copy is doing relative to the authoritative session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPhase {
    /// There is no reachable session: nothing can be sent.
    Offline,
    /// The session is being reconnected; `attempt` grows on each failure.
    Reconnecting { attempt: u32 },
    /// Connected and replaying the durable queue; `total` was the queue length when it started.
    Replaying { total: usize },
    /// Replay stopped on a rejected edit; later edits wait for the GM's decision.
    Conflict { pending: usize },
    /// Connected, nothing left to replay.
    Synced,
}

impl SyncPhase {
    #[must_use]
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Replaying { .. } | Self::Synced)
    }
}

/// The machine itself. Kept free of I/O so every transition is testable on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncMachine {
    phase: SyncPhase,
}

impl SyncMachine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: SyncPhase::Offline,
        }
    }

    #[must_use]
    pub fn phase(&self) -> SyncPhase {
        self.phase
    }

    /// The connection came up. A non-empty durable queue means replay starts; the caller reports
    /// its length before sending the first edit.
    pub fn connected(&mut self, pending: usize) {
        self.phase = if pending > 0 {
            SyncPhase::Replaying { total: pending }
        } else {
            SyncPhase::Synced
        };
    }

    /// The connection was lost. A replay that was in progress is not cancelled: its operations are
    /// still on disk, and the next connection replays them by the same intent ids.
    pub fn lost(&mut self, attempt: u32) {
        self.phase = SyncPhase::Reconnecting { attempt };
    }

    /// A durable edit was rejected by the authority. Later edits keep their order and wait.
    pub fn conflict(&mut self, pending: usize) {
        self.phase = SyncPhase::Conflict { pending };
    }

    /// The GM resolved the conflict (applied their edit on top or discarded it).
    pub fn resolved(&mut self, pending: usize) {
        self.connected(pending);
    }

    /// The server closed the session for good (stopped, expired): reconnecting cannot help.
    pub fn closed(&mut self) {
        self.phase = SyncPhase::Offline;
    }
}

impl Default for SyncMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether an intent may be executed while the session is unreachable (ADR-0004, ADR-0055).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfflineClass {
    /// A durable GM edit: written to `pending_ops` with its original envelope (including
    /// `expected_version`) and replayed in order. The result shown locally is a projection, not
    /// the authority's answer.
    Queued,
    /// Allowed only through a live connection; the transport keeps at most a few minutes of chat
    /// and token movement and drops it afterwards. Never silently replayed later.
    Live,
    /// Refused offline. The reason names what is missing — a roll needs the authority's
    /// randomness, a resolution needs the decision that is still pending on the server.
    Refused(&'static str),
}

/// Classifies an intent for offline use.
///
/// The rule behind the table: an edit that the GM alone decides is queued; a message that only
/// needs ordering is left to the transport; anything whose outcome depends on the authority's
/// randomness or on a shared pending decision is refused. Permissions are *not* re-checked here —
/// the core decides on replay, with the same actor and the same envelope.
#[must_use]
pub fn classify(intent: &Intent) -> OfflineClass {
    use OfflineClass::{Live, Queued, Refused};
    match intent {
        // Ordered, not authored: the transport may hold them briefly, the queue never does.
        Intent::SendMessage { .. } | Intent::MoveToken { .. } => Live,

        // The outcome is a roll or a resolution of a decision that is pending elsewhere. Executing
        // them locally would present local randomness or an outdated decision as final.
        Intent::RollDice { .. }
        | Intent::RollInitiative { .. }
        | Intent::FireTrigger { .. }
        | Intent::ExecutePluginAction { .. }
        | Intent::MoveTokenWithMode { .. }
        | Intent::ResolvePluginReaction { .. }
        | Intent::CancelPluginMovement { .. }
        | Intent::ResolveActionReaction { .. }
        | Intent::ContinuePluginAction { .. }
        | Intent::CancelActionStack { .. }
        | Intent::ResolveActionSave { .. }
        | Intent::CancelActionSaves { .. }
        | Intent::EndMaintainedEffect { .. } => {
            Refused("the outcome depends on the authority's roll or on a pending decision")
        }
        // Whose turn it is belongs to the table, not to one client's copy.
        Intent::NextTurn { .. } | Intent::EndCombat { .. } => {
            Refused("turn order is shared and changes on the server")
        }

        // Everything else the GM decides alone: maps, tokens, characters, journal, items, boards,
        // zones, triggers, plugins, campaign metadata and the fictional clock.
        _ => Queued,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_protocol::*;

    #[test]
    fn a_full_cycle_reaches_synced() {
        let mut machine = SyncMachine::new();
        assert_eq!(machine.phase(), SyncPhase::Offline);
        machine.connected(0);
        assert_eq!(machine.phase(), SyncPhase::Synced);
        machine.lost(1);
        assert_eq!(machine.phase(), SyncPhase::Reconnecting { attempt: 1 });
        machine.lost(2);
        assert_eq!(machine.phase(), SyncPhase::Reconnecting { attempt: 2 });
        machine.connected(3);
        assert_eq!(machine.phase(), SyncPhase::Replaying { total: 3 });
        machine.resolved(0);
        assert_eq!(machine.phase(), SyncPhase::Synced);
    }

    #[test]
    fn a_rejected_edit_holds_later_ones_until_the_gm_decides() {
        let mut machine = SyncMachine::new();
        machine.connected(2);
        machine.conflict(2);
        assert_eq!(machine.phase(), SyncPhase::Conflict { pending: 2 });
        // Still conflicting after another failed connection: the conflict is on disk, not in RAM.
        machine.lost(3);
        assert_eq!(machine.phase(), SyncPhase::Reconnecting { attempt: 3 });
        machine.connected(2);
        assert_eq!(machine.phase(), SyncPhase::Replaying { total: 2 });
        machine.resolved(1);
        assert_eq!(machine.phase(), SyncPhase::Replaying { total: 1 });
        machine.resolved(0);
        assert_eq!(machine.phase(), SyncPhase::Synced);
    }

    #[test]
    fn a_stopped_session_is_offline_not_reconnecting() {
        let mut machine = SyncMachine::new();
        machine.connected(1);
        machine.closed();
        assert_eq!(machine.phase(), SyncPhase::Offline);
        assert!(!machine.phase().is_online());
    }

    fn create_map() -> Intent {
        Intent::CreateMap {
            id: MapId::new_v7(),
            name: "Cave".into(),
            description: String::new(),
            background_asset_id: None,
            width: 4000,
            height: 3000,
            grid: Grid::default(),
            visible_to_players: true,
        }
    }

    #[test]
    fn gm_edits_are_queued_and_rolls_are_refused() {
        assert_eq!(classify(&create_map()), OfflineClass::Queued);
        assert_eq!(
            classify(&Intent::UpdateCampaignMeta {
                name: Some("Renamed".into()),
                description: None,
                token_inspector: None,
                item_states: None,
                visibility: None,
            }),
            OfflineClass::Queued
        );
        assert_eq!(
            classify(&Intent::CreateJournalEntry {
                id: JournalEntryId::new_v7(),
                folder_id: None,
                title: "Notes".into(),
                markdown: "text".into(),
                visibility: JournalVisibility::Gm,
                players_can_edit: false,
                table: None,
            }),
            OfflineClass::Queued
        );

        for refused in [
            Intent::RollDice {
                id: ChatMessageId::new_v7(),
                expression: "1d20".into(),
                hidden: false,
                character_id: None,
                roll: None,
            },
            Intent::RollInitiative {
                id: CombatantId::new_v7(),
            },
            Intent::NextTurn {
                combat_id: CombatId::new_v7(),
            },
            Intent::EndCombat {
                id: CombatId::new_v7(),
            },
        ] {
            assert!(
                matches!(classify(&refused), OfflineClass::Refused(_)),
                "{refused:?} must not run offline"
            );
        }
    }

    #[test]
    fn chat_and_plain_movement_use_the_transport_queue_not_the_durable_one() {
        assert_eq!(
            classify(&Intent::SendMessage {
                id: ChatMessageId::new_v7(),
                body: "hello".into(),
            }),
            OfflineClass::Live
        );
        assert_eq!(
            classify(&Intent::MoveToken {
                id: TokenId::new_v7(),
                x: 32,
                y: 32
            }),
            OfflineClass::Live
        );
    }
}
