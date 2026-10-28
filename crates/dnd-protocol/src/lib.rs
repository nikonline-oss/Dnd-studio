//! Wire protocol: envelope, Intent/Event/Snapshot, error codes (serde + specta).
//!
//! This crate is the root of the dependency graph and must stay free of I/O, async runtimes and
//! Tauri, so the same types serve the desktop app, the session server and a future web client.
//! See `docs/protocol.md`.

pub mod board;
pub mod error;
pub mod event;
pub mod ids;
pub mod intent;
pub mod message;
pub mod model;
pub mod plugin;
pub mod sheet_schema;
mod snapshot;
pub mod trigger;
pub mod validate;
pub mod zone;

#[cfg(feature = "arbitrary")]
pub mod arbitrary;

pub use board::{
    BoardDto, BoardEdgeDto, BoardLink, BoardNodeContent, BoardNodeDto, BoardVisibility,
    MAX_BOARD_COORD, MAX_BOARD_EDGES, MAX_BOARD_NODES, MAX_NODE_SIZE, MAX_NOTE_LEN, MIN_NODE_SIZE,
};
pub use error::RejectReason;
pub use event::{Event, EventBatch};
pub use ids::{
    AssetId, BoardEdgeId, BoardId, BoardNodeId, CharacterId, ChatMessageId, CombatId, CombatantId,
    FogOpId, IntentId, JournalEntryId, JournalFolderId, MapId, MessageId, Role, Seq, SessionId,
    TokenId, TriggerId, UserId, Version, ZoneId, uuid_v7_from_random,
};
pub use intent::{Intent, IntentEnvelope, IntentOutcome, PayloadHash, VersionPolicy};
pub use message::{Acceptance, ClientMessage, Envelope, Rejection, ServerMessage};
pub use model::{
    AssetDto, AssetTileDto, AssetUsage, CHAT_SNAPSHOT_LEN, CampaignMeta, CharacterDto,
    CharacterKind, CharacterStatus, ChatKind, ChatMessageDto, ChatVisibility, CombatDto,
    CombatantDto, DiceRollDto, DiceTermDto, DieDto, FogDto, FogOp, FogOpKind, FogShape, Grid,
    GridKind, JournalEntryDto, JournalEntryMeta, JournalFolderDto, JournalVisibility, MAX_CHAT_LEN,
    MAX_CONDITIONS, MAX_JOURNAL_BYTES, MAX_SHEET_BYTES, MapDto, SystemMessage, TokenDto,
    TokenLayer, UserDto,
};
pub use model::{
    MAX_BRUSH_POINTS, MAX_BRUSH_RADIUS, MAX_CELL_SIZE, MAX_COMBATANTS, MAX_FOG_OPS, MAX_FOG_POINTS,
    MAX_FOLDER_DEPTH, MAX_MAP_SIZE, MIN_CELL_SIZE, UNITS16_PER_UNIT,
};
pub use plugin::{
    ConditionDef, InventoryDef, Label, LocalizedText, MAX_PLUGIN_BYTES, MAX_TRIGGER_TEMPLATES,
    PluginDto, PluginKind, ResourceDef, RollDef, SystemRulesDto, TemplateParam, TemplateParamKind,
    TriggerTemplateDto,
};
pub use snapshot::Snapshot;
pub use trigger::{
    MAX_TRIGGER_STEPS, MAX_TRIGGER_TEXT, MAX_TRIGGERS, MAX_TRIGGERS_PER_INTENT, RevealTarget,
    TriggerAction, TriggerDef, TriggerDto, TriggerEvent, TriggerFiredDto, TriggerStep,
};
/// Re-exported so dependants use the exact same UUID type.
pub use uuid::Uuid;
pub use validate::{
    MAX_SAFE_INTEGER, SheetFieldError, is_js_safe, is_uuid_v7, sheet_field, validate_sheet_json,
};
pub use zone::{MAX_ZONE_PARTS, MAX_ZONE_POINTS, MAX_ZONES_PER_MAP, ZoneDto, ZoneLabel, ZoneShape};

/// Protocol version. Peers with a different version are refused with
/// [`RejectReason::ProtocolMismatch`].
pub const PROTOCOL_VERSION: u32 = 5;

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
