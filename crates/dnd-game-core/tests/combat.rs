//! Combat and initiative (ADR-0020): rights of all four roles, turn order, rounds, rolls.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{GameState, initiative_expression, turn_order};
use dnd_protocol::arbitrary::{pooled_character_id, pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;

const COMBAT: CombatId = CombatId(Uuid::from_u128(0x0190_a000_0000_7000_8000_0000_0000_0c01));

fn combatant_id(n: u128) -> CombatantId {
    CombatantId(v7(0xC000 + n))
}

fn start(state: &mut GameState) {
    run(
        state,
        Role::Gm,
        Intent::StartCombat {
            id: COMBAT,
            map_id: pooled_map_id(0),
        },
        None,
    )
    .unwrap();
}

/// Adds a combatant for token `n` of map 0 (tokens 0..=3, see `table_state`).
fn add(state: &mut GameState, n: u128) -> CombatantId {
    let token = &state.tokens[&pooled_token_id(n)];
    let intent = Intent::AddCombatant {
        id: combatant_id(n),
        combat_id: COMBAT,
        token_id: Some(token.id),
        character_id: token.character_id,
        name: token.name.clone(),
    };
    run(state, Role::Gm, intent, None).unwrap();
    combatant_id(n)
}

fn set(state: &mut GameState, id: CombatantId, initiative: Option<i64>) {
    let version = state.combatants[&id].version;
    run(
        state,
        Role::Gm,
        Intent::SetInitiative { id, initiative },
        Some(version),
    )
    .unwrap();
}

fn next(state: &mut GameState, role: Role) -> Result<Vec<Event>, RejectReason> {
    let version = state.combats[&COMBAT].version;
    run(
        state,
        role,
        Intent::NextTurn { combat_id: COMBAT },
        Some(version),
    )
}

/// Combat on map 0 with: token 0 (player's hero) at 12, token 3 (NPC) at 18, token 1 (other
/// player's hero) without initiative.
fn fight() -> GameState {
    let mut state = table_state();
    start(&mut state);
    let hero = add(&mut state, 0);
    let npc = add(&mut state, 3);
    add(&mut state, 1);
    set(&mut state, hero, Some(12));
    set(&mut state, npc, Some(18));
    state
}

fn order(state: &GameState) -> Vec<CombatantId> {
    turn_order(state, COMBAT).iter().map(|c| c.id).collect()
}

#[test]
fn only_gm_roles_run_the_combat() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = table_state();
        let started = run(
            &mut state,
            role,
            Intent::StartCombat {
                id: COMBAT,
                map_id: pooled_map_id(0),
            },
            None,
        );
        if !allowed {
            assert_eq!(started, Err(RejectReason::Forbidden), "{role:?}");
            continue;
        }
        started.unwrap();
        let id = combatant_id(3);
        run(
            &mut state,
            role,
            Intent::AddCombatant {
                id,
                combat_id: COMBAT,
                token_id: Some(pooled_token_id(3)),
                character_id: None,
                name: "Goblin".into(),
            },
            None,
        )
        .unwrap();
        run(
            &mut state,
            role,
            Intent::SetInitiative {
                id,
                initiative: Some(7),
            },
            Some(Version(1)),
        )
        .unwrap();
        run(
            &mut state,
            role,
            Intent::RollInitiative { id },
            Some(Version(2)),
        )
        .unwrap();
        run(
            &mut state,
            role,
            Intent::NextTurn { combat_id: COMBAT },
            Some(Version(1)),
        )
        .unwrap();
        run(
            &mut state,
            role,
            Intent::RemoveCombatant { id },
            Some(Version(3)),
        )
        .unwrap();
        run(
            &mut state,
            role,
            Intent::EndCombat { id: COMBAT },
            Some(Version(3)),
        )
        .unwrap();
        assert!(state.combats.is_empty() && state.combatants.is_empty());
    }
}

#[test]
fn players_and_spectators_cannot_manage_combatants() {
    let mut state = fight();
    for role in [Role::Player, Role::Spectator] {
        let hero = combatant_id(0);
        for (intent, version) in [
            (
                Intent::SetInitiative {
                    id: hero,
                    initiative: Some(30),
                },
                Some(Version(2)),
            ),
            (Intent::RemoveCombatant { id: hero }, Some(Version(2))),
            (Intent::EndCombat { id: COMBAT }, Some(Version(1))),
            (
                Intent::AddCombatant {
                    id: combatant_id(9),
                    combat_id: COMBAT,
                    token_id: None,
                    character_id: Some(pooled_character_id(0)),
                    name: "Me again".into(),
                },
                None,
            ),
        ] {
            assert_eq!(
                run(&mut state, role, intent.clone(), version),
                Err(RejectReason::Forbidden),
                "{role:?} {intent:?}"
            );
        }
    }
}

