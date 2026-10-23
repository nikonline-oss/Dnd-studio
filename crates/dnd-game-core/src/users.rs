//! Users of a campaign. They are not created by intents: the session server adds a participant
//! who joins (ADR-0022).

use dnd_protocol::{Event, RejectReason, Role, UserDto, UserId};

use crate::apply::valid_name;
use crate::state::GameState;

/// Events for `id` joining as `name` with `role`: none when the user already exists with the
/// same name and role (a reconnect), otherwise `UserJoined` with the validated name.
pub fn join(
    state: &GameState,
    id: UserId,
    name: &str,
    role: Role,
) -> Result<Vec<Event>, RejectReason> {
    let user = UserDto {
        id,
        name: valid_name(name, "name")?,
        role,
    };
    if state.users.get(&id) == Some(&user) {
        return Ok(Vec::new());
    }
    Ok(vec![Event::UserJoined { user }])
}
