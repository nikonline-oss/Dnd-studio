//! Combat and initiative (ADR-0020): one combat per map, combatants in initiative order, turns and
//! rounds. Rights: GM roles do everything; a player rolls initiative for their own character and
//! ends their own turn (`authorize`).

use std::cmp::Ordering;

use dnd_dice::Expr;
use dnd_protocol::{
    ChatKind, ChatMessageDto, ChatVisibility, CombatDto, CombatId, CombatantDto, CombatantId,
    Event, Intent, MAX_COMBATANTS, RejectReason, TokenLayer, Version, sheet_field,
};

use crate::apply::{bump, valid_name};
use crate::chat::{DiceRngAdapter, message_id, posted, roll_dto, sender_name};
use crate::permissions::Actor;
use crate::plugins::CharacterFields;
use crate::rng::GameRng;
use crate::state::GameState;

/// The initiative formula for a character sheet. Rule-system agnostic default: `1d20`, plus the
/// sheet's integer `initiative` field when it has one. Stage 3 takes it from the system plugin
/// (ADR-0005).
pub fn initiative_expression(sheet: Option<&str>) -> &'static str {
    match sheet.map(|s| sheet_field(s, "initiative")) {
        Some(Ok(_)) => "1d20 + @initiative",
        _ => "1d20",
    }
}

/// Turn order: higher initiative first, combatants without initiative last, ties by joining
/// order, then by id (total and deterministic).
pub fn turn_order_cmp(a: &CombatantDto, b: &CombatantDto) -> Ordering {
    match (a.initiative, b.initiative) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then(a.sort_order.cmp(&b.sort_order))
    .then(a.id.cmp(&b.id))
}

/// Combatants of `combat` in turn order.
pub fn turn_order(state: &GameState, combat: CombatId) -> Vec<&CombatantDto> {
    let mut list: Vec<_> = state
        .combatants
        .values()
        .filter(|c| c.combat_id == combat)
        .collect();
    list.sort_by(|a, b| turn_order_cmp(a, b));
    list
}

pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    Some(match intent {
        Intent::StartCombat { id, map_id } => {
            if !state.maps.contains_key(map_id) {
                return Some(Err(RejectReason::NotFound));
            }
            if state.combats.values().any(|c| c.map_id == *map_id) {
                return Some(Err(RejectReason::AlreadyExists));
            }
            let combat = CombatDto {
                id: *id,
                map_id: *map_id,
                round: 1,
                turn: None,
                version: Version::INITIAL,
            };
            let mut events = vec![Event::CombatStarted {
                combat: combat.clone(),
            }];
            let mut work = state.clone();
            crate::evolve::evolve(&mut work, &events[0]);
            events.extend(crate::economy::refresh(&work, &combat, true, rng));
            Ok(events)
        }
        Intent::AddCombatant {
            id,
            combat_id,
            token_id,
            character_id,
            name,
        } => add(state, *id, *combat_id, *token_id, *character_id, name),
        Intent::RemoveCombatant { id } => remove(state, *id),
        Intent::RollInitiative { id } => roll(state, *id, actor, rng),
        Intent::SetInitiative { id, initiative } => {
            let Some(combatant) = state.combatants.get(id) else {
                return Some(Err(RejectReason::NotFound));
            };
            updated(combatant, *initiative)
        }
        Intent::NextTurn { combat_id } => next_turn(state, *combat_id, rng),
        Intent::EndCombat { id } => Ok(vec![Event::CombatEnded { id: *id }]),
        _ => return None,
    })
}

fn add(
    state: &GameState,
    id: CombatantId,
    combat_id: CombatId,
    token_id: Option<dnd_protocol::TokenId>,
    character_id: Option<dnd_protocol::CharacterId>,
    name: &str,
) -> Result<Vec<Event>, RejectReason> {
    let combat = state
        .combats
        .get(&combat_id)
        .ok_or(RejectReason::NotFound)?;
    let members: Vec<_> = state
        .combatants
        .values()
        .filter(|c| c.combat_id == combat_id)
        .collect();
    if members.len() >= MAX_COMBATANTS {
        return Err(RejectReason::LimitExceeded {
            limit: "combatants".to_owned(),
        });
    }
    if let Some(token_id) = token_id {
        let token = state.tokens.get(&token_id).ok_or(RejectReason::NotFound)?;
        if token.map_id != combat.map_id {
            return Err(RejectReason::invalid("token_id"));
        }
        if members.iter().any(|c| c.token_id == Some(token_id)) {
            return Err(RejectReason::AlreadyExists);
        }
    }
    if let Some(character_id) = character_id
        && !state.characters.contains_key(&character_id)
    {
        return Err(RejectReason::NotFound);
    }
    let sort_order = members
        .iter()
        .map(|c| c.sort_order)
        .max()
        .map_or(0, |m| m + 1);
    Ok(vec![Event::CombatantAdded {
        combatant: CombatantDto {
            id,
            combat_id,
            token_id,
            character_id,
            name: valid_name(name, "name")?,
            initiative: None,
            sort_order,
            version: Version::INITIAL,
        },
    }])
}

