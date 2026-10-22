//! In-memory game state.

use std::collections::BTreeMap;

use dnd_protocol::{
    AssetDto, AssetId, CampaignMeta, CharacterDto, CharacterId, CombatDto, CombatId, CombatantDto,
    CombatantId, FogDto, JournalEntryDto, JournalEntryId, JournalEntryMeta, MapDto, MapId, Seq,
    Snapshot, TokenDto, TokenId, UserDto, UserId, Version,
};

/// The authoritative state the core reasons about. Ordered maps keep iteration (and therefore
/// produced events) deterministic.
#[derive(Clone, Debug, PartialEq)]
pub struct GameState {
    pub campaign: CampaignMeta,
    pub maps: BTreeMap<MapId, MapDto>,
    pub tokens: BTreeMap<TokenId, TokenDto>,
    pub characters: BTreeMap<CharacterId, CharacterDto>,
    /// Known users. Read-only for the core in stage 2a (they join through the server in 2b).
    pub users: BTreeMap<UserId, UserDto>,
    pub journal: BTreeMap<JournalEntryId, JournalEntryDto>,
    pub assets: BTreeMap<AssetId, AssetDto>,
    /// Fog of war by map; maps without fog have no entry (ADR-0019).
    pub fog: BTreeMap<MapId, FogDto>,
    /// Combats (at most one per map) and their combatants (ADR-0020).
    pub combats: BTreeMap<CombatId, CombatDto>,
    pub combatants: BTreeMap<CombatantId, CombatantDto>,
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
            assets: BTreeMap::new(),
            fog: BTreeMap::new(),
            combats: BTreeMap::new(),
            combatants: BTreeMap::new(),
        }
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
                        folder: m.folder,
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
            assets: snapshot.assets.into_iter().map(|a| (a.id, a)).collect(),
            fog: snapshot.fog.into_iter().map(|f| (f.map_id, f)).collect(),
            combats: snapshot.combats.into_iter().map(|c| (c.id, c)).collect(),
            combatants: snapshot.combatants.into_iter().map(|c| (c.id, c)).collect(),
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
            assets: self.assets.values().cloned().collect(),
            fog: self.fog.values().cloned().collect(),
            combats: self.combats.values().cloned().collect(),
            combatants: self.combatants.values().cloned().collect(),
            // Chat history is kept by the server layer, not by the core.
            chat: Vec::new(),
        }
    }

    /// Tokens placed on `map_id`, in id order.
    pub fn tokens_on(&self, map_id: MapId) -> impl Iterator<Item = &TokenDto> {
        self.tokens.values().filter(move |t| t.map_id == map_id)
    }
}
