//! Plain data transfer objects shared by intents, events and snapshots.

use serde::{Deserialize, Serialize};

use crate::ids::{
    AssetId, CharacterId, ChatMessageId, CombatId, CombatantId, FogOpId, JournalEntryId,
    JournalFolderId, MapId, TokenId, UserId, Version,
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

/// Grid cell size bounds, in logical units: smaller cells would make the grid unreadable and
/// too expensive to draw (ADR-0017).
pub const MIN_CELL_SIZE: f64 = 8.0;
pub const MAX_CELL_SIZE: f64 = 2048.0;

/// Whether a fog operation uncovers or covers its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FogOpKind {
    Reveal,
    Hide,
}

/// The area of a fog operation, in Units16 (ADR-0016, ADR-0019).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FogShape {
    /// Axis-aligned rectangle with its top-left corner at (`x`, `y`).
    Rect {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        width: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        height: i64,
    },
    /// A stroke: circles of `radius` swept along the polyline `points`.
    Brush {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        radius: i64,
        /// Flat `[x0, y0, x1, y1, …]`, 1..=[`MAX_BRUSH_POINTS`] points.
        #[cfg_attr(feature = "specta", specta(type = Vec<specta_typescript::Number>))]
        points: Vec<i64>,
    },
}

/// One fog-of-war operation. Operations are applied in order over the initial state (ADR-0019).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FogOp {
    pub id: FogOpId,
    pub kind: FogOpKind,
    pub shape: FogShape,
}

/// Fog of war of one map: covered or clear at first, then the operations in order. A map
/// without fog has none (same as `covered: false` with no operations).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FogDto {
    pub map_id: MapId,
    pub covered: bool,
    pub ops: Vec<FogOp>,
}

/// A combat on a map (ADR-0020): the turn order of its combatants. At most one per map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CombatDto {
    pub id: CombatId,
    pub map_id: MapId,
    /// Starts at 1; grows when the turn wraps around.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub round: i64,
    /// Whose turn it is; `None` until the first `next_turn`.
    pub turn: Option<CombatantId>,
    pub version: Version,
}

/// A participant of a combat: usually a token, possibly linked to a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CombatantDto {
    pub id: CombatantId,
    pub combat_id: CombatId,
    /// `None` for a combatant without a token, or after its token was deleted.
    pub token_id: Option<TokenId>,
    /// The sheet the initiative formula reads (and who owns the combatant).
    pub character_id: Option<CharacterId>,
    pub name: String,
    /// `None` until rolled or set.
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub initiative: Option<i64>,
    /// Order of joining; breaks ties in initiative.
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub sort_order: i64,
    pub version: Version,
}

/// Most combatants in one combat.
pub const MAX_COMBATANTS: usize = 100;

/// Most points in one brush stroke.
pub const MAX_BRUSH_POINTS: usize = 512;
/// Largest brush radius, in Units16 (256 logical units).
pub const MAX_BRUSH_RADIUS: i64 = 256 * UNITS16_PER_UNIT;
/// Most fog operations on one map; then the GM resets the fog (ADR-0019).
pub const MAX_FOG_OPS: usize = 4096;
/// Most brush points over all fog operations of one map (bounds the stored JSON, ~1 MB).
pub const MAX_FOG_POINTS: usize = 65_536;

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
    /// The folder it is in; `None` for the root (ADR-0021).
    pub folder_id: Option<JournalFolderId>,
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

/// A folder of the journal (ADR-0021): folders nest; an empty folder exists on its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JournalFolderDto {
    pub id: JournalFolderId,
    /// `None` for a folder at the root.
    pub parent: Option<JournalFolderId>,
    pub name: String,
    /// Order among its siblings (ascending).
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub sort_order: i64,
    pub version: Version,
}

/// Deepest nesting of journal folders.
pub const MAX_FOLDER_DEPTH: usize = 16;

/// A journal entry without its text, as listed in the snapshot (texts are fetched on demand).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JournalEntryMeta {
    pub id: JournalEntryId,
    pub folder_id: Option<JournalFolderId>,
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
            folder_id: e.folder_id,
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

/// What an imported image is for (decides size limits and tiling, ADR-0017).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum AssetUsage {
    /// Map background: cut into 2048² WebP tiles, at most 16 384 px per side.
    Map,
    /// Token or portrait: one WebP, at most 2048 px per side.
    Token,
}

/// One 2048² (or smaller, at the edges) tile of a map image, in image pixels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AssetTileDto {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// SHA-256 (hex) of the tile's WebP file.
    pub sha256: String,
}

/// An imported image. Files are content-addressed by SHA-256 (ADR-0004): `sha256` is the original
/// upload (dedup key), the others are derived WebP files served to the webview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AssetDto {
    pub id: AssetId,
    pub usage: AssetUsage,
    pub sha256: String,
    pub mime: String,
    #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
    pub size_bytes: i64,
    pub original_name: Option<String>,
    /// Pixel size after any downscaling on import.
    pub width: u32,
    pub height: u32,
    /// Whole image as WebP (`None` for maps, which are drawn from tiles).
    pub display_sha256: Option<String>,
    pub thumbnail_sha256: String,
    /// Map tiles, row by row; empty for tokens.
    pub tiles: Vec<AssetTileDto>,
    pub version: Version,
}
