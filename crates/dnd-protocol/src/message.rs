//! The message envelope and the client/server message sets (CLAUDE.md §7).

use serde::{Deserialize, Serialize};

use crate::PROTOCOL_VERSION;
use crate::error::RejectReason;
use crate::event::EventBatch;
use crate::ids::{IntentId, MessageId, Seq, SessionId, UserId};
use crate::intent::IntentEnvelope;
use crate::snapshot::Snapshot;

/// `{ v, id, type, ts, seq, session_id, sender, payload }`.
///
/// `type` and `payload` come from the flattened body (`ClientMessage` / `ServerMessage`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<B> {
    /// Protocol version of the sender.
    pub v: u32,
    pub id: MessageId,
    /// Unix time in milliseconds (sender's clock, informational only).
    pub ts: i64,
    /// Server-assigned sequence number of the last event the message reflects, if any.
    pub seq: Option<Seq>,
    pub session_id: Option<SessionId>,
    pub sender: Option<UserId>,
    #[serde(flatten)]
    pub body: B,
}

impl<B> Envelope<B> {
    /// Wraps `body` with the current protocol version.
    #[must_use]
    pub fn new(id: MessageId, ts: i64, body: B) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            ts,
            seq: None,
            session_id: None,
            sender: None,
            body,
        }
    }

    /// Checks the sender's protocol version against ours.
    pub fn check_version(&self) -> Result<(), RejectReason> {
        if self.v == PROTOCOL_VERSION {
            Ok(())
        } else {
            Err(RejectReason::ProtocolMismatch {
                server: PROTOCOL_VERSION,
                client: self.v,
            })
        }
    }
}

/// Client → server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ClientMessage {
    Intent(IntentEnvelope),
    Ping,
    Resync { last_seq: Seq },
}

/// Server → client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ServerMessage {
    Events(EventBatch),
    Snapshot(Snapshot),
    Rejected(Rejection),
    Pong,
}

/// An intent was not applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Rejection {
    pub intent_id: IntentId,
    pub reason: RejectReason,
}
