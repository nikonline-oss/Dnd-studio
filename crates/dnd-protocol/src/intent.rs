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
        x: f64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: f64,
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
        x: f64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: f64,
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
