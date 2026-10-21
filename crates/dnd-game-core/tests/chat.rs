//! Chat and dice rolls: permissions of all four roles, hidden GM rolls, sheet fields,
//! determinism by seed and system messages.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::{SplitMix64, apply};
use dnd_protocol::arbitrary::{pooled_character_id, v7};
use dnd_protocol::*;

fn msg_id(n: u128) -> ChatMessageId {
    ChatMessageId(v7(0x6000 + n))
}

fn say(text: &str) -> Intent {
    Intent::SendMessage {
        id: msg_id(1),
        body: text.into(),
    }
}

fn roll(expr: &str, hidden: bool, character: Option<u128>) -> Intent {
    Intent::RollDice {
        id: msg_id(2),
        expression: expr.into(),
        hidden,
        character_id: character.map(pooled_character_id),
    }
}

fn posted(events: &[Event]) -> &ChatMessageDto {
    match events {
        [Event::ChatMessagePosted { message }] => message,
        other => panic!("expected one chat message, got {other:?}"),
    }
}

#[test]
fn everyone_but_spectators_chats_and_rolls() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, true),
        (Role::Spectator, false),
    ] {
        for intent in [say("hello"), roll("1d20", false, None)] {
            let mut state = table_state();
            let result = run(&mut state, role, intent.clone(), None);
            assert_eq!(result.is_ok(), allowed, "{role:?} {intent:?}: {result:?}");
            if !allowed {
                assert_eq!(result, Err(RejectReason::Forbidden));
            }
        }
    }
}

#[test]
fn messages_carry_sender_and_trimmed_text() {
    let mut state = table_state();
    let events = run(&mut state, Role::Player, say("  hi all  "), None).unwrap();
    let m = posted(&events);
    assert_eq!((m.kind, m.body.as_str()), (ChatKind::Text, "hi all"));
    assert_eq!((m.sender, m.sender_name.as_str()), (Some(PLAYER), "Player"));
    assert_eq!(m.visibility, ChatVisibility::All);

    for bad in ["   ", &"x".repeat(MAX_CHAT_LEN + 1)] {
        assert_eq!(
            run(&mut state, Role::Gm, say(bad), None),
            Err(RejectReason::invalid("body"))
        );
    }
}

#[test]
fn hidden_rolls_are_for_gm_roles_only() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = table_state();
        let result = run(&mut state, role, roll("1d20", true, None), None);
        assert_eq!(result.is_ok(), allowed, "{role:?}");
        if let Ok(events) = result {
            assert_eq!(posted(&events).visibility, ChatVisibility::Gm);
        }
    }
}

#[test]
fn rolls_read_fields_from_the_sheet_of_an_owned_character() {
    let mut state = table_state();
    state
        .characters
        .get_mut(&pooled_character_id(0))
        .unwrap()
        .sheet_json = r#"{"str_mod": 3, "hp": {"max": 12}, "note": "x"}"#.into();

    let events = run(
        &mut state,
        Role::Player,
        roll("1d1 + @str_mod + @hp.max", false, Some(0)),
        None,
    )
    .unwrap();
    let m = posted(&events);
    let r = m.roll.as_ref().unwrap();
    assert_eq!(r.total, 1 + 3 + 12);
    assert_eq!(r.expression, "d1+@str_mod+@hp.max");
    assert_eq!(
        r.terms
            .iter()
            .map(|t| t.notation.as_str())
            .collect::<Vec<_>>(),
        ["d1", "@str_mod", "@hp.max"]
    );
    assert_eq!(m.body, "1d1 + @str_mod + @hp.max");

    // Someone else's sheet: forbidden for a player, fine for the GM.
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            roll("@str_mod", false, Some(1)),
            None
        ),
        Err(RejectReason::Forbidden)
    );
    assert!(run(&mut state, Role::Gm, roll("1d4", false, Some(1)), None).is_ok());

    // Unknown fields, non-integers, fields without a character, bad syntax.
    for (expr, character) in [
        ("@nope", Some(0)),
        ("@note", Some(0)),
        ("@str_mod", None),
        ("2d", None),
        ("1001d6", None),
    ] {
        assert_eq!(
            run(&mut state, Role::Gm, roll(expr, false, character), None),
            Err(RejectReason::invalid("expression")),
            "{expr}"
        );
    }
}

#[test]
fn rolls_are_deterministic_for_a_seed_and_keep_dice_details() {
    let state = table_state();
    let env = envelope(roll("4d6kh3 + 2", false, None), None);
    let a = apply(&state, &env, &actor(Role::Gm), &mut SplitMix64::new(7)).unwrap();
    let b = apply(&state, &env, &actor(Role::Gm), &mut SplitMix64::new(7)).unwrap();
    assert_eq!(a, b);
    let r = posted(&a).roll.clone().unwrap();
    let dice = &r.terms[0].dice;
    assert_eq!(dice.len(), 4);
    assert_eq!(dice.iter().filter(|d| d.kept).count(), 3);
    let kept: i64 = dice
        .iter()
        .filter(|d| d.kept)
        .map(|d| i64::from(d.value))
        .sum();
    assert_eq!(r.total, kept + 2);
}

#[test]
fn review_steps_post_system_messages() {
    let mut state = table_state();
    let id = CharacterId(v7(0x4000));
    run(
        &mut state,
        Role::Player,
        Intent::CreateCharacter {
            id,
            character_kind: CharacterKind::Pc,
            name: "Aria".into(),
            owner: Some(PLAYER),
            sheet_json: "{}".into(),
        },
        None,
    )
    .unwrap();
    let events = run(
        &mut state,
        Role::Player,
        Intent::SubmitCharacter { id },
        Some(Version(1)),
    )
    .unwrap();
    assert!(matches!(&events[..], [
        Event::CharacterUpdated { .. },
        Event::ChatMessagePosted { message }
    ] if message.kind == ChatKind::System
        && message.system == Some(SystemMessage::CharacterSubmitted { name: "Aria".into() })
        && is_uuid_v7(&message.id.0)));

    let events = run(
        &mut state,
        Role::Gm,
        Intent::ApproveCharacter { id },
        Some(Version(2)),
    )
    .unwrap();
    assert!(matches!(&events[1], Event::ChatMessagePosted { message }
        if message.system == Some(SystemMessage::CharacterApproved { name: "Aria".into() })));
}
