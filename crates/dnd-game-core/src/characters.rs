//! Character intents (CLAUDE.md §5): draft → pending_approval → active → archived, approved by
//! the GM. Permissions are checked in `permissions::authorize` before this runs.

use dnd_protocol::{
    CharacterDto, CharacterStatus, Event, Intent, RejectReason, Version, validate_sheet_json,
};

use crate::apply::{bump, valid_name};
use crate::permissions::Actor;
use crate::state::GameState;

/// Events for a character intent; `None` if `intent` is not about characters.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    actor: &Actor,
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
                    sheet_json: sheet_json.clone(),
                    version: Version::INITIAL,
                },
            }])
        })(),
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
                c.sheet_json.clone_from(sheet);
            }
            Ok(())
        }),
        Intent::SubmitCharacter { id } => transition(
            state,
            *id,
            &[CharacterStatus::Draft],
            CharacterStatus::PendingApproval,
        ),
        Intent::ApproveCharacter { id } => transition(
            state,
            *id,
            &[CharacterStatus::PendingApproval],
            CharacterStatus::Active,
        ),
        Intent::ReturnCharacter { id } => transition(
            state,
            *id,
            &[CharacterStatus::PendingApproval],
            CharacterStatus::Draft,
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