#[test]
fn a_player_rolls_only_for_their_own_character() {
    let mut state = fight();
    // Combatant 0 is the player's hero, 1 the other player's, 3 an NPC.
    let roll = |id| Intent::RollInitiative { id };
    run(
        &mut state,
        Role::Player,
        roll(combatant_id(0)),
        Some(Version(2)),
    )
    .unwrap();
    for other in [combatant_id(1), combatant_id(3)] {
        let version = state.combatants[&other].version;
        assert_eq!(
            run(&mut state, Role::Player, roll(other), Some(version)),
            Err(RejectReason::Forbidden)
        );
    }
    assert_eq!(
        run(
            &mut state,
            Role::Spectator,
            roll(combatant_id(0)),
            Some(Version(3))
        ),
        Err(RejectReason::Forbidden)
    );
}

#[test]
fn a_player_ends_only_their_own_turn() {
    let mut state = fight();
    // Order: NPC (18), hero (12), other hero (none). Nobody's turn yet: players cannot start.
    assert_eq!(next(&mut state, Role::Player), Err(RejectReason::Forbidden));
    next(&mut state, Role::Gm).unwrap(); // NPC's turn
    assert_eq!(next(&mut state, Role::Player), Err(RejectReason::Forbidden));
    next(&mut state, Role::Gm).unwrap(); // the player's hero
    next(&mut state, Role::Player).unwrap(); // ends their turn
    assert_eq!(state.combats[&COMBAT].turn, Some(combatant_id(1)));
    assert_eq!(next(&mut state, Role::Player), Err(RejectReason::Forbidden));
    assert_eq!(
        next(&mut state, Role::Spectator),
        Err(RejectReason::Forbidden)
    );
}

#[test]
fn turns_follow_initiative_and_wrap_into_a_new_round() {
    let mut state = fight();
    let (hero, npc, other) = (combatant_id(0), combatant_id(3), combatant_id(1));
    // Higher first; no initiative last.
    assert_eq!(order(&state), vec![npc, hero, other]);
    assert_eq!(state.combats[&COMBAT].turn, None);

    let mut turns = Vec::new();
    for _ in 0..4 {
        next(&mut state, Role::Gm).unwrap();
        let combat = &state.combats[&COMBAT];
        turns.push((combat.turn.unwrap(), combat.round));
    }
    assert_eq!(turns, vec![(npc, 1), (hero, 1), (other, 1), (npc, 2)]);
}

#[test]
fn ties_keep_the_joining_order() {
    let mut state = fight();
    set(&mut state, combatant_id(3), Some(12)); // NPC joined after the hero
    set(&mut state, combatant_id(1), Some(12));
    assert_eq!(
        order(&state),
        vec![combatant_id(0), combatant_id(3), combatant_id(1)]
    );
}

#[test]
fn a_stale_next_turn_is_rejected() {
    let mut state = fight();
    next(&mut state, Role::Gm).unwrap();
    // A second click based on the old combat version must not skip a turn.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::NextTurn { combat_id: COMBAT },
            Some(Version(1))
        ),
        Err(RejectReason::StaleVersion {
            current: Version(2)
        })
    );
}

#[test]
fn next_turn_needs_combatants() {
    let mut state = table_state();
    start(&mut state);
    assert_eq!(
        next(&mut state, Role::Gm),
        Err(RejectReason::invalid("combatants"))
    );
}

