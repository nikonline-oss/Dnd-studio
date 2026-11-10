//! The execution trace of `apply` (ADR-0052): which stages fired, how many rounds ran, and that a
//! pathological cascade stops at the limit instead of spinning.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{GameState, MAX_RESOLUTION_ROUNDS, SplitMix64, apply_traced, evolve_all};
use dnd_protocol::*;

fn create_map() -> Intent {
    Intent::CreateMap {
        id: MapId::new_v7(),
        name: "Cave".into(),
        description: String::new(),
        background_asset_id: None,
        width: 4000,
        height: 3000,
        grid: Grid::default(),
        visible_to_players: true,
    }
}

fn roll() -> Intent {
    Intent::RollDice {
        id: ChatMessageId::new_v7(),
        expression: "2d6 + 1".into(),
        hidden: false,
        character_id: None,
        roll: None,
    }
}

#[test]
fn a_settled_intent_reports_its_stages_and_stops() {
    let state = base_state();
    let mut rng = SplitMix64::new(7);
    let (events, trace) = apply_traced(
        &state,
        &envelope(create_map(), None),
        &actor(Role::Gm),
        &mut rng,
    )
    .unwrap();
    assert_eq!(trace.stage("intent"), events.len());
    assert_eq!(trace.rounds, 1);
    assert!(!trace.limit_reached);
    assert!(trace.stage("rules_view") <= 1);
}

#[test]
fn the_same_intent_and_seed_give_the_same_events_and_trace() {
    let state = base_state();
    let mut first_rng = SplitMix64::new(11);
    let mut second_rng = SplitMix64::new(11);
    let (first, first_trace) = apply_traced(
        &state,
        &envelope(roll(), None),
        &actor(Role::Gm),
        &mut first_rng,
    )
    .unwrap();
    let (second, second_trace) = apply_traced(
        &state,
        &envelope(roll(), None),
        &actor(Role::Gm),
        &mut second_rng,
    )
    .unwrap();
    // The message id is a time-ordered UUID, not an RNG draw; the roll itself must match.
    let total = |events: &[Event]| match events {
        [Event::ChatMessagePosted { message }] => {
            message.roll.as_ref().expect("a roll result").total
        }
        other => panic!("expected one roll event: {other:?}"),
    };
    assert_eq!(total(&first), total(&second));
    assert_eq!(first_trace, second_trace);
}

#[test]
fn the_trace_never_exceeds_the_round_limit() {
    let mut state = base_state();
    let mut rng = SplitMix64::new(3);
    for _ in 0..20 {
        let (events, trace) =
            apply_traced(&state, &envelope(roll(), None), &actor(Role::Gm), &mut rng).unwrap();
        assert!(trace.rounds <= MAX_RESOLUTION_ROUNDS);
        assert!(!trace.limit_reached, "{trace:?}");
        evolve_all(&mut state, &events);
    }
}

#[test]
fn a_rejected_intent_has_no_trace_to_report() {
    let state = GameState::new("Empty");
    let mut rng = SplitMix64::new(1);
    let result = apply_traced(
        &state,
        &envelope(create_map(), None),
        &actor(Role::Player),
        &mut rng,
    );
    assert!(result.is_err());
}
