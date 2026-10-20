//! Intents: requests to change the game state. Only `dnd-game-core` decides whether they apply.

use serde::{Deserialize, Serialize};

use crate::ids::{AssetId, CharacterId, IntentId, MapId, TokenId, Version};
use crate::model::{Grid, TokenLayer};

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
}

/// What kind of version handling an intent needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionPolicy {
    /// Creates a new entity; no version to compare.
    Create,
    /// Modifies or deletes an existing entity; `expected_version` must match.
    Checked,
    /// Last-write-wins (token positions).
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
            Self::CreateToken { .. } => "create_token",
            Self::UpdateToken { .. } => "update_token",
            Self::MoveToken { .. } => "move_token",
            Self::DeleteToken { .. } => "delete_token",
        }
    }

    /// How `expected_version` is treated for this intent.
    #[must_use]
    pub fn version_policy(&self) -> VersionPolicy {
        match self {
            Self::CreateMap { .. } | Self::CreateToken { .. } => VersionPolicy::Create,
            Self::MoveToken { .. } => VersionPolicy::LastWriteWins,
            Self::UpdateCampaignMeta { .. }
            | Self::UpdateMap { .. }
            | Self::DeleteMap { .. }
            | Self::UpdateToken { .. }
            | Self::DeleteToken { .. } => VersionPolicy::Checked,
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
