//! Temporary token effects (ADR-0024): applied by triggers, removed by the GM, counted down by
//! combat turns and rounds, expired by real time (lazily, off the envelope's `ts_ms` — the core
//! has no clock of its own).

use dnd_protocol::{EffectDuration, Event, Intent, MapId, RejectReason, TokenEffectDto, TokenId};

use crate::apply::bump;
use crate::evolve::evolve;
use crate::rng::GameRng;
use crate::state::GameState;

/// Events for an effect intent; `None` if `intent` is not about effects.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    match intent {
        Intent::RemoveTokenEffect { id } => Some(if state.effects.contains_key(id) {
            Ok(vec![Event::TokenEffectRemoved {
                id: *id,
                expired: false,
            }])
        } else {
            Err(RejectReason::NotFound)
        }),
        _ => None,
    }
}

/// Real-time effects whose deadline passed by `ts_ms`; run before every intent, so a quiet
/// table simply notices the expiry at the next action.
pub(crate) fn expire_by_clock(state: &GameState, ts_ms: i64) -> Vec<Event> {
    state
        .effects
        .values()
        .filter(
            |e| matches!(e.duration, EffectDuration::RealTime { until_ts } if until_ts <= ts_ms),
        )
        .map(|e| Event::TokenEffectRemoved {
            id: e.id,
            expired: true,
        })
        .collect()
}

/// A new round started on `map_id`: every `Rounds` effect of its tokens counts down.
pub(crate) fn tick_round(work: &mut GameState, map_id: MapId) -> Vec<Event> {
    let due: Vec<_> = work
        .effects
        .values()
        .filter(|e| matches!(e.duration, EffectDuration::Rounds { .. }))
        .filter(|e| {
            work.tokens
                .get(&e.token)
                .is_some_and(|t| t.map_id == map_id)
        })
        .map(|e| e.id)
        .collect();
    let mut events = Vec::new();
    for id in due {
        if let Some(event) = countdown(work, id) {
            evolve(work, &event);
            events.push(event);
        }
    }
    events
}

/// `token`'s turn started: its recurring resource changes apply, then its `Turns` effects
/// count down.
pub(crate) fn tick_turn(work: &mut GameState, token: TokenId, rng: &mut dyn GameRng) -> Vec<Event> {
    let due: Vec<TokenEffectDto> = work
        .effects
        .values()
        .filter(|e| e.token == token)
        .cloned()
        .collect();
    let mut events = Vec::new();
    for effect in due {
        // Re-read per effect: an earlier tick of the same turn may have changed the sheet.
        let character = work
            .tokens
            .get(&token)
            .and_then(|t| t.character_id)
            .and_then(|id| work.characters.get(&id))
            .cloned();
        if let (Some(character), Some(resource), Some(dice)) =
            (character, effect.resource.clone(), effect.dice.clone())
        {
            for event in crate::triggers::change_resource(
                work,
                &character,
                &resource,
                &dice,
                effect.heal,
                &std::collections::BTreeMap::new(),
                rng,
            ) {
                evolve(work, &event);
                events.push(event);
            }
        }
        if matches!(effect.duration, EffectDuration::Turns { .. })
            && let Some(event) = countdown(work, effect.id)
        {
            evolve(work, &event);
            events.push(event);
        }
    }
    events
}

/// `remaining - 1`, or the removal when it reaches 0. Real-time effects never count down.
fn countdown(work: &GameState, id: dnd_protocol::EffectId) -> Option<Event> {
    let effect = work.effects.get(&id)?;
    let remaining = match effect.duration {
        EffectDuration::Rounds { remaining } | EffectDuration::Turns { remaining } => remaining,
        EffectDuration::RealTime { .. } => return None,
    };
    if remaining <= 1 {
        return Some(Event::TokenEffectRemoved { id, expired: true });
    }
    let duration = match effect.duration {
        EffectDuration::Rounds { .. } => EffectDuration::Rounds {
            remaining: remaining - 1,
        },
        EffectDuration::Turns { .. } => EffectDuration::Turns {
            remaining: remaining - 1,
        },
        EffectDuration::RealTime { .. } => return None,
    };
    let mut next = effect.clone();
    next.duration = duration;
    next.version = bump(next.version).ok()?;
    Some(Event::TokenEffectUpdated { effect: next })
}
