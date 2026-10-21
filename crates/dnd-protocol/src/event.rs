//! Events: facts produced by `dnd-game-core`. Clients and the database projection apply them.
//!
//! Create/update events carry the full resulting entity so that applying them is idempotent and
//! needs no knowledge of the previous state.

use serde::{Deserialize, Serialize};

use crate::ids::{IntentId, JournalEntryId, MapId, Seq, TokenId};
use crate::model::{
    AssetDto, CampaignMeta, CharacterDto, ChatMessageDto, JournalEntryDto, MapDto, TokenDto,
};

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

    AssetRegistered {
        asset: Box<AssetDto>,
    },

    /// A text, roll or system message.
    ChatMessagePosted {
        message: ChatMessageDto,
    },
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
