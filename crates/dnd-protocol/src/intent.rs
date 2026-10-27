//! Intents: requests to change the game state. Only `dnd-game-core` decides whether they apply.

use serde::{Deserialize, Serialize};

use crate::board::{BoardNodeContent, BoardVisibility};
use crate::ids::{
    AssetId, BoardEdgeId, BoardId, BoardNodeId, CharacterId, ChatMessageId, CombatId, CombatantId,
    IntentId, JournalEntryId, JournalFolderId, MapId, TokenId, UserId, Version,
};
use crate::model::{AssetDto, CharacterKind, FogOp, Grid, JournalVisibility, TokenLayer};
use crate::plugin::PluginDto;

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

    /// GM / Co-GM. Opens a combat on a map (one per map), round 1, nobody's turn yet (ADR-0020).
    StartCombat {
        id: CombatId,
        map_id: MapId,
    },
    /// GM / Co-GM. A token of the combat's map (each at most once) and/or a character.
    AddCombatant {
        id: CombatantId,
        combat_id: CombatId,
        token_id: Option<TokenId>,
        character_id: Option<CharacterId>,
        name: String,
    },
    /// GM / Co-GM. If it was their turn, the turn passes to the next combatant.
    RemoveCombatant {
        id: CombatantId,
    },
    /// GM / Co-GM, or the owner of the combatant's character. Rolls the initiative formula
    /// against the character's sheet; the roll is posted to chat.
    RollInitiative {
        id: CombatantId,
    },
    /// GM / Co-GM. Sets (or clears) a combatant's initiative by hand.
    SetInitiative {
        id: CombatantId,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        initiative: Option<i64>,
    },
    /// GM / Co-GM, or the owner of the character whose turn it is. Passes the turn in initiative
    /// order; after the last combatant a new round starts.
    NextTurn {
        combat_id: CombatId,
    },
    /// GM / Co-GM. Removes the combat with its combatants.
    EndCombat {
        id: CombatId,
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
        folder_id: Option<JournalFolderId>,
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
    /// GM / Co-GM: move into `folder_id` (`None`: the root) at position `sort_order`.
    MoveJournalEntry {
        id: JournalEntryId,
        folder_id: Option<JournalFolderId>,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        sort_order: i64,
    },
    /// GM / Co-GM.
    DeleteJournalEntry {
        id: JournalEntryId,
    },
    /// GM / Co-GM. A folder in `parent` (`None`: the root), last among its siblings. Sibling folders
    /// have different names (ADR-0021).
    CreateJournalFolder {
        id: JournalFolderId,
        parent: Option<JournalFolderId>,
        name: String,
    },
    /// GM / Co-GM.
    RenameJournalFolder {
        id: JournalFolderId,
        name: String,
    },
    /// GM / Co-GM: into `parent` at `sort_order`; never into itself or its own subfolders.
    MoveJournalFolder {
        id: JournalFolderId,
        parent: Option<JournalFolderId>,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        sort_order: i64,
    },
    /// GM / Co-GM. Deletes the folder with all its subfolders and entries.
    DeleteJournalFolder {
        id: JournalFolderId,
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

    /// GM / Co-GM: installs (or replaces) a plugin in the campaign (ADR-0023). The client reads
    /// the package; the core checks the rules again.
    InstallPlugin {
        plugin: Box<PluginDto>,
    },
    /// GM / Co-GM. The campaign's system plugin cannot be uninstalled.
    UninstallPlugin {
        id: String,
    },
    /// GM / Co-GM: the campaign's rules (`None`: no system, free-form sheets).
    SetSystemPlugin {
        id: Option<String>,
    },

    /// GM / Co-GM: a new board (ADR-0024), hidden from players until shown.
    CreateBoard {
        id: BoardId,
        name: String,
    },
    UpdateBoard {
        id: BoardId,
        name: Option<String>,
        visible_to_players: Option<bool>,
    },
    /// Deletes the board with its nodes and edges.
    DeleteBoard {
        id: BoardId,
    },
    CreateBoardNode {
        id: BoardNodeId,
        board_id: BoardId,
        content: BoardNodeContent,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        width: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        height: i64,
        visibility: BoardVisibility,
    },
    UpdateBoardNode {
        id: BoardNodeId,
        content: Option<BoardNodeContent>,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        width: Option<i64>,
        #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
        height: Option<i64>,
        visibility: Option<BoardVisibility>,
    },
    /// Last-write-wins, like a token move (ADR-0003).
    MoveBoardNode {
        id: BoardNodeId,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
    },
    /// Deletes the node with its edges.
    DeleteBoardNode {
        id: BoardNodeId,
    },
    CreateBoardEdge {
        id: BoardEdgeId,
        board_id: BoardId,
        from: BoardNodeId,
        to: BoardNodeId,
        label: String,
        directed: bool,
        visibility: BoardVisibility,
    },
    UpdateBoardEdge {
        id: BoardEdgeId,
        label: Option<String>,
        directed: Option<bool>,
        visibility: Option<BoardVisibility>,
    },
    DeleteBoardEdge {
        id: BoardEdgeId,
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
            Self::StartCombat { .. } => "start_combat",
            Self::AddCombatant { .. } => "add_combatant",
            Self::RemoveCombatant { .. } => "remove_combatant",
            Self::RollInitiative { .. } => "roll_initiative",
            Self::SetInitiative { .. } => "set_initiative",
            Self::NextTurn { .. } => "next_turn",
            Self::EndCombat { .. } => "end_combat",
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
            Self::CreateJournalFolder { .. } => "create_journal_folder",
            Self::RenameJournalFolder { .. } => "rename_journal_folder",
            Self::MoveJournalFolder { .. } => "move_journal_folder",
            Self::DeleteJournalFolder { .. } => "delete_journal_folder",
            Self::RegisterAsset { .. } => "register_asset",
            Self::SendMessage { .. } => "send_message",
            Self::RollDice { .. } => "roll_dice",
            Self::InstallPlugin { .. } => "install_plugin",
            Self::UninstallPlugin { .. } => "uninstall_plugin",
            Self::SetSystemPlugin { .. } => "set_system_plugin",
            Self::CreateBoard { .. } => "create_board",
            Self::UpdateBoard { .. } => "update_board",
            Self::DeleteBoard { .. } => "delete_board",
            Self::CreateBoardNode { .. } => "create_board_node",
            Self::UpdateBoardNode { .. } => "update_board_node",
            Self::MoveBoardNode { .. } => "move_board_node",
            Self::DeleteBoardNode { .. } => "delete_board_node",
            Self::CreateBoardEdge { .. } => "create_board_edge",
            Self::UpdateBoardEdge { .. } => "update_board_edge",
            Self::DeleteBoardEdge { .. } => "delete_board_edge",
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
            | Self::ApplyFog { .. }
            | Self::StartCombat { .. }
            | Self::AddCombatant { .. }
            | Self::CreateJournalFolder { .. }
            | Self::CreateBoard { .. }
            | Self::CreateBoardNode { .. }
            | Self::CreateBoardEdge { .. } => VersionPolicy::Create,
            // Plugins are configuration the GM owns: the last choice wins.
            Self::MoveToken { .. }
            | Self::MoveBoardNode { .. }
            | Self::ResetFog { .. }
            | Self::InstallPlugin { .. }
            | Self::UninstallPlugin { .. }
            | Self::SetSystemPlugin { .. } => VersionPolicy::LastWriteWins,
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
            | Self::DeleteJournalEntry { .. }
            | Self::RemoveCombatant { .. }
            | Self::RollInitiative { .. }
            | Self::SetInitiative { .. }
            | Self::NextTurn { .. }
            | Self::EndCombat { .. }
            | Self::RenameJournalFolder { .. }
            | Self::MoveJournalFolder { .. }
            | Self::DeleteJournalFolder { .. }
            | Self::UpdateBoard { .. }
            | Self::DeleteBoard { .. }
            | Self::UpdateBoardNode { .. }
            | Self::DeleteBoardNode { .. }
            | Self::UpdateBoardEdge { .. }
            | Self::DeleteBoardEdge { .. } => VersionPolicy::Checked,
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
