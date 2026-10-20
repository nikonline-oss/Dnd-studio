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

/// Grid settings of a map. All lengths are in map pixels.
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
    pub x: f64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: f64,
    /// Degrees, clockwise.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub rotation: f64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub scale: f64,
    pub layer: TokenLayer,
    pub visible: bool,
    pub version: Version,
}

/// The part of a character the game core needs before character intents exist (stage 2):
/// who owns it, to decide who may move its tokens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CharacterRef {
    pub id: CharacterId,
    pub name: String,
    pub owner: Option<UserId>,
}
