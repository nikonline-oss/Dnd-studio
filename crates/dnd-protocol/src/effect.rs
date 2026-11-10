//! Temporary effects on tokens (ADR-0024): a condition and/or a recurring resource change
//! (a potion, a poison) with a duration. Triggers apply them, players never edit them, the GM
//! removes them; combat turns and rounds count them down, real time expires them out of combat.

use serde::{Deserialize, Serialize};

use crate::ids::{EffectId, TokenId, Version};

/// How a [`TokenEffectDto`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum EffectDuration {
    /// Fictional elapsed campaign seconds. Only explicit game time advancement expires it.
    GameTime {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        until: i64,
    },
    /// End of the recorded turn, including a one-combatant round wrap.
    TurnEnd {
        combat: crate::CombatId,
        turn: crate::CombatantId,
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        round: i64,
    },
    /// Combat rounds on the token's map; `remaining` counts down when a new round starts.
    Rounds { remaining: u8 },
    /// The token's own turns; `remaining` counts down at the start of each of its turns.
    Turns { remaining: u8 },
    /// Real time, for out of combat: expires at this Unix-ms timestamp. The core has no clock,
    /// so the deadline is set from the envelope's `ts_ms` and checked lazily on later intents.
    RealTime {
        #[cfg_attr(feature = "specta", specta(type = specta_typescript::Number))]
        until_ts: i64,
    },
}

/// A temporary effect hanging on a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TokenEffectDto {
    pub id: EffectId,
    pub token: TokenId,
    /// Shown name (the potion, the trap, ...).
    pub label: String,
    /// A condition id of the system, shown as a badge.
    pub condition: Option<String>,
    /// Resource changed by `dice` at the start of each of the token's turns, if any.
    pub resource: Option<String>,
    pub dice: Option<String>,
    /// `true`: the recurring change heals instead of damaging.
    pub heal: bool,
    /// Inspector rows this effect opens while it lasts (ADR-0061). A temporary reveal beats the
    /// campaign policy and the token override, so an expired effect takes its rows with it.
    #[serde(default)]
    pub reveal: Vec<crate::model::TokenField>,
    pub duration: EffectDuration,
    pub version: Version,
}

/// Most effects on one token.
pub const MAX_EFFECTS_PER_TOKEN: usize = 16;
/// Longest effect label, in characters.
pub const MAX_EFFECT_LABEL: usize = 200;
/// Longest duration in rounds or turns.
pub const MAX_EFFECT_SPAN: u8 = 100;
/// Longest real-time duration, in minutes (a week).
pub const MAX_EFFECT_MINUTES: u32 = 10_080;
