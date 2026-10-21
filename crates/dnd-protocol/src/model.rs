//! Plain data transfer objects shared by intents, events and snapshots.

use serde::{Deserialize, Serialize};

use crate::ids::{AssetId, CharacterId, MapId, TokenId, UserId, Version};

/// Campaign-wide metadata (singleton entity).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CampaignMeta {
    pub name: String,
    pub description: String,
    pub version: Version,
}

/// Grid geometry of a map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum GridKind {
    None,
    Square,
    HexFlat,
    HexPointy,
}

/// Grid settings of a map. Lengths are in logical map units (ADR-0016).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Grid {
    pub kind: GridKind,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub cell_size: f64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub offset_x: f64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub offset_y: f64,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            kind: GridKind::Square,
            cell_size: 70.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

/// A map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MapDto {
    pub id: MapId,
    pub name: String,
    pub background_asset_id: Option<AssetId>,
    /// Logical width (ADR-0016); fixed at creation. The background image is stretched onto it.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub width: i64,
    /// Logical height (ADR-0016).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub height: i64,
    pub grid: Grid,
    pub visible_to_players: bool,
    pub version: Version,
}

/// Rendering layer of a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TokenLayer {
    /// Below regular tokens (props, terrain).
    Background,
    /// Regular tokens.
    Tokens,
    /// Visible to GM / Co-GM only.
    Gm,
}

/// A token on a map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TokenDto {
    pub id: TokenId,
    pub map_id: MapId,
    pub character_id: Option<CharacterId>,
    pub asset_id: Option<AssetId>,
    pub name: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    /// Position in 1/16 logical map units (ADR-0016), `0..=width*16`.
    pub x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: i64,
    /// Degrees, clockwise.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub rotation: f64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub scale: f64,
    pub layer: TokenLayer,
    pub visible: bool,
    pub version: Version,
}

/// Player character or non-player character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CharacterKind {
    Pc,
    Npc,
}

/// Lifecycle of a character (CLAUDE.md §5): draft → pending_approval → active → archived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CharacterStatus {
    Draft,
    PendingApproval,
    Active,
    Archived,
}

/// A character. `sheet_json` is the raw sheet (a JSON object, integers only — ADR-0016) until
/// declarative sheets arrive with system plugins in stage 3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CharacterDto {
    pub id: CharacterId,
    pub kind: CharacterKind,
    pub name: String,
    pub owner: Option<UserId>,
    pub status: CharacterStatus,
    pub sheet_json: String,
    pub version: Version,
}

/// A user of the campaign. Read-only in stage 2a (users join through the session server, 2b).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct UserDto {
    pub id: UserId,
    pub name: String,
    pub role: crate::ids::Role,
}

/// Largest `sheet_json`, in bytes.
pub const MAX_SHEET_BYTES: usize = 256 * 1024;

/// Positions are stored in 1/16 of a logical map unit (ADR-0016).
pub const UNITS16_PER_UNIT: i64 = 16;

/// Largest map width/height in logical units (ADR-0016).
pub const MAX_MAP_SIZE: i64 = 65_536;