#[test]
fn removing_whoever_s_turn_it_is_hands_the_turn_on() {
    let mut state = fight();
    next(&mut state, Role::Gm).unwrap(); // NPC
    let events = run(
        &mut state,
        Role::Gm,
        Intent::RemoveCombatant {
            id: combatant_id(3),
        },
        Some(Version(2)),
    )
    .unwrap();
    assert_eq!(events.len(), 2);
    let combat = &state.combats[&COMBAT];
    assert_eq!((combat.turn, combat.round), (Some(combatant_id(0)), 1));
    // Removing someone else leaves the turn alone.
    run(
        &mut state,
        Role::Gm,
        Intent::RemoveCombatant {
            id: combatant_id(1),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.combats[&COMBAT].turn, Some(combatant_id(0)));
    // The last one leaves: nobody's turn.
    run(
        &mut state,
        Role::Gm,
        Intent::RemoveCombatant {
            id: combatant_id(0),
        },
        Some(Version(2)),
    )
    .unwrap();
    assert_eq!(state.combats[&COMBAT].turn, None);
}

#[test]
fn rolling_uses_the_sheet_field_and_posts_the_roll() {
    let mut state = fight();
    let hero = pooled_character_id(0);
    state.characters.get_mut(&hero).unwrap().sheet_json = r#"{"initiative": 3}"#.into();
    let events = run(
        &mut state,
        Role::Player,
        Intent::RollInitiative {
            id: combatant_id(0),
        },
        Some(Version(2)),
    )
    .unwrap();
    let [
        Event::CombatantUpdated { combatant },
        Event::ChatMessagePosted { message },
    ] = events.as_slice()
    else {
        panic!("unexpected events {events:?}");
    };
    let roll = message.roll.as_ref().unwrap();
    assert_eq!(combatant.initiative, Some(roll.total));
    assert!((4..=23).contains(&roll.total), "{roll:?}");
    assert_eq!(message.body, "1d20 + @initiative");
    assert_eq!(message.sender_name, "Token 0");
    assert_eq!(message.visibility, ChatVisibility::All);
}

#[test]
fn initiative_formula_falls_back_to_a_plain_d20() {
    assert_eq!(initiative_expression(None), "1d20");
    assert_eq!(initiative_expression(Some("{}")), "1d20");
    assert_eq!(
        initiative_expression(Some(r#"{"initiative": "high"}"#)),
        "1d20"
    );
    assert_eq!(
        initiative_expression(Some(r#"{"initiative": -1}"#)),
        "1d20 + @initiative"
    );
}

#[test]
fn hidden_tokens_roll_in_secret() {
    let mut state = table_state();
    start(&mut state);
    add(&mut state, 2); // token 2 is on the GM layer
    let events = run(
        &mut state,
        Role::Gm,
        Intent::RollInitiative {
            id: combatant_id(2),
        },
        Some(Version(1)),
    )
    .unwrap();
    let Some(Event::ChatMessagePosted { message }) = events.last() else {
        panic!("no roll posted");
    };
    assert_eq!(message.visibility, ChatVisibility::Gm);
}

#[test]
fn one_combat_per_map_and_tokens_from_that_map_once() {
    let mut state = fight();
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::StartCombat {
                id: CombatId(v7(0xC999)),
                map_id: pooled_map_id(0),
            },
            None
        ),
        Err(RejectReason::AlreadyExists)
    );
    let add_token = |n: u128, token: u128| Intent::AddCombatant {
        id: combatant_id(n),
        combat_id: COMBAT,
        token_id: Some(pooled_token_id(token)),
        character_id: None,
        name: "Again".into(),
    };
    assert_eq!(
        run(&mut state, Role::Gm, add_token(20, 0), None),
        Err(RejectReason::AlreadyExists)
    );
    // Token 4 lies on map 1.
    assert_eq!(
        run(&mut state, Role::Gm, add_token(21, 4), None),
        Err(RejectReason::invalid("token_id"))
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::StartCombat {
                id: CombatId(v7(0xC998)),
                map_id: MapId(v7(0x7777)),
            },
            None
        ),
        Err(RejectReason::NotFound)
    );
}

#[test]
fn combatants_without_a_token_need_a_name() {
    let mut state = table_state();
    start(&mut state);
    let add = |name: &str| Intent::AddCombatant {
        id: combatant_id(30),
        combat_id: COMBAT,
        token_id: None,
        character_id: None,
        name: name.into(),
    };
    assert_eq!(
        run(&mut state, Role::Gm, add("  "), None),
        Err(RejectReason::invalid("name"))
    );
    run(&mut state, Role::Gm, add("Reinforcements"), None).unwrap();
}

#[test]
fn combatant_limit() {
    let mut state = table_state();
    start(&mut state);
    for n in 0..MAX_COMBATANTS as u128 {
        run(
            &mut state,
            Role::Gm,
            Intent::AddCombatant {
                id: combatant_id(100 + n),
                combat_id: COMBAT,
                token_id: None,
                character_id: None,
                name: format!("Minion {n}"),
            },
            None,
        )
        .unwrap();
    }
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::AddCombatant {
                id: combatant_id(99),
                combat_id: COMBAT,
                token_id: None,
                character_id: None,
                name: "One too many".into(),
            },
            None
        ),
        Err(RejectReason::LimitExceeded {
            limit: "combatants".into()
        })
    );
}

#[test]
fn deleting_the_token_keeps_the_combatant_deleting_the_map_ends_the_combat() {
    let mut state = fight();
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteToken {
            id: pooled_token_id(3),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.combatants[&combatant_id(3)].token_id, None);
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteMap {
            id: pooled_map_id(0),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert!(state.combats.is_empty());
    assert!(state.combatants.is_empty());
}
