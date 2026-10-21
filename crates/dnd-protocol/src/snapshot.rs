//! Full state snapshot sent on connect / resync when a delta is not available.

use serde::{Deserialize, Serialize};

use crate::ids::Seq;
use crate::model::{CampaignMeta, CharacterDto, MapDto, TokenDto, UserDto};

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
}
