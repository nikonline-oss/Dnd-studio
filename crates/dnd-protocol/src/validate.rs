//! Structural validation of incoming messages (ADR-0015). Runs before any game rule, on the
//! session server and on the desktop's local server alike (via `dnd_game_core::apply`).
//!
//! Checks shape only: no rights are ever derived from an id, and existence/collisions are decided
//! by the core against the current state.

use uuid::{Uuid, Variant};

use crate::error::RejectReason;
use crate::ids::{Seq, Version};
use crate::intent::{Intent, IntentEnvelope};
use crate::message::{ClientMessage, Envelope};

/// `Number.MAX_SAFE_INTEGER`: the largest integer a JS client represents exactly (2^53 − 1).
pub const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

/// Whether `value` survives a round trip through a JS `number` (|value| ≤ 2^53 − 1).
#[must_use]
pub fn is_js_safe(value: i64) -> bool {
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value)
}

/// RFC 9562 version 7 (time-ordered) UUID with the standard variant.
#[must_use]
pub fn is_uuid_v7(id: &Uuid) -> bool {
    id.get_version_num() == 7 && id.get_variant() == Variant::RFC4122
}

fn js_safe(value: i64, field: &str) -> Result<(), RejectReason> {
    if is_js_safe(value) {
        Ok(())
    } else {
        Err(RejectReason::invalid(field))
    }
}

fn uuid_v7(id: &Uuid, field: &str) -> Result<(), RejectReason> {
    if is_uuid_v7(id) {
        Ok(())
    } else {
        Err(RejectReason::invalid(field))
    }
}

impl Version {
    /// The next version, or `None` once it would leave the JS-safe range.
    #[must_use]
    pub fn checked_next(self) -> Option<Self> {
        let next = self.0.checked_add(1)?;
        is_js_safe(next).then_some(Self(next))
    }
}

impl Seq {
    /// The next sequence number, or `None` once it would leave the JS-safe range.
    #[must_use]
    pub fn checked_next(self) -> Option<Self> {
        let next = self.0.checked_add(1)?;
        is_js_safe(next).then_some(Self(next))
    }
}

impl Intent {
    /// Ids of entities this intent creates. They must be client-generated UUIDv7 (ADR-0007).
    #[must_use]
    pub fn created_id(&self) -> Option<Uuid> {
        match self {
            Self::CreateMap { id, .. } => Some(id.0),
            Self::CreateToken { id, .. } => Some(id.0),
            Self::UpdateCampaignMeta { .. }
            | Self::UpdateMap { .. }
            | Self::DeleteMap { .. }
            | Self::UpdateToken { .. }
            | Self::MoveToken { .. }
            | Self::DeleteToken { .. } => None,
        }
    }
}

impl IntentEnvelope {
    /// `intent_id` and created ids are UUIDv7; every integer is JS-safe.
    pub fn validate(&self) -> Result<(), RejectReason> {
        uuid_v7(&self.intent_id.0, "intent_id")?;
        if let Some(version) = self.expected_version {
            js_safe(version.0, "expected_version")?;
        }
        if let Some(id) = self.intent.created_id() {
            uuid_v7(&id, "id")?;
        }
        Ok(())
    }
}

impl ClientMessage {
    pub fn validate(&self) -> Result<(), RejectReason> {
        match self {
            Self::Intent(envelope) => envelope.validate(),
            Self::Ping => Ok(()),
            Self::Resync { last_seq } => js_safe(last_seq.0, "last_seq"),
        }
    }
}

impl Envelope<ClientMessage> {
    /// Protocol version, header fields and the body.
    pub fn validate(&self) -> Result<(), RejectReason> {
        self.check_version()?;
        uuid_v7(&self.id.0, "id")?;
        js_safe(self.ts, "ts")?;
        if let Some(seq) = self.seq {
            js_safe(seq.0, "seq")?;
        }
        self.body.validate()
    }
}
