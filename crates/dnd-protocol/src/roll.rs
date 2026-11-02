//! Manual rolls a trigger asks for: the core creates a request per target instead of rolling,
//! the controller (a player or the GM) fulfills it, a GM role may cancel it — reversing an
//! already applied resource change.

use serde::{Deserialize, Serialize};

use crate::ids::{CharacterId, RollRequestId, TokenId, TriggerId, Version};

/// Where a [`RollRequestDto`] stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum RollRequestStatus {
    /// Waiting for the controller's roll.
    Pending,
    /// Rolled; `delta` holds the applied resource change, if the roll hit.
    Rolled,
    /// Dismissed by a GM role; a rolled request's delta has been reversed.
    Cancelled,
}

/// One roll someone owes the table. Created by a `change_resource` trigger step with
/// `manual: true`; players see the requests of their own characters, GM roles see all of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RollRequestDto {
    pub id: RollRequestId,
    /// What the roller sees ("Poisoned dart").
    pub label: String,
    pub trigger: Option<TriggerId>,
    /// A GM-created plain check group, independent of triggers and resource changes.
    #[serde(default)]
    pub group: Option<RollRequestId>,
    pub character: CharacterId,
    pub token: Option<TokenId>,
    /// The roll the controller makes; `@` fields read the character's sheet.
    pub dice: String,
    /// An attack/check: `@total` is the roll, the character's fields evaluate the condition;
    /// the resource changes only on a hit. `None`: a plain roll always applies.
    pub hit: Option<String>,
    /// What a hit (or a plain roll) applies to the character's `resource`.
    pub resource: String,
    pub damage_dice: String,
    pub heal: bool,
    pub status: RollRequestStatus,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub total: Option<i64>,
    /// The applied resource delta, reversed when a GM cancels a rolled request.
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub delta: Option<i64>,
    pub version: Version,
}

/// Most pending manual rolls in a campaign.
pub const MAX_ROLL_REQUESTS: usize = 500;

pub const MAX_ROLL_TARGETS: usize = 20;

/// One row of the GM-only final summary. `None` means the request was cancelled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RollCheckResult {
    pub character: CharacterId,
    pub name: String,
    #[cfg_attr(feature = "specta", specta(type = Option<specta_typescript::Number>))]
    pub total: Option<i64>,
}
