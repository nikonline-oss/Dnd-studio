//! Rejection reasons with stable machine-readable codes.

use serde::{Deserialize, Serialize};

use crate::ids::Version;

/// Why an intent or message was rejected. The `code` tag is stable and safe to match on in clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RejectReason {
    /// `expected_version` did not match; `current` is the entity's version now (ADR-0003).
    #[error("entity was changed concurrently (current version {current})")]
    StaleVersion { current: Version },
    /// A checked intent was sent without `expected_version`.
    #[error("expected_version is required for this intent")]
    MissingExpectedVersion,
    /// The actor's role does not allow this intent.
    #[error("not allowed for this role")]
    Forbidden,
    /// The referenced entity does not exist.
    #[error("entity not found")]
    NotFound,
    /// An entity with this id already exists.
    #[error("entity already exists")]
    AlreadyExists,
    /// The intent is malformed; `field` names the offending field.
    #[error("invalid value in field `{field}`")]
    Invalid { field: String },
    /// The `intent_id` was already used for an intent with different data. Any retry with changed
    /// data (e.g. a new `expected_version`) needs a new `intent_id` (ADR-0015).
    #[error("intent_id was already used for a different intent")]
    IntentIdReused,
    /// Client and server speak different protocol versions.
    #[error(
        "protocol version mismatch (server {server}, client {client}); update the client or the server"
    )]
    ProtocolMismatch { server: u32, client: u32 },
}

impl RejectReason {
    /// The stable code, as in the JSON `code` tag.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::StaleVersion { .. } => "stale_version",
            Self::MissingExpectedVersion => "missing_expected_version",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            Self::IntentIdReused => "intent_id_reused",
            Self::Invalid { .. } => "invalid",
            Self::ProtocolMismatch { .. } => "protocol_mismatch",
        }
    }

    /// Shorthand for [`RejectReason::Invalid`].
    #[must_use]
    pub fn invalid(field: &str) -> Self {
        Self::Invalid {
            field: field.to_owned(),
        }
    }
}
