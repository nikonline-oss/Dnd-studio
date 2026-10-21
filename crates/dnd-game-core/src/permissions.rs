//! Who may do what (CLAUDE.md §5). Clients only hide UI; this is the actual check.

use dnd_protocol::{
    CharacterId, CharacterKind, CharacterStatus, Intent, RejectReason, Role, TokenLayer, UserId,
};

use crate::state::GameState;

/// The participant on whose behalf an intent is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Actor {
    pub user_id: UserId,
    pub role: Role,
}

/// Checks that `actor` may apply `intent` to `state`.
///
/// Runs before existence and version checks. For players, a missing token and someone else's token
/// are both `Forbidden`, so players cannot probe for hidden entities.
pub fn authorize(actor: &Actor, intent: &Intent, state: &GameState) -> Result<(), RejectReason> {
    match actor.role {
        // Co-GM differs from GM only for campaign deletion and GM role changes, which have no
        // intents yet.
        Role::Gm | Role::CoGm => Ok(()),
        Role::Spectator => Err(RejectReason::Forbidden),
        Role::Player => match intent {
            Intent::MoveToken { id, .. } if player_controls_token(actor.user_id, *id, state) => {
                Ok(())
            }
            // Players create only their own player characters (they start as drafts).
            Intent::CreateCharacter {
                character_kind: CharacterKind::Pc,
                owner: Some(owner),
                ..
            } if *owner == actor.user_id => Ok(()),
            // Their own characters, except while the GM reviews them or after archiving.
            Intent::UpdateCharacter { id, .. }
                if owns_character(actor.user_id, *id, state, |s| {
                    matches!(s, CharacterStatus::Draft | CharacterStatus::Active)
                }) =>
            {
                Ok(())
            }
            Intent::SubmitCharacter { id }
                if owns_character(actor.user_id, *id, state, |_| true) =>
            {
                Ok(())
            }
            _ => Err(RejectReason::Forbidden),
        },
    }
}

/// `user` owns character `id` and its status passes `status_ok`.
fn owns_character(
    user: UserId,
    id: CharacterId,
    state: &GameState,
    status_ok: impl Fn(CharacterStatus) -> bool,
) -> bool {
    state
        .characters
        .get(&id)
        .is_some_and(|c| c.owner == Some(user) && status_ok(c.status))
}

/// A player controls a token when it represents a character they own, sits on a map visible to
/// players and is not on the GM layer.
fn player_controls_token(user: UserId, token_id: dnd_protocol::TokenId, state: &GameState) -> bool {
    let Some(token) = state.tokens.get(&token_id) else {
        return false;
    };
    let owns_character = token
        .character_id
        .and_then(|c| state.characters.get(&c))
        .is_some_and(|c| c.owner == Some(user));
    let map_visible = state
        .maps
        .get(&token.map_id)
        .is_some_and(|m| m.visible_to_players);
    owns_character && map_visible && token.layer != TokenLayer::Gm
}
