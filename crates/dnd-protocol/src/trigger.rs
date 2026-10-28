//! Triggers (ADR-0024): "when → if → then" records the core runs. Data, not code: conditions are
//! `dnd-expr`, dice are `dnd-dice`, actions come from a fixed list. GM roles only; players never
//! see them.

use serde::{Deserialize, Serialize};

use crate::board::BoardVisibility;
use crate::ids::{
    BoardId, BoardNodeId, CharacterId, JournalEntryId, MapId, TokenId, TriggerId, Version, ZoneId,
};

/// What sets a trigger off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TriggerEvent {
    /// A character whose token stands in `zone` made one of `rolls` (system roll ids; empty:
    /// any system roll). `@total` is the roll's total.
    RollInZone { zone: ZoneId, rolls: Vec<String> },
    /// A token that was outside `zone` is now inside.
    TokenEnteredZone { zone: ZoneId },
    /// The GM's button (`fire_trigger`).
    Manual,
    /// A character's inventory gained an item called `name` (ignoring case).
    ItemReceived { name: String },
}

/// Something triggers can show to players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RevealTarget {
    /// A board node becomes open to players.
    Node {
        id: BoardNodeId,
    },
    /// A board is shown to players.
    Board {
        id: BoardId,
    },
    Map {
        id: MapId,
    },
    /// A journal entry is shared with players.
    Journal {
        id: JournalEntryId,
    },
    /// A token becomes visible.
    Token {
        id: TokenId,
    },
}

/// What a trigger does, step by step. Character actions apply to the character who set it off.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TriggerAction {
    /// Shows something to players.
    Reveal {
        target: RevealTarget,
    },
    /// Hides a board node again (or opens it: any visibility).
    SetNodeVisibility {
        node: BoardNodeId,
        visibility: BoardVisibility,
    },
    /// A narration line in the chat; `gm_only` keeps it from players.
    Chat {
        text: String,
        gm_only: bool,
    },
    /// Rolls dice (with the character's fields); later steps see the total as `@roll`.
    Roll {
        dice: String,
        label: String,
    },
    /// Damage (`heal: false`) or healing by a dice roll, kept within 0..=max of the resource.
    ChangeResource {
        resource: String,
        dice: String,
        heal: bool,
    },
    AddCondition {
        condition: String,
    },
    RemoveCondition {
        condition: String,
    },
    GiveItem {
        name: String,
    },
}

/// One action and, optionally, the expression that must hold for it to run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TriggerStep {
    pub when: Option<String>,
    pub action: TriggerAction,
}

/// What the GM edits: everything of a trigger but its id, state and version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TriggerDef {
    pub name: String,
    pub enabled: bool,
    /// Fires at most once (until edited again).
    pub once: bool,
    pub event: TriggerEvent,
    /// Tokens that may set it off; empty: any token of a character.
    pub tokens: Vec<TokenId>,
    /// Must hold (`dnd-expr` over the character's fields and `@total`) for the trigger to fire.
    pub condition: Option<String>,
    pub steps: Vec<TriggerStep>,
}

/// A trigger of the campaign.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TriggerDto {
    pub id: TriggerId,
    pub definition: TriggerDef,
    /// A `once` trigger that already fired.
    pub fired: bool,
    pub version: Version,
}

/// Who set a trigger off (for the GM's log).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TriggerFiredDto {
    pub trigger: TriggerId,
    pub name: String,
    pub character: Option<CharacterId>,
    pub token: Option<TokenId>,
}

/// Most steps of one trigger.
pub const MAX_TRIGGER_STEPS: usize = 32;
/// Most triggers firing for one intent.
pub const MAX_TRIGGERS_PER_INTENT: usize = 64;
/// Most triggers in a campaign.
pub const MAX_TRIGGERS: usize = 1000;
/// Longest chat line of a trigger, in characters.
pub const MAX_TRIGGER_TEXT: usize = 1000;
