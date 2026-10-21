//! Plain data transfer objects shared by intents, events and snapshots.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AssetId, CharacterId, ChatMessageId, JournalEntryId, MapId, TokenId, UserId, Version,
};

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

/// Who may read a journal entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum JournalVisibility {
    /// GM and Co-GM only.
    Gm,
    /// Everyone in the campaign.
    Players,
}

/// A journal entry with its Markdown text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JournalEntryDto {
    pub id: JournalEntryId,
    /// `/`-separated folder path, `""` for the root.
    pub folder: String,
    pub title: String,
    pub markdown: String,
    /// Order inside the folder (ascending).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub sort_order: i64,
    pub visibility: JournalVisibility,
    /// Players may edit it (only when visible to players).
    pub players_can_edit: bool,
    pub author: Option<UserId>,
    pub version: Version,
}

/// A journal entry without its text, as listed in the snapshot (texts are fetched on demand).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JournalEntryMeta {
    pub id: JournalEntryId,
    pub folder: String,
    pub title: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub sort_order: i64,
    pub visibility: JournalVisibility,
    pub players_can_edit: bool,
    pub author: Option<UserId>,
    pub version: Version,
}

impl From<&JournalEntryDto> for JournalEntryMeta {
    fn from(e: &JournalEntryDto) -> Self {
        Self {
            id: e.id,
            folder: e.folder.clone(),
            title: e.title.clone(),
            sort_order: e.sort_order,
            visibility: e.visibility,
            players_can_edit: e.players_can_edit,
            author: e.author,
            version: e.version,
        }
    }
}

/// Largest Markdown text of a journal entry, in bytes.
pub const MAX_JOURNAL_BYTES: usize = 1024 * 1024;
/// Largest folder path, in characters.
pub const MAX_FOLDER_LEN: usize = 200;

/// What a chat message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ChatKind {
    Text,
    /// Produced by the core; see [`SystemMessage`].
    System,
    Roll,
}

/// Who may see a chat message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ChatVisibility {
    All,
    /// Hidden GM rolls: GM and Co-GM only (filtering per recipient — stage 4).
    Gm,
}

/// One physical die of a roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DieDto {
    pub value: u32,
    /// `false` if dropped by keep/drop or advantage/disadvantage.
    pub kept: bool,
}

/// One term of a roll: dice (with each die), a constant or a sheet field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiceTermDto {
    pub negative: bool,
    /// Canonical notation: `4d6kh3`, `5`, `@str_mod`.
    pub notation: String,
    /// Value before the sign is applied.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub subtotal: i64,
    pub dice: Vec<DieDto>,
}

/// Result of a dice roll (ADR-0005, ADR-0016: all numbers are integers).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DiceRollDto {
    /// Canonical form of the expression.
    pub expression: String,
    pub terms: Vec<DiceTermDto>,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub total: i64,
}

/// Messages the core posts; the UI localizes them by `code`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SystemMessage {
    CharacterSubmitted { name: String },
    CharacterApproved { name: String },
    CharacterReturned { name: String },
}

/// A chat message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChatMessageDto {
    pub id: ChatMessageId,
    pub kind: ChatKind,
    /// `None` for system messages and after the sender was deleted.
    pub sender: Option<UserId>,
    /// The sender's name when the message was written (ADR-0008).
    pub sender_name: String,
    /// Text for `text`; the expression as typed for `roll`; empty for `system`.
    pub body: String,
    pub roll: Option<DiceRollDto>,
    pub system: Option<SystemMessage>,
    pub visibility: ChatVisibility,
    /// When it was stored (unix ms); `None` in live events (the core has no clock).
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub at_ms: Option<i64>,
}

/// Longest chat message, in characters.
pub const MAX_CHAT_LEN: usize = 4000;
/// Chat messages carried by a snapshot; older ones are paged with `chat_history`.
pub const CHAT_SNAPSHOT_LEN: usize = 200;
