#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use dnd_protocol::arbitrary::{pooled_character_id, v7};
use dnd_protocol::*;

fn ask(targets: Vec<CharacterId>) -> Intent {
    Intent::RequestRolls {
        id: RollRequestId(v7(0xF100)),
        label: " Perception ".into(),
        dice: " d1+4 ".into(),
        characters: targets,
    }
}

#[test]
fn gm_and_co_gm_request_checks_without_a_trigger_and_other_roles_cannot() {
    for role in [Role::Gm, Role::CoGm, Role::Player, Role::Spectator] {
        let mut state = base_state();
        let result = run(&mut state, role, ask(vec![pooled_character_id(0)]), None);
        assert_eq!(result.is_ok(), matches!(role, Role::Gm | Role::CoGm));
        if let Ok(events) = result {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::RollRequested { .. }))
            );
            let request = state.roll_requests.values().next().unwrap();
            assert_eq!(request.label, "Perception");
            assert_eq!(request.dice, "d1+4");
            assert!(request.trigger.is_none());
            assert_eq!(request.group, Some(RollRequestId(v7(0xF100))));
        }
    }
}

fn summaries(events: &[Event]) -> Vec<&ChatMessageDto> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ChatMessagePosted { message }
                if matches!(message.system, Some(SystemMessage::RollsCollected { .. })) =>
            {
                Some(message)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn controllers_roll_checks_without_resource_changes_and_gm_gets_one_final_summary() {
    let mut state = base_state();
    let sheets: Vec<_> = state
        .characters
        .values()
        .map(|c| c.sheet_json.clone())
        .collect();
    run(
        &mut state,
        Role::Gm,
        ask(vec![pooled_character_id(0), pooled_character_id(1)]),
        None,
    )
    .unwrap();
    let ids: Vec<_> = [0, 1]
        .into_iter()
        .map(|n| {
            state
                .roll_requests
                .values()
                .find(|r| r.character == pooled_character_id(n))
                .unwrap()
                .id
        })
        .collect();
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            Intent::FulfillRollRequest { id: ids[1] },
            Some(Version(1))
        ),
        Err(RejectReason::Forbidden)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Spectator,
            Intent::FulfillRollRequest { id: ids[0] },
            Some(Version(1))
        ),
        Err(RejectReason::Forbidden)
    );
    let first = run(
        &mut state,
        Role::Player,
        Intent::FulfillRollRequest { id: ids[0] },
        Some(Version(1)),
    )
    .unwrap();
    assert!(summaries(&first).is_empty());
    assert_eq!(state.roll_requests[&ids[0]].total, Some(5));
    assert_eq!(state.roll_requests[&ids[0]].delta, None);
    let last = run(
        &mut state,
        Role::Gm,
        Intent::FulfillRollRequest { id: ids[1] },
        Some(Version(1)),
    )
    .unwrap();
    let summary = summaries(&last);
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].visibility, ChatVisibility::Gm);
    if let Some(SystemMessage::RollsCollected { results, .. }) = &summary[0].system {
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.total == Some(5)));
    } else {
        panic!("missing results");
    }
    assert_eq!(
        sheets,
        state
            .characters
            .values()
            .map(|c| c.sheet_json.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        run(
            &mut state,
            Role::Player,
            Intent::FulfillRollRequest { id: ids[0] },
            Some(Version(1))
        )
        .is_err()
    );
    let cancel = run(
        &mut state,
        Role::CoGm,
        Intent::CancelRollRequest { id: ids[0] },
        Some(Version(2)),
    )
    .unwrap();
    assert!(
        summaries(&cancel).is_empty(),
        "completed summaries are not reposted"
    );
}

#[test]
fn cancellation_and_character_deletion_complete_groups_privately() {
    let mut state = base_state();
    run(
        &mut state,
        Role::Gm,
        ask(vec![pooled_character_id(0), pooled_character_id(1)]),
        None,
    )
    .unwrap();
    let first = state
        .roll_requests
        .values()
        .find(|r| r.character == pooled_character_id(0))
        .unwrap()
        .id;
    let cancelled = run(
        &mut state,
        Role::CoGm,
        Intent::CancelRollRequest { id: first },
        Some(Version(1)),
    )
    .unwrap();
    assert!(summaries(&cancelled).is_empty());
    assert!(
        cancelled
            .iter()
            .filter_map(|e| match e {
                Event::ChatMessagePosted { message } => Some(message),
                _ => None,
            })
            .all(|m| m.visibility == ChatVisibility::Gm)
    );
    let deleted = run(
        &mut state,
        Role::Gm,
        Intent::DeleteCharacter {
            id: pooled_character_id(1),
        },
        Some(Version(1)),
    )
    .unwrap();
    let summary = summaries(&deleted);
    assert_eq!(summary.len(), 1);
    if let Some(SystemMessage::RollsCollected { results, .. }) = &summary[0].system {
        assert!(results.iter().all(|r| r.total.is_none()));
    }
}

#[test]
fn creation_is_atomic_validated_and_cannot_reuse_an_entity_id() {
    let base = base_state();
    for targets in [
        vec![],
        vec![pooled_character_id(0); 2],
        vec![CharacterId(v7(0xDEAD))],
        vec![pooled_character_id(0); 21],
    ] {
        let mut state = base.clone();
        assert!(run(&mut state, Role::Gm, ask(targets), None).is_err());
        assert!(state.roll_requests.is_empty());
    }
    for dice in ["nonsense", "d0", "d20+@missing"] {
        let mut state = base.clone();
        let mut intent = ask(vec![pooled_character_id(0), pooled_character_id(1)]);
        if let Intent::RequestRolls { dice: d, .. } = &mut intent {
            *d = dice.into();
        }
        assert!(run(&mut state, Role::Gm, intent, None).is_err());
        assert!(state.roll_requests.is_empty());
    }
    let mut state = base;
    state
        .characters
        .get_mut(&pooled_character_id(1))
        .unwrap()
        .status = CharacterStatus::Archived;
    assert!(
        run(
            &mut state,
            Role::Gm,
            ask(vec![pooled_character_id(0), pooled_character_id(1)]),
            None
        )
        .is_err()
    );
    assert!(state.roll_requests.is_empty());
    let intent = ask(vec![pooled_character_id(0)]);
    run(&mut state, Role::Gm, intent.clone(), None).unwrap();
    assert_eq!(
        run(&mut state, Role::Gm, intent, None),
        Err(RejectReason::AlreadyExists)
    );
}

#[test]
fn checks_read_each_characters_fields_and_snapshots_remain_personal() {
    let mut state = base_state();
    state
        .characters
        .get_mut(&pooled_character_id(0))
        .unwrap()
        .sheet_json = r#"{"bonus":3}"#.into();
    state
        .characters
        .get_mut(&pooled_character_id(1))
        .unwrap()
        .sheet_json = r#"{"bonus":7}"#.into();
    let mut intent = ask(vec![pooled_character_id(0), pooled_character_id(1)]);
    if let Intent::RequestRolls { dice, .. } = &mut intent {
        *dice = "d1+@bonus".into();
    }
    run(&mut state, Role::Gm, intent, None).unwrap();
    let mine = dnd_game_core::snapshot_for_user(Role::Player, PLAYER, &state, Seq(1), [].iter());
    assert_eq!(mine.roll_requests.len(), 1);
    let id = mine.roll_requests[0].id;
    run(
        &mut state,
        Role::Player,
        Intent::FulfillRollRequest { id },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.roll_requests[&id].total, Some(4));
}
