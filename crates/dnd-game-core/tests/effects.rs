//! Temporary token effects (ADR-0024): application by triggers, removal rights, countdown by
//! turns and rounds, real-time expiry, recurring resource changes, visibility to players.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::path::Path;

use common::*;
use dnd_game_core::{
    ForPlayers, GameState, RuleError, SplitMix64, apply, evolve_all, filter_for_players,
    players_state,
};
use dnd_protocol::arbitrary::{pooled_character_id, pooled_map_id, pooled_token_id, v7};
use dnd_protocol::*;
use uuid::Uuid;

fn srd() -> PluginDto {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/system-srd5");
    let bytes = dnd_plugin::pack_directory(&path).unwrap();
    dnd_plugin::read_package(&bytes, dnd_plugin::PackageLimits::default())
        .unwrap()
        .to_dto()
}

/// The table with the SRD system and its default sheets (hp 10/10) on both heroes.
fn srd_state() -> GameState {
    let mut state = table_state();
    for (intent, version) in [
        (
            Intent::InstallPlugin {
                plugin: Box::new(srd()),
            },
            None,
        ),
        (
            Intent::SetSystemPlugin {
                id: Some("srd5".into()),
            },
            None,
        ),
    ] {
        run(&mut state, Role::Gm, intent, version).unwrap();
    }
    for n in [0, 1] {
        let id = pooled_character_id(n);
        let version = state.characters[&id].version;
        run(
            &mut state,
            Role::Gm,
            Intent::UpdateCharacter {
                id,
                name: None,
                sheet_json: Some("{}".into()),
            },
            Some(version),
        )
        .unwrap();
    }
    state
}

fn trigger(n: u128) -> TriggerId {
    TriggerId(v7(0x7500 + n))
}

fn create(state: &mut GameState, n: u128, action: TriggerAction) -> Result<Vec<Event>, RuleError> {
    run(
        state,
        Role::Gm,
        Intent::CreateTrigger {
            id: trigger(n),
            definition: TriggerDef {
                map_ids: Vec::new(),
                name: "Effect".into(),
                enabled: true,
                once: false,
                event: TriggerEvent::Manual,
                tokens: Vec::new(),
                condition: None,
                steps: vec![TriggerStep { when: None, action }],
            },
        },
        None,
    )
}

/// A label-only marker effect.
fn marker(duration: EffectDurationDef) -> TriggerAction {
    TriggerAction::ApplyEffect {
        label: "Blessed".into(),
        condition: None,
        resource: None,
        dice: None,
        heal: false,
        reveal: Vec::new(),
        duration,
    }
}

/// SRD poison: the poisoned condition plus 3 damage at the start of each of the token's turns.
fn poison(turns: u8) -> TriggerAction {
    TriggerAction::ApplyEffect {
        label: "Poison".into(),
        condition: Some("poisoned".into()),
        resource: Some("hp".into()),
        dice: Some("3".into()),
        heal: false,
        reveal: Vec::new(),
        duration: EffectDurationDef::Turns { turns },
    }
}

fn fire(state: &mut GameState, n: u128, token: u128) -> Vec<Event> {
    run(
        state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(n),
            token: Some(pooled_token_id(token)),
        },
        None,
    )
    .unwrap()
}

/// Fires with a caller-owned RNG, so consecutive applications draw distinct effect ids.
fn fire_rng(state: &mut GameState, rng: &mut SplitMix64, n: u128, token: u128) -> Vec<Event> {
    let events = apply(
        state,
        &envelope(
            Intent::FireTrigger {
                id: trigger(n),
                token: Some(pooled_token_id(token)),
            },
            None,
        ),
        &actor(Role::Gm),
        rng,
    )
    .unwrap();
    evolve_all(state, &events);
    events
}

fn applied(events: &[Event]) -> Vec<TokenEffectDto> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::TokenEffectApplied { effect } => Some(effect.clone()),
            _ => None,
        })
        .collect()
}

fn the_effect(state: &GameState) -> TokenEffectDto {
    assert_eq!(state.effects.len(), 1);
    state.effects.values().next().unwrap().clone()
}

const COMBAT: CombatId = CombatId(Uuid::from_u128(0x0190_a000_0000_7000_8000_0000_0000_0e01));

fn combatant_id(n: u128) -> CombatantId {
    CombatantId(v7(0xE000 + n))
}

