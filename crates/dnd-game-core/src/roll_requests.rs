//! Manual roll requests (ADR: triggers ask, controllers roll). A `change_resource` step with
//! `manual: true` creates one request per target character instead of rolling; the controller
//! (the character's owner, or GM roles for NPCs) fulfills it with `FulfillRollRequest` and the
//! core rolls and applies; a GM role cancels with `CancelRollRequest`, reversing an applied delta.

use crate::permissions::Actor;
use dnd_dice::{Expr, FieldLookup, Term};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::RequestRolls {
            id,
            label,
            dice,
            characters,
        } => request_checks(state, *id, label, dice, characters, rng),
        Intent::FulfillRollRequest { id } => fulfill(state, *id, actor, rng),
        Intent::CancelRollRequest { id } => cancel(state, *id, rng),
        _ => return None,
    };
    Some(result)
}

/// The controller rolls the request's dice; on a hit (or without a check) the resource changes.
fn fulfill(
    state: &GameState,
    id: dnd_protocol::RollRequestId,
    actor: &Actor,
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
        sender: request.group.map(|_| actor.user_id),
        sender_name: if request.group.is_some() {
            crate::chat::sender_name(state, actor)
        } else {
            request.label.clone()
        },
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
    if hit && request.group.is_none() {
        let damage = if request.hit.is_none() {
            // A plain manual roll applies the displayed result; only attacks roll damage separately.
            roll.total.max(0)
        } else {
            roll_dice(
                state,
                &request.damage_dice,
                Some(&character),
                &hit_vars,
                rng,
            )
            .map(|roll| roll.total.max(0))
            .ok_or_else(|| RejectReason::invalid("steps"))?
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
    if request.group.is_some()
        && let Event::ChatMessagePosted { message } = &mut events[0]
    {
        message.visibility = ChatVisibility::Gm;
        message.related_characters = vec![request.character];
    }
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

fn request_checks(
    state: &GameState,
    id: dnd_protocol::RollRequestId,
    label: &str,
    dice: &str,
    characters: &[dnd_protocol::CharacterId],
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let unique: BTreeSet<_> = characters.iter().copied().collect();
    if unique.len() != characters.len() {
        return Err(RejectReason::invalid("characters"));
    }
    if state.roll_requests.len() + unique.len() > dnd_protocol::MAX_ROLL_REQUESTS {
        return Err(RejectReason::LimitExceeded {
            limit: "roll_requests".into(),
        });
    }
    let expression = Expr::parse(dice.trim()).map_err(|_| RejectReason::invalid("dice"))?;
    // Validate every target before emitting anything or drawing IDs from the RNG.
    for id in &unique {
        let character = state.characters.get(id).ok_or(RejectReason::NotFound)?;
        if character.status != dnd_protocol::CharacterStatus::Active {
            return Err(RejectReason::invalid("characters"));
        }
        let fields = crate::plugins::CharacterFields::new(state, Some(&character.sheet_json));
        for term in expression.terms() {
            if let Term::Field(path) = &term.term {
                let value = fields
                    .lookup(path)
                    .map_err(|_| RejectReason::invalid("dice"))?;
                if !dnd_protocol::validate::is_js_safe(value) {
                    return Err(RejectReason::invalid("dice"));
                }
            }
        }
    }
    let mut used = BTreeSet::from([id.0]);
    let mut events = Vec::new();
    for (index, character) in unique.iter().enumerate() {
        let request_id = if index == 0 {
            id
        } else {
            let mut found = None;
            for _ in 0..1024 {
                let bits = (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64());
                let candidate = dnd_protocol::uuid_v7_from_random(bits);
                if !crate::apply::id_in_use(state, candidate) && used.insert(candidate) {
                    found = Some(dnd_protocol::RollRequestId(candidate));
                    break;
                }
            }
            found.ok_or(RejectReason::AlreadyExists)?
        };
        events.push(Event::RollRequested {
            request: dnd_protocol::RollRequestDto {
                id: request_id,
                label: label.trim().to_owned(),
                trigger: None,
                group: Some(id),
                character: *character,
                token: None,
                dice: dice.trim().to_owned(),
                hit: None,
                resource: String::new(),
                damage_dice: "0".into(),
                heal: false,
                status: RollRequestStatus::Pending,
                total: None,
                delta: None,
                version: dnd_protocol::Version::INITIAL,
            },
        });
        let mut event = crate::chat::system_for_character(
            *character,
            SystemMessage::RollRequested {
                name: state
                    .characters
                    .get(character)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
                label: label.trim().to_owned(),
            },
            rng,
        );
        if let Event::ChatMessagePosted { message } = &mut event {
            message.visibility = ChatVisibility::Gm;
        }
        events.push(event);
    }
    Ok(events)
}

/// Complete a group once, also when deletion of a character cancels the final pending request.
pub(crate) fn summaries(state: &GameState, events: &[Event], rng: &mut dyn GameRng) -> Vec<Event> {
    let groups: BTreeSet<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::RollRequestUpdated { request } => request.group,
            _ => None,
        })
        .collect();
    let mut result = Vec::new();
    for group in groups {
        let mut requests: BTreeMap<_, _> = state
            .roll_requests
            .values()
            .filter(|r| r.group == Some(group))
            .map(|r| (r.id, r.clone()))
            .collect();
        if !requests
            .values()
            .any(|r| r.status == RollRequestStatus::Pending)
        {
            continue;
        }
        for event in events {
            if let Event::RollRequestUpdated { request } = event
                && request.group == Some(group)
            {
                requests.insert(request.id, request.clone());
            }
        }
        if requests
            .values()
            .any(|r| r.status == RollRequestStatus::Pending)
        {
            continue;
        }
        let label = requests
            .values()
            .next()
            .map(|r| r.label.clone())
            .unwrap_or_default();
        let results: Vec<_> = requests
            .values()
            .map(|r| dnd_protocol::RollCheckResult {
                character: r.character,
                name: state
                    .characters
                    .get(&r.character)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
                total: (r.status == RollRequestStatus::Rolled)
                    .then_some(r.total)
                    .flatten(),
            })
            .collect();
        let related = results.iter().map(|r| r.character).collect();
        let mut event = system(SystemMessage::RollsCollected { label, results }, rng);
        if let Event::ChatMessagePosted { message } = &mut event {
            message.visibility = ChatVisibility::Gm;
            message.related_characters = related;
        }
        result.push(event);
    }
    result
}
