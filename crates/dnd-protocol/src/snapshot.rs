//! Full state snapshot sent on connect / resync when a delta is not available.

use serde::{Deserialize, Serialize};

use crate::board::{BoardDto, BoardEdgeDto, BoardNodeDto};
use crate::ids::Seq;
use crate::library::{LibraryFolderDto, SoundDto};
use crate::model::{
    AssetDto, CampaignMeta, CharacterDto, ChatMessageDto, CombatDto, CombatantDto, FogDto,
    JournalEntryMeta, JournalFolderDto, MapDto, TokenDto, UserDto,
};
use crate::plugin::PluginDto;
use crate::trigger::TriggerDto;
use crate::zone::ZoneDto;

/// The visible state as of event `seq`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Snapshot {
    #[serde(default)]
    pub rules_view: crate::RulesView,
    #[serde(default)]
    pub items: Vec<crate::ItemDto>,
    #[serde(default)]
    pub plugin_runtime: crate::PluginRuntime,
    /// Sequence number of the last event included (0 = no events yet).
    pub seq: Seq,
    pub campaign: CampaignMeta,
    pub maps: Vec<MapDto>,
    pub tokens: Vec<TokenDto>,
    /// Which inspector rows each visible token shows this recipient (ADR-0061). Built per
    /// recipient from the campaign policy, the token override and active effects.
    #[serde(default)]
    pub token_inspector: Vec<crate::model::TokenInspectorView>,
    pub characters: Vec<CharacterDto>,
    pub users: Vec<UserDto>,
    /// Journal index; texts are fetched per entry.
    pub journal: Vec<JournalEntryMeta>,
    /// Journal folders (ADR-0021).
    pub journal_folders: Vec<JournalFolderDto>,
    pub assets: Vec<AssetDto>,
    /// Fog of maps that have any (ADR-0019).
    pub fog: Vec<FogDto>,
    /// Combats and their combatants (ADR-0020).
    pub combats: Vec<CombatDto>,
    pub combatants: Vec<CombatantDto>,
    /// The last `CHAT_SNAPSHOT_LEN` chat messages, oldest first.
    pub chat: Vec<ChatMessageDto>,
    /// Plugins installed in the campaign (ADR-0023).
    pub plugins: Vec<PluginDto>,
    /// Id of the campaign's system plugin.
    pub system_plugin: Option<String>,
    /// Boards with their nodes and edges (ADR-0024).
    pub boards: Vec<BoardDto>,
    pub board_nodes: Vec<BoardNodeDto>,
    pub board_edges: Vec<BoardEdgeDto>,
    /// Zones on maps (GM roles only; empty for players).
    pub zones: Vec<ZoneDto>,
    /// Triggers (GM roles only; empty for players).
    pub triggers: Vec<TriggerDto>,
    /// Folders of the image and audio libraries (GM roles only; empty for players, ADR-0026).
    #[serde(default)]
    pub library_folders: Vec<LibraryFolderDto>,
    /// Looping or paused sounds, with the last commanded position for reconnecting clients.
    #[serde(default)]
    pub sounds: Vec<SoundDto>,
    #[serde(default)]
    pub playlists: Vec<crate::PlaylistDto>,
    /// Temporary effects on tokens (ADR-0024); filtered with their tokens.
    #[serde(default)]
    pub effects: Vec<crate::TokenEffectDto>,
    /// Manual rolls triggers asked for; every role receives them (the widget filters by
    /// character ownership), GM roles see and manage all of them.
    #[serde(default)]
    pub roll_requests: Vec<crate::RollRequestDto>,
}
