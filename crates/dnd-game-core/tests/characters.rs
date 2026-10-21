//! Characters (CLAUDE.md §5): permissions of all four roles, the status lifecycle and sheet
//! validation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_protocol::arbitrary::{pooled_character_id, v7};
use dnd_protocol::*;

fn new_id() -> CharacterId {
    CharacterId(v7(0x4000))
}

fn create(kind: CharacterKind, owner: Option<UserId>) -> Intent {
    Intent::CreateCharacter {
        id: new_id(),
        character_kind: kind,
        name: "Aria".into(),
        owner,
        sheet_json: r#"{"hp": 12, "skills": [1, 2]}"#.into(),
    }
}

fn status_of(state: &dnd_game_core::GameState, id: CharacterId) -> CharacterStatus {
    state.characters[&id].status
}

#[test]
fn who_may_create_what() {
    // (role, kind, owner, allowed, resulting status)
    let cases = [
        (
            Role::Gm,
            CharacterKind::Npc,
            None,
            true,
            Some(CharacterStatus::Active),
        ),
        (
            Role::Gm,
            CharacterKind::Pc,
            Some(PLAYER),
            true,
            Some(CharacterStatus::Active),
        ),
        (
            Role::CoGm,
            CharacterKind::Npc,
            None,
            true,
            Some(CharacterStatus::Active),
        ),
        (
            Role::Player,
            CharacterKind::Pc,
            Some(PLAYER),
            true,
            Some(CharacterStatus::Draft),
        ),
        (Role::Player, CharacterKind::Npc, Some(PLAYER), false, None),
        (
            Role::Player,
            CharacterKind::Pc,
            Some(OTHER_PLAYER),
            false,
            None,
        ),
        (Role::Player, CharacterKind::Pc, None, false, None),
        (
            Role::Spectator,
            CharacterKind::Pc,
            Some(SPECTATOR),
            false,
            None,
        ),
    ];
    for (role, kind, owner, allowed, status) in cases {
        let mut state = table_state();
        let result = run(&mut state, role, create(kind, owner), None);
        if allowed {
            assert!(result.is_ok(), "{role:?} {kind:?} {owner:?}: {result:?}");
            assert_eq!(Some(status_of(&state, new_id())), status, "{role:?}");
        } else {
            assert_eq!(
                result,
                Err(RejectReason::Forbidden),
                "{role:?} {kind:?} {owner:?}"
            );
        }
    }
}

#[test]
fn full_lifecycle_with_gm_review() {
    let mut state = table_state();
    run(
        &mut state,
        Role::Player,
        create(CharacterKind::Pc, Some(PLAYER)),
        None,
    )
    .unwrap();
    let id = new_id();

    // The player edits the draft and submits it.
    run(
        &mut state,
        Role::Player,
        Intent::UpdateCharacter {
            id,
            name: Some("Aria the Bold".into()),
            sheet_json: None,
        },
        Some(Version(1)),
    )
    .unwrap();
    run(
        &mut state,
        Role::Player,
        Intent::SubmitCharacter { id },
        Some(Version(2)),
    )
    .unwrap();
    assert_eq!(status_of(&state, id), CharacterStatus::PendingApproval);

    // No edits while under review; the player cannot approve.
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            Intent::UpdateCharacter {
                id,
                name: Some("Sneaky".into()),
                sheet_json: None
            },
            Some(Version(3))
        ),
        Err(RejectReason::Forbidden)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            Intent::ApproveCharacter { id },
            Some(Version(3))
        ),
        Err(RejectReason::Forbidden)
    );

    // The GM returns it, the player resubmits, the Co-GM approves.
    run(
        &mut state,
        Role::Gm,
        Intent::ReturnCharacter { id },
        Some(Version(3)),
    )
    .unwrap();
    assert_eq!(status_of(&state, id), CharacterStatus::Draft);
    run(
        &mut state,
        Role::Player,
        Intent::SubmitCharacter { id },
        Some(Version(4)),
    )
    .unwrap();
    run(
        &mut state,
        Role::CoGm,
        Intent::ApproveCharacter { id },
        Some(Version(5)),
    )
    .unwrap();
    assert_eq!(status_of(&state, id), CharacterStatus::Active);

    // Active characters stay editable by their owner; the GM archives.
    run(
        &mut state,
        Role::Player,
        Intent::UpdateCharacter {
            id,
            name: None,
            sheet_json: Some(r#"{"hp": 7}"#.into()),
        },
        Some(Version(6)),
    )
    .unwrap();
    run(
        &mut state,
        Role::Gm,
        Intent::ArchiveCharacter { id },
        Some(Version(7)),
    )
    .unwrap();
    assert_eq!(status_of(&state, id), CharacterStatus::Archived);
    assert_eq!(state.characters[&id].version, Version(8));
}

