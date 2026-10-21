//! Shared fixtures for game-core tests.

#![allow(dead_code)]

use dnd_game_core::{Actor, GameState, RuleError, SplitMix64, apply, evolve_all};
use dnd_protocol::arbitrary::{pooled_character_id, pooled_map_id, pooled_token_id};
use dnd_protocol::*;
use uuid::Uuid;

pub const GM: UserId = UserId(Uuid::from_u128(0xA1));
pub const CO_GM: UserId = UserId(Uuid::from_u128(0xA2));
pub const PLAYER: UserId = UserId(Uuid::from_u128(0xA3));
pub const OTHER_PLAYER: UserId = UserId(Uuid::from_u128(0xA4));
pub const SPECTATOR: UserId = UserId(Uuid::from_u128(0xA5));

pub fn actor(role: Role) -> Actor {
    let user_id = match role {
        Role::Gm => GM,
        Role::CoGm => CO_GM,
        Role::Player => PLAYER,
        Role::Spectator => SPECTATOR,
    };
    Actor { user_id, role }
}

/// Character 0 belongs to `PLAYER`, character 1 to `OTHER_PLAYER`.
pub fn base_state() -> GameState {
    let mut state = GameState::new("Test campaign");
    for (n, owner) in [(0, PLAYER), (1, OTHER_PLAYER)] {
        let id = pooled_character_id(n);
        state.characters.insert(
            id,
            CharacterDto {
                id,
                kind: CharacterKind::Pc,
                name: format!("Hero {n}"),
                owner: Some(owner),
                status: CharacterStatus::Active,
                sheet_json: "{}".into(),
                version: Version::INITIAL,
            },
        );
    }
    for (id, role) in [
        (GM, Role::Gm),
        (CO_GM, Role::CoGm),
        (PLAYER, Role::Player),
        (OTHER_PLAYER, Role::Player),
        (SPECTATOR, Role::Spectator),
    ] {
        state.users.insert(
            id,
            UserDto {
                id,
                name: format!("{role:?}"),
                role,
            },
        );
    }
    state
}

pub fn map(n: u128, visible_to_players: bool) -> MapDto {
    MapDto {
        id: pooled_map_id(n),
        name: format!("Map {n}"),
        background_asset_id: None,
        width: 4000,
        height: 3000,
        grid: Grid::default(),
        visible_to_players,
        version: Version::INITIAL,
    }
}

pub fn token(n: u128, map_n: u128, character: Option<u128>, layer: TokenLayer) -> TokenDto {
    TokenDto {
        id: pooled_token_id(n),
        map_id: pooled_map_id(map_n),
        character_id: character.map(pooled_character_id),
        asset_id: None,
        name: format!("Token {n}"),
        x: 0,
        y: 0,
        rotation: 0.0,
        scale: 1.0,
        layer,
        visible: true,
        version: Version::INITIAL,
    }
}

/// Map 0 (visible) with: token 0 — player's hero, token 1 — other player's hero,
/// token 2 — player's hero on the GM layer, token 3 — NPC.
/// Map 1 (hidden) with token 4 — player's hero.
pub fn table_state() -> GameState {
    let mut state = base_state();
    for m in [map(0, true), map(1, false)] {
        state.maps.insert(m.id, m);
    }
    for t in [
        token(0, 0, Some(0), TokenLayer::Tokens),
        token(1, 0, Some(1), TokenLayer::Tokens),
        token(2, 0, Some(0), TokenLayer::Gm),
        token(3, 0, None, TokenLayer::Tokens),
        token(4, 1, Some(0), TokenLayer::Tokens),
    ] {
        state.tokens.insert(t.id, t);
    }
    state
}

pub fn envelope(intent: Intent, expected_version: Option<Version>) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId(dnd_protocol::arbitrary::v7(0xFFFF)),
        expected_version,
        intent,
    }
}

/// Applies an intent and, on success, evolves the state.
pub fn run(
    state: &mut GameState,
    role: Role,
    intent: Intent,
    expected_version: Option<Version>,
) -> Result<Vec<Event>, RuleError> {
    let events = apply(
        state,
        &envelope(intent, expected_version),
        &actor(role),
        &mut SplitMix64::new(0),
    )?;
    evolve_all(state, &events);
    Ok(events)
}
