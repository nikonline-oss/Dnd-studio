//! Property tests over random intent sequences from all roles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use std::collections::BTreeMap;

use common::*;
use dnd_game_core::{GameState, SplitMix64, apply, evolve_all};
use dnd_protocol::arbitrary;
use dnd_protocol::*;
use proptest::prelude::*;
use proptest::sample::select;

/// One step: who acts, what they send, and whether to "fix" `expected_version` to the current
/// one (otherwise random versions would almost always be stale).
#[derive(Clone, Debug)]
struct Step {
    role: Role,
    envelope: IntentEnvelope,
    use_current_version: bool,
}

fn step() -> impl Strategy<Value = Step> {
    (
        select(Role::ALL.to_vec()),
        arbitrary::intent_envelope(),
        any::<bool>(),
    )
        .prop_map(|(role, envelope, use_current_version)| Step {
            role,
            envelope,
            use_current_version,
        })
}

fn target_version(state: &GameState, intent: &Intent) -> Option<Version> {
    match intent {
        Intent::UpdateCampaignMeta { .. } => Some(state.campaign.version),
        Intent::UpdateMap { id, .. } | Intent::DeleteMap { id } => {
            state.maps.get(id).map(|m| m.version)
        }
        Intent::UpdateToken { id, .. }
        | Intent::DeleteToken { id }
        | Intent::MoveToken { id, .. } => state.tokens.get(id).map(|t| t.version),
        Intent::CreateMap { .. } | Intent::CreateToken { .. } | Intent::CreateCharacter { .. } => {
            None
        }
        other => other
            .character_id()
            .and_then(|id| state.characters.get(&id).map(|c| c.version)),
    }
}

/// Runs the steps and returns every outcome, checking invariants after each step.
fn simulate(
    steps: &[Step],
    seed: u64,
) -> Result<Vec<Result<Vec<Event>, RejectReason>>, TestCaseError> {
    let mut state = base_state();
    let mut rng = SplitMix64::new(seed);
    let mut outcomes = Vec::with_capacity(steps.len());
    let mut last_versions: BTreeMap<String, Version> =
        BTreeMap::from([("campaign".to_owned(), state.campaign.version)]);

    for step in steps {
        let mut envelope = step.envelope.clone();
        let current = target_version(&state, &envelope.intent);
        if step.use_current_version {
            envelope.expected_version = current;
        }

        let before = state.clone();
        let outcome = apply(&state, &envelope, &actor(step.role), &mut rng);

        match &outcome {
            Ok(events) => {
                evolve_all(&mut state, events);
                prop_assert!(!events.is_empty(), "accepted intent produced no events");
            }
            Err(reason) => {
                prop_assert_eq!(&state, &before, "rejection must not change state");
                // (3) A wrong expected version on an authorized, existing target is always StaleVersion.
                let checked = envelope.intent.version_policy() == VersionPolicy::Checked;
                let authorized =
                    dnd_game_core::authorize(&actor(step.role), &envelope.intent, &before).is_ok();
                if let (true, true, Some(current), Some(expected)) =
                    (checked, authorized, current, envelope.expected_version)
                    && expected != current
                {
                    prop_assert_eq!(reason, &RejectReason::StaleVersion { current });
                }
            }
        }

        // (2) Versions are monotonic: every entity's version only grows, by at most 1 per step.
        let mut versions: BTreeMap<String, Version> = BTreeMap::new();
        versions.insert("campaign".into(), state.campaign.version);
        versions.extend(
            state
                .maps
                .values()
                .map(|m| (format!("map:{}", m.id), m.version)),
        );
        versions.extend(
            state
                .tokens
                .values()
                .map(|t| (format!("token:{}", t.id), t.version)),
        );
        for (key, version) in &versions {
            if let Some(prev) = last_versions.get(key) {
                prop_assert!(
                    *version == *prev || *version == prev.next(),
                    "{key}: {prev:?} -> {version:?}"
                );
            } else {
                prop_assert_eq!(
                    *version,
                    Version::INITIAL,
                    "{} created with non-initial version",
                    key
                );
            }
        }
        // Deleted entities may be re-created later with version 1.
        last_versions = versions;

        // (5) No token references a missing map; all numbers stay finite.
        for token in state.tokens.values() {
            prop_assert!(
                state.maps.contains_key(&token.map_id),
                "dangling token {:?}",
                token.id
            );
            // ADR-0016: positions stay on their map.
            let map = &state.maps[&token.map_id];
            prop_assert!(
                (0..=map.width * 16).contains(&token.x) && (0..=map.height * 16).contains(&token.y)
            );
            prop_assert!((0.0..360.0).contains(&token.rotation) && token.scale > 0.0);
        }

        outcomes.push(outcome);
    }
    Ok(outcomes)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// (1) never panics, (2) monotonic versions, (3) stale versions rejected, (5) no dangling tokens.
    #[test]
    fn invariants_hold(steps in proptest::collection::vec(step(), 1..60)) {
        simulate(&steps, 0)?;
    }

    /// (4) Same seed + same intents → same events.
    #[test]
    fn deterministic(steps in proptest::collection::vec(step(), 1..40), seed in any::<u64>()) {
        let a = simulate(&steps, seed)?;
        let b = simulate(&steps, seed)?;
        prop_assert_eq!(a, b);
    }

    /// Spectators never change anything.
    #[test]
    fn spectator_is_read_only(envelope in arbitrary::intent_envelope()) {
        let state = table_state();
        let result = apply(&state, &envelope, &actor(Role::Spectator), &mut SplitMix64::new(1));
        prop_assert_eq!(result, Err(RejectReason::Forbidden));
    }

    /// Players only move tokens they control and change characters they own.
    #[test]
    fn players_only_move_their_tokens(envelope in arbitrary::intent_envelope()) {
        let state = table_state();
        if let Ok(events) = apply(&state, &envelope, &actor(Role::Player), &mut SplitMix64::new(1)) {
            for event in events {
                match event {
                    Event::TokenMoved { id, .. } => {
                        let token = &state.tokens[&id];
                        let owner = token.character_id.and_then(|c| state.characters[&c].owner);
                        prop_assert_eq!(owner, Some(PLAYER));
                    }
                    Event::CharacterCreated { character } | Event::CharacterUpdated { character } => {
                        prop_assert_eq!(character.owner, Some(PLAYER));
                    }
                    other => prop_assert!(false, "player produced {:?}", other),
                }
            }
        }
    }
}
