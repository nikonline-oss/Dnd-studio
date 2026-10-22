//! Intents: requests to change the game state. Only `dnd-game-core` decides whether they apply.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AssetId, CharacterId, ChatMessageId, IntentId, JournalEntryId, MapId, TokenId, UserId, Version,
};
use crate::model::{AssetDto, CharacterKind, FogOp, Grid, JournalVisibility, TokenLayer};

/// An intent together with its correlation id and the version it was based on (ADR-0003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IntentEnvelope {
    pub intent_id: IntentId,
    /// Required for intents that modify or delete an existing entity, except `move_token`
    /// (last-write-wins). Ignored for creations.
    pub expected_version: Option<Version>,
    pub intent: Intent,
}

/// A request to change state. Update intents carry only the fields to change (`null` = keep).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Intent {
    UpdateCampaignMeta {
        name: Option<String>,
        description: Option<String>,
    },

    CreateMap {
        id: MapId,
        name: String,
        background_asset_id: Option<AssetId>,
        /// Logical size (ADR-0016), `1..=MAX_MAP_SIZE`; fixed for the life of the map.
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        width: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        height: i64,
        grid: Grid,
        visible_to_players: bool,
    },
    UpdateMap {
        id: MapId,
        name: Option<String>,
        grid: Option<Grid>,
        visible_to_players: Option<bool>,
    },
    DeleteMap {
        id: MapId,
    },
    /// GM / Co-GM. Appends a reveal/hide operation to the map's fog (ADR-0019). The shape must
    /// lie on the map; `op.id` is a new UUIDv7. Does not change the map version.
    ApplyFog {
        map_id: MapId,
        op: FogOp,
    },
    /// GM / Co-GM. Drops every fog operation: the whole map becomes covered or clear.
    ResetFog {
        map_id: MapId,
        covered: bool,
    },

    CreateToken {
        id: TokenId,
        map_id: MapId,
        character_id: Option<CharacterId>,
        asset_id: Option<AssetId>,
        name: String,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        rotation: f64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        scale: f64,
        layer: TokenLayer,
        visible: bool,
    },
    UpdateToken {
        id: TokenId,
        name: Option<String>,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        rotation: Option<f64>,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        scale: Option<f64>,
        layer: Option<TokenLayer>,
        visible: Option<bool>,
    },
    /// Last-write-wins: no version check, does not bump the token version (ADR-0003).
    MoveToken {
        id: TokenId,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
    },
    DeleteToken {
        id: TokenId,
    },

    /// Players create their own `pc` in `draft` (owner = themselves); GM / Co-GM create any
    /// character directly as `active`.
    CreateCharacter {
        id: CharacterId,
        character_kind: CharacterKind,
        name: String,
        owner: Option<UserId>,
        sheet_json: String,
    },
    UpdateCharacter {
        id: CharacterId,
        name: Option<String>,
        sheet_json: Option<String>,
    },
    /// draft → pending_approval (owner or GM).
    SubmitCharacter {
        id: CharacterId,
    },
    /// pending_approval → active (GM / Co-GM).
    ApproveCharacter {
        id: CharacterId,
    },
    /// pending_approval → draft, back to the player for changes (GM / Co-GM).
    ReturnCharacter {
        id: CharacterId,
    },
    /// any → archived (GM / Co-GM).
    ArchiveCharacter {
        id: CharacterId,
    },
    /// GM / Co-GM; `owner` must be a known user.
    AssignCharacterOwner {
        id: CharacterId,
        owner: Option<UserId>,
    },

    /// GM / Co-GM. Placed last in its folder.
    CreateJournalEntry {
        id: JournalEntryId,
        folder: String,
        title: String,
        markdown: String,
        visibility: JournalVisibility,
        players_can_edit: bool,
    },
    /// GM / Co-GM change anything; players only `title`/`markdown` of entries that are visible to
    /// them and marked `players_can_edit`.
    UpdateJournalEntry {
        id: JournalEntryId,
        title: Option<String>,
        markdown: Option<String>,
        visibility: Option<JournalVisibility>,
        players_can_edit: Option<bool>,
    },
    /// GM / Co-GM: move to `folder` at position `sort_order`.
    MoveJournalEntry {
        id: JournalEntryId,
        folder: String,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        sort_order: i64,
    },
    /// GM / Co-GM.
    DeleteJournalEntry {
        id: JournalEntryId,
    },

    /// GM / Co-GM. Records an image the server-side import already validated, converted and
    /// stored; the same original (`sha256`) cannot be registered twice.
    RegisterAsset {
        /// Boxed: by far the largest intent payload (keeps `Intent` small).
        asset: Box<AssetDto>,
    },

    /// Everyone except spectators.
    SendMessage {
        id: ChatMessageId,
        body: String,
    },
    /// Everyone except spectators. `hidden` (seen by GM roles only) is for GM and Co-GM. Field
    /// references (`@str_mod`) read the sheet of `character_id`, which a player must own.
    RollDice {
        id: ChatMessageId,
        expression: String,
        hidden: bool,
        character_id: Option<CharacterId>,
    },
}