/// A combat on map 0: token 0 (the player's hero) at 20, token 3 (NPC) at 5.
fn start_fight(state: &mut GameState) {
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
    for (n, initiative) in [(0, 20), (3, 5)] {
        let token = &state.tokens[&pooled_token_id(n)];
        run(
            state,
            Role::Gm,
            Intent::AddCombatant {
                id: combatant_id(n),
                combat_id: COMBAT,
                token_id: Some(token.id),
                character_id: token.character_id,
                name: token.name.clone(),
            },
            None,
        )
        .unwrap();
        let version = state.combatants[&combatant_id(n)].version;
        run(
            state,
            Role::Gm,
            Intent::SetInitiative {
                id: combatant_id(n),
                initiative: Some(initiative),
            },
            Some(version),
        )
        .unwrap();
    }
}

fn next(state: &mut GameState) -> Vec<Event> {
    let version = state.combats[&COMBAT].version;
    run(
        state,
        Role::Gm,
        Intent::NextTurn { combat_id: COMBAT },
        Some(version),
    )
    .unwrap()
}

fn hp(state: &GameState, character: u128) -> i64 {
    let sheet: serde_json::Value =
        serde_json::from_str(&state.characters[&pooled_character_id(character)].sheet_json)
            .unwrap();
    sheet["hp"]["current"].as_i64().unwrap()
}

#[test]
fn a_trigger_puts_an_effect_on_the_firing_token() {
    let mut state = srd_state();
    create(&mut state, 0, poison(3)).unwrap();
    let events = fire(&mut state, 0, 0);
    let applied_events = applied(&events);
    let [effect] = applied_events.as_slice() else {
        panic!("one effect applied: {events:?}");
    };
    assert_eq!(effect.token, pooled_token_id(0));
    assert_eq!(effect.label, "Poison");
    assert_eq!(effect.condition.as_deref(), Some("poisoned"));
    assert_eq!(effect.duration, EffectDuration::Turns { remaining: 3 });
    assert_eq!(effect.version, Version::INITIAL);
    assert_eq!(the_effect(&state).id, effect.id);
    // A token-less firing applies nothing.
    let events = run(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(0),
            token: None,
        },
        None,
    )
    .unwrap();
    assert!(applied(&events).is_empty());
}

#[test]
fn effect_definitions_are_checked() {
    let state = srd_state();
    let attempt = |action: TriggerAction| create(&mut state.clone(), 9, action);
    let with = |action: TriggerAction| {
        assert_eq!(
            attempt(action.clone()),
            Err(RejectReason::invalid("steps")),
            "{action:?}"
        );
    };
    let effect = |label: &str,
                  condition: Option<&str>,
                  resource: Option<&str>,
                  dice: Option<&str>,
                  duration: EffectDurationDef| {
        TriggerAction::ApplyEffect {
            label: label.into(),
            condition: condition.map(str::to_owned),
            resource: resource.map(str::to_owned),
            dice: dice.map(str::to_owned),
            heal: false,
            reveal: Vec::new(),
            duration,
        }
    };
    let turns = EffectDurationDef::Turns { turns: 3 };
    // An unknown condition of the system.
    with(effect("Poison", Some("on_fire"), None, None, turns));
    // A resource without dice (and vice versa), one the sheet has no path for, a bad roll.
    for (resource, dice) in [
        (Some("hp"), None),
        (None, Some("1d6")),
        (Some("nope"), Some("1d6")),
        (Some("hp"), Some("1d")),
    ] {
        with(effect("Poison", None, resource, dice, turns));
    }
    with(effect("   ", None, None, None, turns));
    for duration in [
        EffectDurationDef::Rounds { rounds: 0 },
        EffectDurationDef::Turns { turns: 0 },
        EffectDurationDef::Rounds {
            rounds: MAX_EFFECT_SPAN + 1,
        },
        EffectDurationDef::RealTime { minutes: 0 },
        EffectDurationDef::RealTime {
            minutes: MAX_EFFECT_MINUTES + 1,
        },
    ] {
        with(effect("Poison", None, None, None, duration));
    }
    // The bounds themselves are fine.
    for duration in [
        EffectDurationDef::Rounds {
            rounds: MAX_EFFECT_SPAN,
        },
        EffectDurationDef::RealTime {
            minutes: MAX_EFFECT_MINUTES,
        },
    ] {
        assert!(attempt(marker(duration)).is_ok(), "{duration:?}");
    }
}

