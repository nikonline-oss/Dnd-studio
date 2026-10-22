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
mod evolve;
mod fog;
mod journal;
mod permissions;
mod recent;
mod rng;
mod state;

pub use apply::{MAX_DESCRIPTION_LEN, MAX_NAME_LEN, RuleError, apply};
pub use evolve::{evolve, evolve_all};
pub use permissions::{Actor, authorize};
pub use recent::{IntentWindow, Recall, RecentIntents};
pub use rng::{GameRng, SplitMix64};
pub use state::GameState;