#[test]
fn invalid_transitions_are_rejected() {
    let mut state = table_state();
    let active = pooled_character_id(0);
    for intent in [
        Intent::SubmitCharacter { id: active },
        Intent::ApproveCharacter { id: active },
        Intent::ReturnCharacter { id: active },
    ] {
        assert_eq!(
            run(&mut state, Role::Gm, intent.clone(), Some(Version(1))),
            Err(RejectReason::invalid("status")),
            "{intent:?}"
        );
    }
    run(
        &mut state,
        Role::Gm,
        Intent::ArchiveCharacter { id: active },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::ArchiveCharacter { id: active },
            Some(Version(2))
        ),
        Err(RejectReason::invalid("status"))
    );
}

#[test]
fn only_gm_roles_review_archive_and_assign() {
    let id = pooled_character_id(0); // owned by PLAYER, active
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        for intent in [
            Intent::ArchiveCharacter { id },
            Intent::AssignCharacterOwner {
                id,
                owner: Some(OTHER_PLAYER),
            },
        ] {
            let mut state = table_state();
            let result = run(&mut state, role, intent.clone(), Some(Version(1)));
            assert_eq!(result.is_ok(), allowed, "{role:?} {intent:?}: {result:?}");
            if !allowed {
                assert_eq!(result, Err(RejectReason::Forbidden));
            }
        }
    }
}

#[test]
fn players_edit_only_their_own_characters() {
    let mut state = table_state();
    let foreign = pooled_character_id(1); // OTHER_PLAYER's
    let update = |id| Intent::UpdateCharacter {
        id,
        name: Some("X".into()),
        sheet_json: None,
    };
    assert_eq!(
        run(&mut state, Role::Player, update(foreign), Some(Version(1))),
        Err(RejectReason::Forbidden)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Spectator,
            update(pooled_character_id(0)),
            Some(Version(1))
        ),
        Err(RejectReason::Forbidden)
    );
    assert!(
        run(
            &mut state,
            Role::Player,
            update(pooled_character_id(0)),
            Some(Version(1))
        )
        .is_ok()
    );
}

#[test]
fn owner_must_be_a_known_user() {
    let mut state = table_state();
    let stranger = UserId(v7(0xDEAD));
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            create(CharacterKind::Pc, Some(stranger)),
            None
        ),
        Err(RejectReason::NotFound)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::AssignCharacterOwner {
                id: pooled_character_id(0),
                owner: Some(stranger)
            },
            Some(Version(1))
        ),
        Err(RejectReason::NotFound)
    );
    run(
        &mut state,
        Role::Gm,
        Intent::AssignCharacterOwner {
            id: pooled_character_id(0),
            owner: None,
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.characters[&pooled_character_id(0)].owner, None);
}

/// ADR-0016: sheets are JSON objects with integer numbers only, at most 256 KB.
#[test]
fn sheet_json_is_validated() {
    let bad = [
        "not json",
        "[1, 2]",
        r#"{"hp": 1.5}"#,
        r#"{"nested": {"weight": 0.25}}"#,
        r#"{"big": 9007199254740992}"#,
        r#"{"exp": 1e3}"#,
    ];
    for sheet in bad {
        let mut state = table_state();
        let intent = Intent::CreateCharacter {
            id: new_id(),
            character_kind: CharacterKind::Npc,
            name: "N".into(),
            owner: None,
            sheet_json: sheet.into(),
        };
        assert_eq!(
            run(&mut state, Role::Gm, intent, None),
            Err(RejectReason::invalid("sheet_json")),
            "{sheet}"
        );
    }
    let huge = format!(r#"{{"notes": "{}"}}"#, "x".repeat(MAX_SHEET_BYTES));
    assert_eq!(
        validate_sheet_json(&huge),
        Err(RejectReason::invalid("sheet_json"))
    );
    assert_eq!(
        validate_sheet_json(r#"{"hp": -9007199254740991, "ok": [true, null, "s"]}"#),
        Ok(())
    );
}