#[test]
fn only_gm_roles_remove_effects() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::Rounds { rounds: 5 }),
    )
    .unwrap();
    fire(&mut state, 0, 0);
    let effect = the_effect(&state);
    for role in [Role::Player, Role::Spectator] {
        assert_eq!(
            run(
                &mut state,
                role,
                Intent::RemoveTokenEffect { id: effect.id },
                Some(effect.version)
            ),
            Err(RejectReason::Forbidden),
            "{role:?}"
        );
    }
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::RemoveTokenEffect { id: effect.id },
            Some(Version(99))
        ),
        Err(RejectReason::StaleVersion {
            current: effect.version
        })
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::RemoveTokenEffect {
                id: EffectId(v7(0xDEAD))
            },
            None
        ),
        Err(RejectReason::NotFound)
    );
    let events = run(
        &mut state,
        Role::CoGm,
        Intent::RemoveTokenEffect { id: effect.id },
        Some(effect.version),
    )
    .unwrap();
    assert_eq!(
        events,
        [Event::TokenEffectRemoved {
            id: effect.id,
            expired: false
        }]
    );
    assert!(state.effects.is_empty());
}

#[test]
fn turns_tick_down_and_hurt_at_the_turns_start() {
    let mut state = srd_state();
    create(&mut state, 0, poison(2)).unwrap();
    fire(&mut state, 0, 0);
    start_fight(&mut state);
    // The hero's first turn: 3 poison damage, then the countdown.
    let events = next(&mut state);
    assert_eq!(hp(&state, 0), 7);
    let delta = events.iter().find_map(|e| match e {
        Event::ChatMessagePosted { message } => match &message.system {
            Some(SystemMessage::ResourceChanged { delta, .. }) => Some(*delta),
            _ => None,
        },
        _ => None,
    });
    assert_eq!(delta, Some(-3));
    let updates: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::TokenEffectUpdated { .. }))
        .collect();
    let [Event::TokenEffectUpdated { effect }] = updates.as_slice() else {
        panic!("the effect counts down: {events:?}");
    };
    assert_eq!(effect.duration, EffectDuration::Turns { remaining: 1 });
    // The NPC's turn does not touch the hero's effect.
    let events = next(&mut state);
    assert!(!events.iter().any(|e| matches!(
        e,
        Event::TokenEffectUpdated { .. } | Event::TokenEffectRemoved { .. }
    )));
    // The hero's second turn: another 3 damage, and the effect expires.
    let events = next(&mut state);
    assert_eq!(hp(&state, 0), 4);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TokenEffectRemoved { expired: true, .. }))
    );
    assert!(state.effects.is_empty());
}

#[test]
fn rounds_tick_down_when_a_new_round_starts() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::Rounds { rounds: 2 }),
    )
    .unwrap();
    fire(&mut state, 0, 0);
    start_fight(&mut state);
    // Round 1: both turns leave the effect alone.
    for _ in 0..2 {
        let events = next(&mut state);
        assert!(!events.iter().any(|e| matches!(
            e,
            Event::TokenEffectUpdated { .. } | Event::TokenEffectRemoved { .. }
        )));
    }
    // The wrap into round 2 counts the effect down.
    let events = next(&mut state);
    assert_eq!(state.combats[&COMBAT].round, 2);
    let updates: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::TokenEffectUpdated { .. }))
        .collect();
    let [Event::TokenEffectUpdated { effect }] = updates.as_slice() else {
        panic!("the effect counts down: {events:?}");
    };
    assert_eq!(effect.duration, EffectDuration::Rounds { remaining: 1 });
    // The wrap into round 3 ends it.
    next(&mut state);
    let events = next(&mut state);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TokenEffectRemoved { expired: true, .. }))
    );
    assert!(state.effects.is_empty());
}

#[test]
fn real_time_effects_expire_by_the_envelope_clock() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::RealTime { minutes: 10 }),
    )
    .unwrap();
    run_at(
        &mut state,
        Role::Gm,
        Intent::FireTrigger {
            id: trigger(0),
            token: Some(pooled_token_id(0)),
        },
        None,
        1_000,
    )
    .unwrap();
    let EffectDuration::RealTime { until_ts } = the_effect(&state).duration else {
        panic!("a real-time effect");
    };
    assert_eq!(until_ts, 601_000);
    let nudge = |state: &mut GameState, ts_ms| {
        run_at(
            state,
            Role::Gm,
            Intent::MoveToken {
                id: pooled_token_id(1),
                x: 1,
                y: 1,
            },
            None,
            ts_ms,
        )
        .unwrap()
    };
    // A minute early: nothing expires.
    let events = nudge(&mut state, 600_999);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::TokenEffectRemoved { .. }))
    );
    assert_eq!(state.effects.len(), 1);
    // At the deadline the next intent of anyone notices the expiry first.
    let events = nudge(&mut state, 601_000);
    assert!(matches!(
        events.first(),
        Some(Event::TokenEffectRemoved { expired: true, .. })
    ));
    assert!(state.effects.is_empty());
}

