//! Character intents (CLAUDE.md §5): draft → pending_approval → active → archived, approved by
//! the GM. Permissions are checked in `permissions::authorize` before this runs.

use dnd_protocol::{
    CharacterDto, CharacterStatus, Event, Intent, MAX_CONDITIONS, RejectReason, SystemMessage,
    Version, validate_sheet_json,
};

use crate::apply::{bump, valid_name};
use crate::permissions::Actor;
use crate::rng::GameRng;
use crate::state::GameState;

/// Events for a character intent; `None` if `intent` is not about characters.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    actor: &Actor,
    rng: &mut dyn GameRng,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateCharacter {
            id,
            character_kind,
            name,
            owner,
            sheet_json,
        } => (|| {
            if let Some(owner) = owner
                && !state.users.contains_key(owner)
            {
                return Err(RejectReason::NotFound);
            }
            validate_sheet_json(sheet_json)?;
            let sheet_json = crate::plugins::normalize_sheet(state, sheet_json)?;
            // The GM's own characters skip review; players start with a draft.
            let status = if actor.role.is_game_master() {
                CharacterStatus::Active
            } else {
                CharacterStatus::Draft
            };
            Ok(vec![Event::CharacterCreated {
                character: CharacterDto {
                    id: *id,
                    kind: *character_kind,
                    name: valid_name(name, "name")?,
                    owner: *owner,
                    status,
                    sheet_json,
                    conditions: Vec::new(),
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::DeleteCharacter { id } => delete(state, *id),
        Intent::UpdateCharacter {
            id,
            name,
            sheet_json,
        } => change(state, *id, |c| {
            if name.is_none() && sheet_json.is_none() {
                return Err(RejectReason::invalid("intent"));
            }
            if let Some(name) = name {
                c.name = valid_name(name, "name")?;
            }
            if let Some(sheet) = sheet_json {
                validate_sheet_json(sheet)?;
                c.sheet_json = crate::plugins::normalize_sheet(state, sheet)?;
            }
            Ok(())
        }),
        Intent::SetConditions { id, conditions } => change(state, *id, |c| {
            c.conditions = valid_conditions(state, conditions)?;
            Ok(())
        }),
        Intent::GiveItem { id, name } => change(state, *id, |c| {
            let name = valid_name(name, "name")?;
            c.sheet_json = crate::plugins::give_item(state, &c.sheet_json, &name)?;
            Ok(())
        }),
        Intent::SubmitCharacter { id } => announce(
            transition(
                state,
                *id,
                &[CharacterStatus::Draft],
                CharacterStatus::PendingApproval,
            ),
            |name| SystemMessage::CharacterSubmitted { name },
            rng,
        ),
        Intent::ApproveCharacter { id } => announce(
            transition(
                state,
                *id,
                &[CharacterStatus::PendingApproval],
                CharacterStatus::Active,
            ),
            |name| SystemMessage::CharacterApproved { name },
            rng,
        ),
        Intent::ReturnCharacter { id } => announce(
            transition(
                state,
                *id,
                &[CharacterStatus::PendingApproval],
                CharacterStatus::Draft,
            ),
            |name| SystemMessage::CharacterReturned { name },
            rng,
        ),
        Intent::ArchiveCharacter { id } => transition(
            state,
            *id,
            &[
                CharacterStatus::Draft,
                CharacterStatus::PendingApproval,
                CharacterStatus::Active,
            ],
            CharacterStatus::Archived,
        ),
        Intent::AssignCharacterOwner { id, owner } => change(state, *id, |c| {
            if let Some(owner) = owner
                && !state.users.contains_key(owner)
            {
                return Err(RejectReason::NotFound);
            }
            c.owner = *owner;
            Ok(())
        }),
        _ => return None,
    };
    Some(result)
}

/// Applies `edit` to a copy of the character and emits `CharacterUpdated` with a bumped version.
/// Conditions of the campaign's system, each once, in the system's order.
fn valid_conditions(state: &GameState, ids: &[String]) -> Result<Vec<String>, RejectReason> {
    let invalid = || RejectReason::invalid("conditions");
    let rules = state.rules().ok_or_else(invalid)?;
    if ids.len() > MAX_CONDITIONS
        || ids
            .iter()
            .any(|id| !rules.conditions.iter().any(|c| c.id == *id))
    {
        return Err(invalid());
    }
    Ok(rules
        .conditions
        .iter()
        .filter(|c| ids.contains(&c.id))
        .map(|c| c.id.clone())
        .collect())
}

fn delete(state: &GameState, id: dnd_protocol::CharacterId) -> Result<Vec<Event>, RejectReason> {
    let mut events = Vec::new();
    for token in state.tokens.values().filter(|t| t.character_id == Some(id)) {
        let mut token = token.clone();
        token.character_id = None;
        token.version = bump(token.version)?;
        events.push(Event::TokenUpdated { token });
    }
    for combatant in state
        .combatants
        .values()
        .filter(|c| c.character_id == Some(id))
    {
        let mut combatant = combatant.clone();
        combatant.character_id = None;
        combatant.version = bump(combatant.version)?;
        events.push(Event::CombatantUpdated { combatant });
    }
    let nodes: Vec<_> = state.board_nodes.values().filter(|n| matches!(&n.content, dnd_protocol::BoardNodeContent::Link { target: dnd_protocol::BoardLink::Character { id: target } } if *target == id)).map(|n| n.id).collect();
    for node in nodes {
        events.extend(crate::boards::delete_node(state, node));
    }
    events.push(Event::CharacterDeleted { id });
    Ok(events)
}

fn change(
    state: &GameState,
    id: dnd_protocol::CharacterId,
    edit: impl FnOnce(&mut CharacterDto) -> Result<(), RejectReason>,
) -> Result<Vec<Event>, RejectReason> {
    let mut character = state
        .characters
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    edit(&mut character)?;
    character.version = bump(character.version)?;
    Ok(vec![Event::CharacterUpdated { character }])
}

/// Status change allowed only from `from`; otherwise `Rejected { invalid: status }`.
fn transition(
    state: &GameState,
    id: dnd_protocol::CharacterId,
    from: &[CharacterStatus],
    to: CharacterStatus,
) -> Result<Vec<Event>, RejectReason> {
    change(state, id, |c| {
        if !from.contains(&c.status) {
            return Err(RejectReason::invalid("status"));
        }
        c.status = to;
        Ok(())
    })
}

/// Adds a system chat message about a review step to a successful transition.
fn announce(
    result: Result<Vec<Event>, RejectReason>,
    message: impl FnOnce(String) -> SystemMessage,
    rng: &mut dyn GameRng,
) -> Result<Vec<Event>, RejectReason> {
    let mut events = result?;
    let name = events.iter().find_map(|e| match e {
        Event::CharacterUpdated { character } => Some(character.name.clone()),
        _ => None,
    });
    if let Some(name) = name {
        events.push(crate::chat::system(message(name), rng));
    }
    Ok(events)
}