fn remove(state: &GameState, id: CombatantId) -> Result<Vec<Event>, RejectReason> {
    let combatant = state.combatants.get(&id).ok_or(RejectReason::NotFound)?;
    let combat = state
        .combats
        .get(&combatant.combat_id)
        .ok_or(RejectReason::NotFound)?;
    let mut events = Vec::new();
    // Removing whoever's turn it is hands the turn to the next one (same round).
    if combat.turn == Some(id) {
        let order = turn_order(state, combat.id);
        let pos = order.iter().position(|c| c.id == id);
        let next = pos
            .and_then(|p| order.get(p + 1).or_else(|| order.first()))
            .map(|c| c.id)
            .filter(|next| *next != id);
        events.push(Event::CombatUpdated {
            combat: CombatDto {
                turn: next,
                version: bump(combat.version)?,
                ..combat.clone()
            },
        });
    }
    events.push(Event::CombatantRemoved { id });
    Ok(events)
}

fn roll(
    state: &GameState,
    id: CombatantId,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let combatant = state.combatants.get(&id).ok_or(RejectReason::NotFound)?;
    let sheet = combatant
        .character_id
        .and_then(|c| state.characters.get(&c))
        .map(|c| c.sheet_json.as_str());
    // The system's formula (ADR-0023), or the built-in one for campaigns without a system.
    let expression = state
        .rules()
        .map_or_else(|| initiative_expression(sheet), |r| r.initiative.as_str());
    let expr = Expr::parse(expression).map_err(|_| RejectReason::invalid("expression"))?;
    let roll = if let Some(c) = combatant
        .character_id
        .and_then(|id| state.characters.get(&id))
    {
        crate::modifiers::roll(
            state,
            expression,
            crate::modifiers::RollContext {
                actor: c,
                target: None,
                kind: dnd_protocol::RollKind::Initiative,
                tags: &[],
                distance: None,
            },
            rng,
        )?
    } else {
        expr.roll(
            &CharacterFields::new(state, sheet),
            &mut DiceRngAdapter(rng),
        )
        .map_err(|_| RejectReason::invalid("expression"))?
    };
    // A combatant the players cannot see rolls where they cannot see it either.
    let hidden = combatant
        .token_id
        .and_then(|t| state.tokens.get(&t))
        .is_some_and(|t| t.layer == TokenLayer::Gm || !t.visible);
    let mut events = updated(combatant, Some(roll.total))?;
    events.push(posted(ChatMessageDto {
        id: message_id(rng),
        kind: ChatKind::Roll,
        sender: Some(actor.user_id),
        sender_name: if combatant.name.is_empty() {
            sender_name(state, actor)
        } else {
            combatant.name.clone()
        },
        body: expression.to_owned(),
        roll: Some(roll_dto(&roll)),
        system: None,
        visibility: if hidden {
            ChatVisibility::Gm
        } else {
            ChatVisibility::All
        },
        related_characters: combatant.character_id.into_iter().collect(),
        at_ms: None,
    }));
    Ok(events)
}

fn updated(combatant: &CombatantDto, initiative: Option<i64>) -> Result<Vec<Event>, RejectReason> {
    Ok(vec![Event::CombatantUpdated {
        combatant: CombatantDto {
            initiative,
            version: bump(combatant.version)?,
            ..combatant.clone()
        },
    }])
}

fn next_turn(
    state: &GameState,
    combat_id: CombatId,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let combat = state
        .combats
        .get(&combat_id)
        .ok_or(RejectReason::NotFound)?;
    let order = turn_order(state, combat_id);
    let current = combat
        .turn
        .and_then(|id| order.iter().position(|c| c.id == id));
    let start = current.map_or(0, |p| p + 1);
    let next = (0..order.len())
        .find_map(|offset| {
            let index = (start + offset) % order.len();
            let c = order[index];
            let dead = c
                .character_id
                .and_then(|id| state.characters.get(&id))
                .is_some_and(|c| {
                    crate::rules_view::life(state, c) == dnd_protocol::LifeStatus::Dead
                });
            (!dead).then_some((c.id, current.is_some() && start + offset >= order.len()))
        })
        .ok_or_else(|| RejectReason::invalid("combatants"))?;
    let turn = next.0;
    let round = combat
        .round
        .checked_add(i64::from(next.1))
        .ok_or_else(|| RejectReason::invalid("round"))?;
    let mut work = state.clone();
    let updated = Event::CombatUpdated {
        combat: CombatDto {
            turn: Some(turn),
            round,
            version: bump(combat.version)?,
            ..combat.clone()
        },
    };
    let mut events = vec![updated];
    for event in &events {
        crate::evolve::evolve(&mut work, event);
    }
    if let Some(combat) = work.combats.get(&combat_id) {
        let budgets = crate::economy::refresh(&work, combat, false, rng);
        for event in budgets {
            crate::evolve::evolve(&mut work, &event);
            events.push(event);
        }
    }
    // A wrapped order opens a new round: round effects of the map's tokens count down.
    if round > combat.round {
        for event in crate::effects::tick_round(&mut work, combat.map_id) {
            crate::evolve::evolve(&mut work, &event);
            events.push(event);
        }
    }
    // The turn's combatant: recurring changes of their token's effects, then the countdown.
    let token = work
        .combatants
        .get(&turn)
        .and_then(|c| c.token_id)
        .filter(|id| work.tokens.contains_key(id));
    if let Some(token) = token {
        for event in crate::effects::tick_turn(&mut work, token, rng) {
            crate::evolve::evolve(&mut work, &event);
            events.push(event);
        }
    }
    Ok(events)
}
