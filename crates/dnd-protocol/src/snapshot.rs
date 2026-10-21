//! Full state snapshot sent on connect / resync when a delta is not available.

use serde::{Deserialize, Serialize};

use crate::ids::Seq;
use crate::model::{
    CampaignMeta, CharacterDto, ChatMessageDto, JournalEntryMeta, MapDto, TokenDto, UserDto,
};

/// The visible state as of event `seq`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Snapshot {
    /// Sequence number of the last event included (0 = no events yet).
    pub seq: Seq,
    pub campaign: CampaignMeta,
    pub maps: Vec<MapDto>,
    pub tokens: Vec<TokenDto>,
    pub characters: Vec<CharacterDto>,
    pub users: Vec<UserDto>,
    /// Journal index; texts are fetched per entry.
    pub journal: Vec<JournalEntryMeta>,
    /// The last `CHAT_SNAPSHOT_LEN` chat messages, oldest first.
    pub chat: Vec<ChatMessageDto>,
}
