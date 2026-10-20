//! Who may do what (CLAUDE.md §5). Clients only hide UI; this is the actual check.

use dnd_protocol::{Intent, RejectReason, Role, TokenLayer, UserId};

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
            _ => Err(RejectReason::Forbidden),
        },
    }
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
