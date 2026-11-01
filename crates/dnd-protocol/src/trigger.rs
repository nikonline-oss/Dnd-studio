//! Triggers (ADR-0024): "when → if → then" records the core runs. Data, not code: conditions are
//! `dnd-expr`, dice are `dnd-dice`, actions come from a fixed list. GM roles only; players never
//! see them.

use serde::{Deserialize, Serialize};

use crate::board::BoardVisibility;
use crate::ids::{
    AssetId, BoardId, BoardNodeId, CharacterId, JournalEntryId, MapId, TokenId, TriggerId, Version,
    ZoneId,
};

/// Where tokens created by a trigger land on the firing map (Units16, ADR-0016).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SpawnPlacement {
    /// An exact point; later tokens fan out around it.
    Point {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        x: i64,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        y: i64,
    },
    /// A random point inside the zone (per token).
    Zone { id: ZoneId },
}

/// Who a damage/healing step applies to. Zone and map targets need a firing map; the zone is
/// matched by name on that map, like zone events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ResourceTarget {
    /// The character who set the trigger off (legacy behavior).
    #[default]
    Activator,
    /// Every distinct character with a token inside the zone of this name on the firing map.
    Zone { id: ZoneId },
    /// Every distinct character with a token on the firing map.
    Map,
}

/// An attack/check roll the damage waits on (auto mode): rolled per target with the activator's
/// fields and the step variables; the damage applies only when `hit` — evaluated against the
/// target's fields with `@total` = the attack roll — holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AttackCheck {
    pub dice: String,
    pub hit: String,
}

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
    /// Without `attack`, one roll is shared by every target; `attack` rolls per target and
    /// applies only on a hit. `manual` asks the targets' controllers to roll instead.
    ChangeResource {
        resource: String,
        dice: String,
        heal: bool,
        /// Who takes the change; missing in legacy data: the activator.
        #[serde(default)]
        target: ResourceTarget,
        #[serde(default)]
        attack: Option<AttackCheck>,
        #[serde(default)]
        manual: bool,
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
    /// Plays an audio asset to everyone (ADR-0026).
    PlaySound {
        asset: AssetId,
        volume: u8,
        looped: bool,
    },
    /// Stops a looping sound; `None`: all sounds.
    StopSound {
        asset: Option<AssetId>,
    },
    /// Spawns `count` tokens of a character on the firing map (ADR-0024).
    SpawnTokens {
        character: CharacterId,
        count: u8,
        placement: SpawnPlacement,
    },
    /// Opens a combat on the firing map: every visible character token joins and rolls
    /// initiative by the system's formula.
    StartCombat,
    /// A one-shot sound: background music pauses for everyone and resumes from its position
    /// when the sting ends.
    PlaySting {
        asset: AssetId,
        volume: u8,
    },
    /// Puts a temporary effect on the firing token (ADR-0024).
    ApplyEffect {
        label: String,
        /// A condition id of the system to show, if any.
        condition: Option<String>,
        /// A resource changed by `dice` at the start of each of the token's turns, if any.
        resource: Option<String>,
        dice: Option<String>,
        heal: bool,
        duration: EffectDurationDef,
    },
}

/// The duration a trigger gives to an effect (the entity stores the countdown).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum EffectDurationDef {
    /// Combat rounds on the token's map.
    Rounds { rounds: u8 },
    /// The token's own turns.
    Turns { turns: u8 },
    /// Real time, for out of combat.
    RealTime { minutes: u32 },
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
    /// Fires at most once per bound map (until edited again).
    pub once: bool,
    /// Maps where this trigger runs. Empty legacy zone definitions infer the zone's map;
    /// other empty legacy definitions retain their campaign-wide behavior.
    #[serde(default)]
    pub map_ids: Vec<MapId>,
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
    /// Bound maps already spent by a once trigger; edits clear this list.
    #[serde(default)]
    pub fired_maps: Vec<MapId>,
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
    /// Map context; absent in legacy/global firings.
    #[serde(default)]
    pub map_id: Option<MapId>,
}

/// Most steps of one trigger.
pub const MAX_TRIGGER_STEPS: usize = 32;
/// Most triggers firing for one intent.
pub const MAX_TRIGGERS_PER_INTENT: usize = 64;
/// Most triggers in a campaign.
pub const MAX_TRIGGERS: usize = 1000;
/// Longest chat line of a trigger, in characters.
pub const MAX_TRIGGER_TEXT: usize = 1000;
/// Most tokens one spawn action creates.
pub const MAX_SPAWN_TOKENS: usize = 20;
