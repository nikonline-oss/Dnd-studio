//! Manual roll requests (ADR: triggers ask, controllers roll). A `change_resource` step with
//! `manual: true` creates one request per target character instead of rolling; the controller
//! (the character's owner, or GM roles for NPCs) fulfills it with `FulfillRollRequest` and the
//! core rolls and applies; a GM role cancels with `CancelRollRequest`, reversing an applied delta.

use std::collections::BTreeMap;

use dnd_protocol::{
    ChatKind, ChatMessageDto, ChatVisibility, Event, Intent, RejectReason, RollRequestStatus,
    SystemMessage,
};

use crate::apply::bump;
use crate::chat::{message_id, posted, roll_dto, system};
use crate::rng::GameRng;
use crate::state::GameState;
use crate::triggers::{apply_resource_change_delta, expr_holds, roll_dice};

/// Events for a roll request intent; `None` if `intent` is not about roll requests.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::FulfillRollRequest { id } => fulfill(state, *id, rng),
        Intent::CancelRollRequest { id } => cancel(state, *id, rng),
        _ => return None,
    };
    Some(result)
}

/// The controller rolls the request's dice; on a hit (or without a check) the resource changes.
fn fulfill(
    state: &GameState,
    id: dnd_protocol::RollRequestId,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let request = state
        .roll_requests
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    if request.status != RollRequestStatus::Pending {
        return Err(RejectReason::invalid("id"));
    }
    let character = state
        .characters
        .get(&request.character)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    let vars = BTreeMap::new();
    let Some(roll) = roll_dice(state, &request.dice, Some(&character), &vars, rng) else {
        return Err(RejectReason::invalid("steps"));
    };
    let mut events = vec![posted(ChatMessageDto {
        id: message_id(rng),
        kind: ChatKind::Roll,
        sender: None,
        sender_name: request.label.clone(),
        body: request.dice.trim().to_owned(),
        roll: Some(roll_dto(&roll)),
        system: None,
        visibility: ChatVisibility::All,
        related_characters: vec![request.character],
        at_ms: None,
    })];
    let mut next = request.clone();
    next.total = Some(roll.total);
    let mut hit_vars = vars;
    hit_vars.insert("total".to_owned(), roll.total);
    let hit = request
        .hit
        .as_deref()
        .map(|hit| expr_holds(state, &character, &hit_vars, hit))
        .unwrap_or(true);
    if hit {
        let Some(damage) = roll_dice(
            state,
            &request.damage_dice,
            Some(&character),
            &hit_vars,
            rng,
        )
        .map(|roll| roll.total.max(0)) else {
            return Err(RejectReason::invalid("steps"));
        };
        let (apply_events, delta) = apply_resource_change_delta(
            state,
            &character,
            &request.resource,
            damage,
            request.heal,
            rng,
        );
        events.extend(apply_events);
        next.delta = delta;
    }
    next.status = RollRequestStatus::Rolled;
    next.version = bump(next.version)?;
    events.push(Event::RollRequestUpdated { request: next });
    Ok(events)
}

/// GM roles: drop a pending request, or reverse the delta of a rolled one.
fn cancel(
    state: &GameState,
    id: dnd_protocol::RollRequestId,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let request = state
        .roll_requests
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    let name = state
        .characters
        .get(&request.character)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut events = vec![system(
        SystemMessage::RollRequestCancelled {
            name,
            label: request.label.clone(),
        },
        rng,
    )];
    match request.status {
        RollRequestStatus::Pending => {}
        RollRequestStatus::Rolled => {
            if let Some(delta) = request.delta {
                let character = state
                    .characters
                    .get(&request.character)
                    .cloned()
                    .ok_or(RejectReason::NotFound)?;
                // Reverse what the roll did: damage heals back, healing is taken back.
                let (reverse, _) = apply_resource_change_delta(
                    state,
                    &character,
                    &request.resource,
                    delta.abs(),
                    !request.heal,
                    rng,
                );
                events.extend(reverse);
            }
        }
        RollRequestStatus::Cancelled => return Err(RejectReason::invalid("id")),
    }
    let mut next = request;
    next.status = RollRequestStatus::Cancelled;
    next.version = bump(next.version)?;
    events.push(Event::RollRequestUpdated { request: next });
    Ok(events)
}
