//! Chat and dice rolls (CLAUDE.md §12): public messages, rolls with full dice detail, hidden GM
//! rolls and system messages posted by the core. Chat history is not part of `GameState`: no rule
//! depends on it, and it grows without bound.

use dnd_dice::{DiceRng, Expr, Sign, TermDetail};
use dnd_protocol::{
    ChatKind, ChatMessageDto, ChatMessageId, ChatVisibility, DiceRollDto, DiceTermDto, DieDto,
    Event, Intent, MAX_CHAT_LEN, RejectReason, SystemMessage, uuid_v7_from_random,
};

use crate::permissions::Actor;
use crate::plugins::CharacterFields;
use crate::rng::GameRng;
use crate::state::GameState;

/// Lets `dnd-dice` draw from the core's injected RNG (ADR-0001).
pub(crate) struct DiceRngAdapter<'a>(pub(crate) &'a mut dyn GameRng);

impl DiceRng for DiceRngAdapter<'_> {
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
}

/// Events for a chat intent; `None` if `intent` is not about chat.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::SendMessage { id, body } => (|| {
            let body = body.trim();
            if body.is_empty() || body.chars().count() > MAX_CHAT_LEN {
                return Err(RejectReason::invalid("body"));
            }
            Ok(vec![posted(ChatMessageDto {
                id: *id,
                kind: ChatKind::Text,
                sender: Some(actor.user_id),
                sender_name: sender_name(state, actor),
                body: body.to_owned(),
                roll: None,
                system: None,
                visibility: ChatVisibility::All,
                at_ms: None,
            })])
        })(),
        Intent::RollDice {
            id,
            expression,
            hidden,
            character_id,
            roll: roll_id,
        } => (|| {
            if *hidden && !actor.role.is_game_master() {
                return Err(RejectReason::Forbidden);
            }
            let sheet = match character_id {
                None => None,
                Some(cid) => {
                    let character = state.characters.get(cid);
                    let allowed = actor.role.is_game_master()
                        || character.is_some_and(|c| c.owner == Some(actor.user_id));
                    if !allowed {
                        return Err(RejectReason::Forbidden);
                    }
                    Some(character.ok_or(RejectReason::NotFound)?.sheet_json.as_str())
                }
            };
            // A system roll uses the system's formula: a client cannot pass off "1d20 + 100" as a
            // perception check to a trigger (ADR-0024).
            let expression = match roll_id {
                None => expression.as_str(),
                Some(roll_id) => {
                    let def = state
                        .rules()
                        .and_then(|r| r.rolls.iter().find(|r| r.id == *roll_id))
                        .ok_or_else(|| RejectReason::invalid("roll"))?;
                    if def.dice.trim() != expression.trim() {
                        return Err(RejectReason::invalid("expression"));
                    }
                    def.dice.as_str()
                }
            };
            let expr = Expr::parse(expression).map_err(|_| RejectReason::invalid("expression"))?;
            let roll = expr
                .roll(
                    &CharacterFields::new(state, sheet),
                    &mut DiceRngAdapter(rng),
                )
                .map_err(|_| RejectReason::invalid("expression"))?;
            Ok(vec![posted(ChatMessageDto {
                id: *id,
                kind: ChatKind::Roll,
                sender: Some(actor.user_id),
                sender_name: sender_name(state, actor),
                body: expression.trim().to_owned(),
                roll: Some(roll_dto(&roll)),
                system: None,
                visibility: if *hidden {
                    ChatVisibility::Gm
                } else {
                    ChatVisibility::All
                },
                at_ms: None,
            })])
        })(),
        _ => return None,
    };
    Some(result)
}

/// A system message with an id drawn from the core's RNG (deterministic for a seed).
pub(crate) fn system(system: SystemMessage, rng: &mut dyn GameRng) -> Event {
    posted(ChatMessageDto {
        id: message_id(rng),
        kind: ChatKind::System,
        sender: None,
        sender_name: String::new(),
        body: String::new(),
        roll: None,
        system: Some(system),
        visibility: ChatVisibility::All,
        at_ms: None,
    })
}

/// A chat message id drawn from the core's RNG (deterministic for a seed).
pub(crate) fn message_id(rng: &mut dyn GameRng) -> ChatMessageId {
    let bits = (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64());
    ChatMessageId(uuid_v7_from_random(bits))
}

pub(crate) fn posted(message: ChatMessageDto) -> Event {
    Event::ChatMessagePosted { message }
}

pub(crate) fn sender_name(state: &GameState, actor: &Actor) -> String {
    state
        .users
        .get(&actor.user_id)
        .map(|u| u.name.clone())
        .unwrap_or_default()
}

pub(crate) fn roll_dto(roll: &dnd_dice::Roll) -> DiceRollDto {
    DiceRollDto {
        expression: roll.expression.to_string(),
        terms: roll
            .terms
            .iter()
            .map(|t| {
                let (notation, dice) = match &t.term {
                    TermDetail::Dice { notation, dice, .. } => (
                        notation.clone(),
                        dice.iter()
                            .map(|d| DieDto {
                                value: d.value,
                                kept: d.kept,
                            })
                            .collect(),
                    ),
                    TermDetail::Constant { value } => (value.to_string(), Vec::new()),
                    TermDetail::Field { path, .. } => (format!("@{path}"), Vec::new()),
                };
                DiceTermDto {
                    negative: t.sign == Sign::Minus,
                    notation,
                    subtotal: t.subtotal,
                    dice,
                }
            })
            .collect(),
        total: roll.total,
    }
}
