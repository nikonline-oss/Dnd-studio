//! Events: facts produced by `dnd-game-core`. Clients and the database projection apply them.
//!
//! Create/update events carry the full resulting entity so that applying them is idempotent and
//! needs no knowledge of the previous state.

use serde::{Deserialize, Serialize};

use crate::board::{BoardDto, BoardEdgeDto, BoardNodeDto};
use crate::ids::{
    AssetId, BoardEdgeId, BoardId, BoardNodeId, CombatId, CombatantId, IntentId, JournalEntryId,
    JournalFolderId, LibraryFolderId, MapId, Seq, TokenId, TriggerId, ZoneId,
};
use crate::library::{LibraryFolderDto, SoundDto};
use crate::model::{
    AssetDto, CampaignMeta, CharacterDto, ChatMessageDto, CombatDto, CombatantDto, FogOp,
    JournalEntryDto, JournalFolderDto, MapDto, TokenDto, UserDto,
};
use crate::plugin::PluginDto;
use crate::trigger::{TriggerDto, TriggerFiredDto};
use crate::zone::ZoneDto;

/// A fact about a state change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Event {
    CampaignMetaUpdated {
        campaign: CampaignMeta,
    },

    MapCreated {
        map: MapDto,
    },
    MapUpdated {
        map: MapDto,
    },
    MapDeleted {
        id: MapId,
    },
    /// Appended to the map's fog; applying the same `op.id` twice adds it once.
    FogApplied {
        map_id: MapId,
        op: FogOp,
    },
    /// The map's fog now has no operations, only the initial state.
    FogReset {
        map_id: MapId,
        covered: bool,
    },

    TokenCreated {
        token: TokenDto,
    },
    TokenUpdated {
        token: TokenDto,
    },
    TokenMoved {
        id: TokenId,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
    },
    TokenDeleted {
        id: TokenId,
    },

    CharacterCreated {
        character: CharacterDto,
    },
    /// Any change: name, sheet, status, owner.
    CharacterUpdated {
        character: CharacterDto,
    },

    JournalEntryCreated {
        entry: JournalEntryDto,
    },
    /// Any change: text, title, folder, order, visibility.
    JournalEntryUpdated {
        entry: JournalEntryDto,
    },
    JournalEntryDeleted {
        id: JournalEntryId,
    },
    JournalFolderCreated {
        folder: JournalFolderDto,
    },
    /// Renamed or moved.
    JournalFolderUpdated {
        folder: JournalFolderDto,
    },
    /// The folder only: deleting a folder also sends the deletion of everything in it, entries
    /// and subfolders first.
    JournalFolderDeleted {
        id: JournalFolderId,
    },

    AssetRegistered {
        asset: Box<AssetDto>,
    },

    CombatStarted {
        combat: CombatDto,
    },
    /// Round or turn changed.
    CombatUpdated {
        combat: CombatDto,
    },
    /// The combat and all its combatants are gone.
    CombatEnded {
        id: CombatId,
    },
    CombatantAdded {
        combatant: CombatantDto,
    },
    /// Initiative changed.
    CombatantUpdated {
        combatant: CombatantDto,
    },
    CombatantRemoved {
        id: CombatantId,
    },

    /// A text, roll or system message.
    ChatMessagePosted {
        message: ChatMessageDto,
    },

    /// A participant of a session became (or already is) a user of the campaign. Produced by the
    /// session server when someone joins, not by an intent (ADR-0022).
    UserJoined {
        user: UserDto,
    },

    /// A plugin is installed (or replaced) in the campaign (ADR-0023).
    PluginInstalled {
        plugin: Box<PluginDto>,
    },
    PluginUninstalled {
        id: String,
    },
    /// The campaign's rules changed.
    SystemPluginSet {
        id: Option<String>,
    },

    /// A board appeared (ADR-0024).
    BoardCreated {
        board: BoardDto,
    },
    /// Renamed, shown to or hidden from players.
    BoardUpdated {
        board: BoardDto,
    },
    /// Its nodes and edges were deleted by earlier events of the batch.
    BoardDeleted {
        id: BoardId,
    },
    BoardNodeCreated {
        node: BoardNodeDto,
    },
    BoardNodeUpdated {
        node: BoardNodeDto,
    },
    /// Last-write-wins position; does not bump the version.
    BoardNodeMoved {
        id: BoardNodeId,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
    },
    /// Its edges were deleted by earlier events of the batch.
    BoardNodeDeleted {
        id: BoardNodeId,
    },
    BoardEdgeCreated {
        edge: BoardEdgeDto,
    },
    BoardEdgeUpdated {
        edge: BoardEdgeDto,
    },
    BoardEdgeDeleted {
        id: BoardEdgeId,
    },

    /// A zone on a map (ADR-0024); GM roles only.
    ZoneCreated {
        zone: ZoneDto,
    },
    ZoneUpdated {
        zone: ZoneDto,
    },
    ZoneDeleted {
        id: ZoneId,
    },

    /// A trigger (ADR-0024); GM roles only.
    TriggerCreated {
        trigger: TriggerDto,
    },
    /// Edited, or a `once` trigger fired.
    TriggerUpdated {
        trigger: TriggerDto,
    },
    TriggerDeleted {
        id: TriggerId,
    },
    /// A trigger fired; its effects follow as ordinary events. GM roles only.
    TriggerFired {
        fired: TriggerFiredDto,
    },

    /// Library folders (ADR-0026). GM roles only.
    LibraryFolderCreated {
        folder: LibraryFolderDto,
    },
    LibraryFolderUpdated {
        folder: LibraryFolderDto,
    },
    LibraryFolderDeleted {
        id: LibraryFolderId,
    },
    /// An asset was moved to another folder or renamed.
    AssetUpdated {
        asset: Box<AssetDto>,
    },
    /// A sound starts for everyone (a looping one stays in `sounds` until stopped).
    SoundPlayed {
        sound: SoundDto,
    },
    /// A looping sound stops; `None`: every sound stops.
    SoundStopped {
        asset_id: Option<AssetId>,
    },

    /// Stands in for an event the recipient may not see, so sequence numbers stay contiguous
    /// (ADR-0022). Never stored; applying it changes nothing.
    Redacted,
}

/// Events produced by one accepted intent. Event `i` of the batch has sequence number `seq + i`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EventBatch {
    /// Sequence number of the first event in `events`.
    pub seq: Seq,
    /// The intent that caused these events, if any.
    pub caused_by: Option<IntentId>,
    pub events: Vec<Event>,
}

impl EventBatch {
    /// Sequence number of the last event, or `seq - 1` for an empty batch.
    #[must_use]
    pub fn last_seq(&self) -> Seq {
        let len = i64::try_from(self.events.len()).unwrap_or(i64::MAX);
        Seq(self.seq.0.saturating_add(len).saturating_sub(1))
    }
}
