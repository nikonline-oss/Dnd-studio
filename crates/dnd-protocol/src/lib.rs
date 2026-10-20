//! Wire protocol: envelope, Intent/Event/Snapshot, error codes (serde + specta).
//!
//! This crate is the root of the dependency graph and must stay free of I/O, async runtimes and
//! Tauri, so the same types serve the desktop app, the session server and a future web client.
//! See `docs/protocol.md`.

pub mod error;
pub mod event;
pub mod ids;
pub mod intent;
pub mod message;
pub mod model;
pub mod snapshot;
pub mod validate;

#[cfg(feature = "arbitrary")]
pub mod arbitrary;

pub use error::RejectReason;
pub use event::{Event, EventBatch};
pub use ids::{
    AssetId, CharacterId, IntentId, MapId, MessageId, Role, Seq, SessionId, TokenId, UserId,
    Version,
};
pub use intent::{Intent, IntentEnvelope, IntentOutcome, VersionPolicy};
pub use message::{ClientMessage, Envelope, Rejection, ServerMessage};
pub use model::{CampaignMeta, CharacterRef, Grid, GridKind, MapDto, TokenDto, TokenLayer};
pub use snapshot::Snapshot;
/// Re-exported so dependants use the exact same UUID type.
pub use uuid::Uuid;
pub use validate::{MAX_SAFE_INTEGER, is_js_safe, is_uuid_v7};

/// Protocol version. Peers with a different version are refused with
/// [`RejectReason::ProtocolMismatch`].
pub const PROTOCOL_VERSION: u32 = 1;

/// Registers every protocol type that is exported to TypeScript.
#[cfg(feature = "specta")]
#[must_use]
pub fn register_types(types: specta::Types) -> specta::Types {
    types
        .register::<ClientMessage>()
        .register::<ServerMessage>()
        .register::<IntentOutcome>()
        .register::<Role>()
}
