//! Deterministic game core: state, apply(intent) -> events, role permissions. No I/O.
//!
//! ```text
//! apply(state, intent, actor, rng) -> Result<Vec<Event>, RuleError>   // decide
//! evolve(state, event)                                                 // apply a fact
//! ```
//!
//! The session server and the desktop app's local "server" run exactly this pipeline (ADR-0001).

mod apply;
mod assets;
mod characters;
mod chat;
mod combat;
mod evolve;
mod fog;
mod journal;
mod permissions;
mod plugins;
mod recent;
mod rng;
mod state;
mod users;
mod visibility;

pub use apply::{MAX_DESCRIPTION_LEN, MAX_NAME_LEN, RuleError, apply};
pub use combat::{initiative_expression, turn_order, turn_order_cmp};
pub use evolve::{evolve, evolve_all};
pub use permissions::{Actor, authorize};
pub use plugins::{CharacterFields, SheetContext, check_plugin, default_sheet};
pub use recent::{IntentWindow, Recall, RecentIntents};
pub use rng::{GameRng, SplitMix64};
pub use state::GameState;
pub use users::join;
pub use visibility::{
    ForPlayers, PlayersSee, chat_visible_to_players, filter_for_players, may_change_visibility,
    players_state, sees_everything, snapshot_for,
};
