//! In-memory game state.

use std::collections::BTreeMap;

use dnd_protocol::{
    AssetDto, AssetId, BoardDto, BoardEdgeDto, BoardEdgeId, BoardId, BoardNodeDto, BoardNodeId,
    CampaignMeta, CharacterDto, CharacterId, CombatDto, CombatId, CombatantDto, CombatantId,
    FogDto, JournalEntryDto, JournalEntryId, JournalEntryMeta, JournalFolderDto, JournalFolderId,
    MapDto, MapId, PluginDto, Seq, Snapshot, SystemRulesDto, TokenDto, TokenId, TriggerDto,
    TriggerId, UserDto, UserId, Version, ZoneDto, ZoneId,
};

/// The authoritative state the core reasons about. Ordered maps keep iteration (and therefore
/// produced events) deterministic.
#[derive(Clone, Debug, PartialEq)]
pub struct GameState {
    pub campaign: CampaignMeta,
    pub maps: BTreeMap<MapId, MapDto>,
    pub tokens: BTreeMap<TokenId, TokenDto>,
    pub characters: BTreeMap<CharacterId, CharacterDto>,
    /// Known users. Not changed by intents: participants join through the session server.
    pub users: BTreeMap<UserId, UserDto>,
    pub journal: BTreeMap<JournalEntryId, JournalEntryDto>,
    /// Journal folders (ADR-0021).
    pub journal_folders: BTreeMap<JournalFolderId, JournalFolderDto>,
    pub assets: BTreeMap<AssetId, AssetDto>,
    /// Fog of war by map; maps without fog have no entry (ADR-0019).
    pub fog: BTreeMap<MapId, FogDto>,
    /// Combats (at most one per map) and their combatants (ADR-0020).
    pub combats: BTreeMap<CombatId, CombatDto>,
    pub combatants: BTreeMap<CombatantId, CombatantDto>,
    /// Plugins installed in the campaign, by id (ADR-0023).
    pub plugins: BTreeMap<String, PluginDto>,
    /// The campaign's system plugin; its rules govern sheets and rolls.
    pub system_plugin: Option<String>,
    /// Boards with their nodes and edges (ADR-0024).
    pub boards: BTreeMap<BoardId, BoardDto>,
    pub board_nodes: BTreeMap<BoardNodeId, BoardNodeDto>,
    pub board_edges: BTreeMap<BoardEdgeId, BoardEdgeDto>,
    /// Zones on maps, watched by triggers (ADR-0024).
    pub zones: BTreeMap<ZoneId, ZoneDto>,
    /// Triggers the core runs after intents (ADR-0024).
    pub triggers: BTreeMap<TriggerId, TriggerDto>,
}

impl GameState {
    /// An empty campaign with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            campaign: CampaignMeta {
                name: name.into(),
                description: String::new(),
                version: Version::INITIAL,
            },
            maps: BTreeMap::new(),
            tokens: BTreeMap::new(),
            characters: BTreeMap::new(),
            users: BTreeMap::new(),
            journal: BTreeMap::new(),
            journal_folders: BTreeMap::new(),
            assets: BTreeMap::new(),
            fog: BTreeMap::new(),
            combats: BTreeMap::new(),
            combatants: BTreeMap::new(),
            plugins: BTreeMap::new(),
            system_plugin: None,
            boards: BTreeMap::new(),
            board_nodes: BTreeMap::new(),
            board_edges: BTreeMap::new(),
            zones: BTreeMap::new(),
            triggers: BTreeMap::new(),
        }
    }

    /// The rules of the campaign's system plugin, if it has one.
    #[must_use]
    pub fn rules(&self) -> Option<&SystemRulesDto> {
        self.system_plugin
            .as_ref()
            .and_then(|id| self.plugins.get(id))
            .and_then(|p| p.system.as_ref())
    }

    /// Rebuilds state from a snapshot. Journal texts are not part of a snapshot (fetched per
    /// entry), so rebuilt entries have an empty `markdown`.
    #[must_use]
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        Self {
            campaign: snapshot.campaign,
            maps: snapshot.maps.into_iter().map(|m| (m.id, m)).collect(),
            tokens: snapshot.tokens.into_iter().map(|t| (t.id, t)).collect(),
            characters: snapshot.characters.into_iter().map(|c| (c.id, c)).collect(),
            users: snapshot.users.into_iter().map(|u| (u.id, u)).collect(),
            journal: snapshot
                .journal
                .into_iter()
                .map(|m| {
                    let entry = JournalEntryDto {
                        id: m.id,
                        folder_id: m.folder_id,
                        title: m.title,
                        markdown: String::new(),
                        sort_order: m.sort_order,
                        visibility: m.visibility,
                        players_can_edit: m.players_can_edit,
                        author: m.author,
                        version: m.version,
                    };
                    (entry.id, entry)
                })
                .collect(),
            journal_folders: snapshot
                .journal_folders
                .into_iter()
                .map(|f| (f.id, f))
                .collect(),
            assets: snapshot.assets.into_iter().map(|a| (a.id, a)).collect(),
            fog: snapshot.fog.into_iter().map(|f| (f.map_id, f)).collect(),
            combats: snapshot.combats.into_iter().map(|c| (c.id, c)).collect(),
            combatants: snapshot.combatants.into_iter().map(|c| (c.id, c)).collect(),
            plugins: snapshot
                .plugins
                .into_iter()
                .map(|p| (p.id.clone(), p))
                .collect(),
            system_plugin: snapshot.system_plugin,
            boards: snapshot.boards.into_iter().map(|b| (b.id, b)).collect(),
            board_nodes: snapshot
                .board_nodes
                .into_iter()
                .map(|n| (n.id, n))
                .collect(),
            board_edges: snapshot
                .board_edges
                .into_iter()
                .map(|e| (e.id, e))
                .collect(),
            zones: snapshot.zones.into_iter().map(|z| (z.id, z)).collect(),
            triggers: snapshot.triggers.into_iter().map(|t| (t.id, t)).collect(),
        }
    }

    /// Full (unfiltered) snapshot as of `seq`.
    #[must_use]
    pub fn to_snapshot(&self, seq: Seq) -> Snapshot {
        Snapshot {
            seq,
            campaign: self.campaign.clone(),
            maps: self.maps.values().cloned().collect(),
            tokens: self.tokens.values().cloned().collect(),
            characters: self.characters.values().cloned().collect(),
            users: self.users.values().cloned().collect(),
            journal: self.journal.values().map(JournalEntryMeta::from).collect(),
            journal_folders: self.journal_folders.values().cloned().collect(),
            assets: self.assets.values().cloned().collect(),
            fog: self.fog.values().cloned().collect(),
            combats: self.combats.values().cloned().collect(),
            combatants: self.combatants.values().cloned().collect(),
            // Chat history is kept by the server layer, not by the core.
            chat: Vec::new(),
            plugins: self.plugins.values().cloned().collect(),
            system_plugin: self.system_plugin.clone(),
            boards: self.boards.values().cloned().collect(),
            board_nodes: self.board_nodes.values().cloned().collect(),
            board_edges: self.board_edges.values().cloned().collect(),
            zones: self.zones.values().cloned().collect(),
            triggers: self.triggers.values().cloned().collect(),
        }
    }

    /// Tokens placed on `map_id`, in id order.
    pub fn tokens_on(&self, map_id: MapId) -> impl Iterator<Item = &TokenDto> {
        self.tokens.values().filter(move |t| t.map_id == map_id)
    }
}