/// What kind of version handling an intent needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionPolicy {
    /// Creates a new entity; no version to compare.
    Create,
    /// Modifies or deletes an existing entity; `expected_version` must match.
    Checked,
    /// Last-write-wins (token positions, fog reset); does not bump versions.
    LastWriteWins,
}

impl Intent {
    /// Stable snake_case name, as in the JSON `kind` tag.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UpdateCampaignMeta { .. } => "update_campaign_meta",
            Self::CreateMap { .. } => "create_map",
            Self::UpdateMap { .. } => "update_map",
            Self::DeleteMap { .. } => "delete_map",
            Self::ApplyFog { .. } => "apply_fog",
            Self::ResetFog { .. } => "reset_fog",
            Self::CreateToken { .. } => "create_token",
            Self::UpdateToken { .. } => "update_token",
            Self::MoveToken { .. } => "move_token",
            Self::DeleteToken { .. } => "delete_token",
            Self::CreateCharacter { .. } => "create_character",
            Self::UpdateCharacter { .. } => "update_character",
            Self::SubmitCharacter { .. } => "submit_character",
            Self::ApproveCharacter { .. } => "approve_character",
            Self::ReturnCharacter { .. } => "return_character",
            Self::ArchiveCharacter { .. } => "archive_character",
            Self::AssignCharacterOwner { .. } => "assign_character_owner",
            Self::CreateJournalEntry { .. } => "create_journal_entry",
            Self::UpdateJournalEntry { .. } => "update_journal_entry",
            Self::MoveJournalEntry { .. } => "move_journal_entry",
            Self::DeleteJournalEntry { .. } => "delete_journal_entry",
            Self::RegisterAsset { .. } => "register_asset",
            Self::SendMessage { .. } => "send_message",
            Self::RollDice { .. } => "roll_dice",
        }
    }

    /// How `expected_version` is treated for this intent.
    #[must_use]
    pub fn version_policy(&self) -> VersionPolicy {
        match self {
            Self::CreateMap { .. }
            | Self::CreateToken { .. }
            | Self::CreateCharacter { .. }
            | Self::CreateJournalEntry { .. }
            | Self::SendMessage { .. }
            | Self::RollDice { .. }
            | Self::RegisterAsset { .. }
            | Self::ApplyFog { .. } => VersionPolicy::Create,
            Self::MoveToken { .. } | Self::ResetFog { .. } => VersionPolicy::LastWriteWins,
            Self::UpdateCampaignMeta { .. }
            | Self::UpdateMap { .. }
            | Self::DeleteMap { .. }
            | Self::UpdateToken { .. }
            | Self::DeleteToken { .. }
            | Self::UpdateCharacter { .. }
            | Self::SubmitCharacter { .. }
            | Self::ApproveCharacter { .. }
            | Self::ReturnCharacter { .. }
            | Self::ArchiveCharacter { .. }
            | Self::AssignCharacterOwner { .. }
            | Self::UpdateJournalEntry { .. }
            | Self::MoveJournalEntry { .. }
            | Self::DeleteJournalEntry { .. } => VersionPolicy::Checked,
        }
    }

    /// The character a character intent targets.
    #[must_use]
    pub fn character_id(&self) -> Option<CharacterId> {
        match self {
            Self::CreateCharacter { id, .. }
            | Self::UpdateCharacter { id, .. }
            | Self::SubmitCharacter { id }
            | Self::ApproveCharacter { id }
            | Self::ReturnCharacter { id }
            | Self::ArchiveCharacter { id }
            | Self::AssignCharacterOwner { id, .. } => Some(*id),
            _ => None,
        }
    }
}

/// Result of submitting an intent. A rejection is an expected outcome, not an error.
/// A repeated `intent_id` yields the original outcome again (ADR-0015).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum IntentOutcome {
    Accepted {
        /// Sequence number of the last event produced.
        seq: crate::ids::Seq,
    },
    Rejected {
        reason: crate::error::RejectReason,
    },
}

/// SHA-256 of an intent's payload (`expected_version` + `intent`), used to tell a genuine repeat
/// of an `intent_id` from a reuse of the id with different data (ADR-0015).
///
/// Persisted in `event_log`, so the canonical form (compact JSON of the tuple) must never change;
/// a snapshot test guards it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PayloadHash(pub [u8; 32]);

impl PayloadHash {
    /// Lowercase hex, 64 characters.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write;
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    /// Parses [`to_hex`](Self::to_hex) output.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl IntentEnvelope {
    /// Hash of everything except `intent_id`.
    #[must_use]
    pub fn payload_hash(&self) -> PayloadHash {
        use sha2::{Digest, Sha256};
        // Serializing protocol types cannot fail (no maps with non-string keys, no custom
        // serializers); an empty payload would still hash deterministically.
        let canonical =
            serde_json::to_vec(&(&self.expected_version, &self.intent)).unwrap_or_default();
        PayloadHash(Sha256::digest(&canonical).into())
    }
}