#[test]
fn deleting_a_token_takes_its_effects_along() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::Rounds { rounds: 5 }),
    )
    .unwrap();
    fire(&mut state, 0, 0);
    let effect = the_effect(&state);
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteToken {
            id: pooled_token_id(0),
        },
        Some(Version::INITIAL),
    )
    .unwrap();
    let removed = events.iter().position(|e| {
        matches!(
            e,
            Event::TokenEffectRemoved { id, expired: false } if *id == effect.id
        )
    });
    let deleted = events
        .iter()
        .position(|e| matches!(e, Event::TokenDeleted { .. }));
    assert!(
        removed.is_some_and(|r| deleted.is_some_and(|d| r < d)),
        "{events:?}"
    );
    assert!(state.effects.is_empty());
}

#[test]
fn the_per_token_limit_caps_stacking() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::Rounds { rounds: 5 }),
    )
    .unwrap();
    let mut rng = SplitMix64::new(7);
    for _ in 0..MAX_EFFECTS_PER_TOKEN {
        assert_eq!(applied(&fire_rng(&mut state, &mut rng, 0, 0)).len(), 1);
    }
    assert_eq!(state.effects.len(), MAX_EFFECTS_PER_TOKEN);
    // One past the limit applies nothing (the trigger still fires).
    let events = fire_rng(&mut state, &mut rng, 0, 0);
    assert!(applied(&events).is_empty());
    assert_eq!(state.effects.len(), MAX_EFFECTS_PER_TOKEN);
}

#[test]
fn players_see_only_effects_of_tokens_they_see() {
    let mut state = table_state();
    create(
        &mut state,
        0,
        marker(EffectDurationDef::Rounds { rounds: 5 }),
    )
    .unwrap();
    let mut rng = SplitMix64::new(11);
    // Token 0 is visible; token 2 sits on the GM layer.
    let shown = fire_rng(&mut state, &mut rng, 0, 0);
    let hidden = fire_rng(&mut state, &mut rng, 0, 2);
    assert_eq!(state.effects.len(), 2);
    let view = players_state(&state);
    assert_eq!(view.effects.len(), 1);
    assert_eq!(
        view.effects.values().next().unwrap().token,
        pooled_token_id(0)
    );
    // The visible application reaches players as is; the hidden one is redacted.
    let before = table_state_with_trigger(&state);
    for (events, expect_shown) in [(shown, true), (hidden, false)] {
        let batch = EventBatch {
            seq: Seq(1),
            caused_by: None,
            events,
        };
        let ForPlayers::Batch(filtered) = filter_for_players(&before, &state, &batch) else {
            panic!("a filtered batch");
        };
        assert_eq!(
            filtered
                .events
                .iter()
                .any(|e| matches!(e, Event::TokenEffectApplied { .. })),
            expect_shown
        );
    }
}

/// The state before the two effects were applied (both firings replayed against it in the test
/// above): the table with the trigger created.
fn table_state_with_trigger(state: &GameState) -> GameState {
    let mut before = table_state();
    let trigger = state.triggers[&trigger(0)].clone();
    before.triggers.insert(trigger.id, trigger);
    before
}

#[test]
fn fictional_time_expires_only_at_explicit_deadline_and_is_versioned_and_authorized() {
    let mut s = srd_state();
    create(
        &mut s,
        0,
        marker(EffectDurationDef::GameTime { seconds: 60 }),
    )
    .unwrap();
    fire(&mut s, 0, 0);
    let effect = s.effects.values().next().unwrap().clone();
    assert_eq!(effect.duration, EffectDuration::GameTime { until: 60 });
    for role in [Role::Player, Role::Spectator] {
        assert_eq!(
            run(
                &mut s,
                role,
                Intent::AdvanceGameTime { seconds: 60 },
                Some(Version::INITIAL)
            ),
            Err(RejectReason::Forbidden)
        );
    }
    run(
        &mut s,
        Role::CoGm,
        Intent::AdvanceGameTime { seconds: 59 },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert!(s.effects.contains_key(&effect.id));
    assert_eq!(s.campaign.game_clock.seconds, 59);
    assert!(
        run(
            &mut s,
            Role::Gm,
            Intent::AdvanceGameTime { seconds: 1 },
            Some(Version::INITIAL)
        )
        .is_err()
    );
    let events = run(
        &mut s,
        Role::Gm,
        Intent::AdvanceGameTime { seconds: 1 },
        Some(Version(2)),
    )
    .unwrap();
    assert!(s.effects.is_empty());
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TokenEffectRemoved { expired: true, .. }))
    );
}
#[test]
fn fictional_time_rejects_invalid_steps_and_does_not_consume_round_effects() {
    let mut s = srd_state();
    create(&mut s, 0, marker(EffectDurationDef::Rounds { rounds: 3 })).unwrap();
    fire(&mut s, 0, 0);
    for seconds in [0, -1, 604_801, i64::MAX] {
        assert!(
            run(
                &mut s,
                Role::Gm,
                Intent::AdvanceGameTime { seconds },
                Some(Version::INITIAL)
            )
            .is_err()
        );
    }
    run(
        &mut s,
        Role::Gm,
        Intent::AdvanceGameTime { seconds: 604_800 },
        Some(Version::INITIAL),
    )
    .unwrap();
    assert_eq!(
        s.effects.values().next().unwrap().duration,
        EffectDuration::Rounds { remaining: 3 }
    );
}

