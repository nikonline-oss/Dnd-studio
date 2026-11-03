//! Fictional campaign time. No wall-clock reads and no game-system constants.
use crate::{GameState, apply::bump};
use dnd_protocol::{EffectDuration, Event, RejectReason, RollRequestStatus};

pub(crate) fn advance(state: &GameState, seconds: i64) -> Result<Vec<Event>, RejectReason> {
    if !(1..=604_800).contains(&seconds) {
        return Err(RejectReason::invalid("seconds"));
    }
    advance_inner(state, seconds)
}
pub(crate) fn advance_inner(state: &GameState, seconds: i64) -> Result<Vec<Event>, RejectReason> {
    if !(1..=10_000_000_000).contains(&seconds) {
        return Err(RejectReason::invalid("seconds"));
    }
    if !state.plugin_runtime.saves.is_empty()
        || !state.plugin_runtime.action_stacks.is_empty()
        || !state.plugin_runtime.movements.is_empty()
        || state
            .roll_requests
            .values()
            .any(|r| r.status == RollRequestStatus::Pending)
    {
        return Err(RejectReason::invalid("pending_action"));
    }
    let mut clock = state.campaign.game_clock.clone();
    clock.seconds = clock
        .seconds
        .checked_add(seconds)
        .filter(|n| *n <= dnd_protocol::validate::MAX_SAFE_INTEGER - 604_800)
        .ok_or_else(|| RejectReason::invalid("seconds"))?;
    clock.version = bump(clock.version)?;
    let mut events = vec![Event::GameTimeAdvanced {
        clock: clock.clone(),
    }];
    events.extend(state.effects.values().filter(|e| matches!(e.duration, EffectDuration::GameTime { until } if until <= clock.seconds))
        .map(|e| Event::TokenEffectRemoved { id: e.id, expired: true }));
    Ok(events)
}
