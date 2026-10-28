//! Boards (ADR-0024): shared canvases of notes, links, pictures and the edges between them. What
//! players see on a board is decided by the GM and by triggers.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AssetId, BoardEdgeId, BoardId, BoardNodeId, CharacterId, JournalEntryId, MapId, TriggerId,
    Version,
};

/// Who sees a board element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BoardVisibility {
    /// GM roles only (the default for new elements).
    Gm,
    /// Everyone who sees the board.
    Players,
}

/// A board: one center tab of elements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BoardDto {
    pub id: BoardId,
    pub name: String,
    /// Order in the board list (ascending).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub sort_order: i64,
    /// Players see the board (and its elements open to players).
    pub visible_to_players: bool,
    pub version: Version,
}

/// What a link node opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BoardLink {
    Map { id: MapId },
    Journal { id: JournalEntryId },
    Character { id: CharacterId },
}

/// What a node shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BoardNodeContent {
    /// A card with Markdown text (at most [`MAX_NOTE_LEN`] characters).
    Note { markdown: String },
    /// A card that opens a map, a journal entry or a character sheet.
    Link { target: BoardLink },
    /// A picture from the campaign's assets.
    Image { asset_id: AssetId },
    /// A trigger as a block: GM roles only, whatever the node's visibility says.
    Trigger { id: TriggerId },
}

/// An element of a board. Position and size in Units16 (ADR-0016), the top-left corner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BoardNodeDto {
    pub id: BoardNodeId,
    pub board_id: BoardId,
    pub content: BoardNodeContent,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub x: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub y: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub width: i64,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub height: i64,
    pub visibility: BoardVisibility,
    pub version: Version,
}

/// A connection between two nodes of the same board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BoardEdgeDto {
    pub id: BoardEdgeId,
    pub board_id: BoardId,
    pub from: BoardNodeId,
    pub to: BoardNodeId,
    /// Caption on the line; may be empty.
    pub label: String,
    /// Drawn with an arrow towards `to`.
    pub directed: bool,
    /// Players see it only when it and both its nodes are open to them.
    pub visibility: BoardVisibility,
    pub version: Version,
}

/// Longest note, in characters.
pub const MAX_NOTE_LEN: usize = 8000;
/// Most nodes on one board.
pub const MAX_BOARD_NODES: usize = 1000;
/// Most edges on one board.
pub const MAX_BOARD_EDGES: usize = 2000;
/// Board coordinates lie in `-MAX_BOARD_COORD..=MAX_BOARD_COORD` (Units16).
pub const MAX_BOARD_COORD: i64 = 1_000_000 * 16;
/// Node sizes lie in `MIN_NODE_SIZE..=MAX_NODE_SIZE` (Units16).
pub const MIN_NODE_SIZE: i64 = 16 * 16;
pub const MAX_NODE_SIZE: i64 = 4000 * 16;