#[test]
fn wall_timestamp_and_metadata_edit_do_not_consume_fictional_duration() {
    let mut s = srd_state();
    create(
        &mut s,
        0,
        marker(EffectDurationDef::GameTime { seconds: 60 }),
    )
    .unwrap();
    fire(&mut s, 0, 0);
    run(
        &mut s,
        Role::Gm,
        Intent::AdvanceGameTime { seconds: 10 },
        Some(Version::INITIAL),
    )
    .unwrap();
    let mut env = envelope(
        Intent::UpdateCampaignMeta {
            name: Some("Renamed".into()),
            description: None,
            token_inspector: None,
            item_states: None,
            visibility: None,
        },
        Some(s.campaign.version),
    );
    env.ts_ms = 9_000_000_000_000;
    let events = apply(&s, &env, &actor(Role::Gm), &mut SplitMix64::new(42)).unwrap();
    evolve_all(&mut s, &events);
    assert_eq!(s.campaign.game_clock.seconds, 10);
    assert_eq!(s.effects.len(), 1);
    assert_eq!(s.campaign.name, "Renamed");
    assert_eq!(players_state(&s).campaign.game_clock.seconds, 10);
}

// --- A8: scheduled real-time expiry (ADR-0056) ---

/// A state with one real-time effect due at `until_ts`.
fn state_with_deadline(until_ts: i64) -> GameState {
    let mut state = table_state();
    let id = EffectId::new_v7();
    state.effects.insert(
        id,
        TokenEffectDto {
            id,
            token: pooled_token_id(0),
            label: "Bless".into(),
            condition: None,
            resource: None,
            dice: None,
            heal: false,
            reveal: Vec::new(),
            duration: EffectDuration::RealTime { until_ts },
            version: Version::INITIAL,
        },
    );
    state
}

#[test]
fn the_next_deadline_is_the_earliest_real_moment() {
    assert_eq!(dnd_game_core::next_real_deadline(&table_state()), None);
    let mut state = state_with_deadline(5_000);
    assert_eq!(dnd_game_core::next_real_deadline(&state), Some(5_000));
    let second = EffectId::new_v7();
    state.effects.insert(
        second,
        TokenEffectDto {
            id: second,
            token: pooled_token_id(1),
            label: "Shield".into(),
            condition: None,
            resource: None,
            dice: None,
            heal: false,
            reveal: Vec::new(),
            duration: EffectDuration::RealTime { until_ts: 3_000 },
            version: Version::INITIAL,
        },
    );
    assert_eq!(dnd_game_core::next_real_deadline(&state), Some(3_000));
}

#[test]
fn the_deadline_boundary_is_exact() {
    let state = state_with_deadline(5_000);
    assert!(dnd_game_core::expire_due_effects(&state, 4_999).is_empty());
    let due = dnd_game_core::expire_due_effects(&state, 5_000);
    assert_eq!(due.len(), 1);
    assert!(matches!(
        due[0],
        Event::TokenEffectRemoved { expired: true, .. }
    ));
}

#[test]
fn a_repeated_signal_removes_nothing_and_a_backwards_clock_keeps_the_effect() {
    let mut state = state_with_deadline(5_000);
    let due = dnd_game_core::expire_due_effects(&state, 5_000);
    evolve_all(&mut state, &due);
    assert!(state.effects.is_empty());
    // The timer fires again (a second adapter, a retry): nothing left to remove.
    assert!(dnd_game_core::expire_due_effects(&state, 6_000).is_empty());
    // A clock that moved backwards does not expire a future effect.
    let state = state_with_deadline(9_000);
    assert!(dnd_game_core::expire_due_effects(&state, 1_000).is_empty());
}
