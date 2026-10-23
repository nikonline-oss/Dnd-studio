//! Journal folders (ADR-0021): rights of all four roles, names, cycles, depth, recursive delete.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_game_core::GameState;
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;

fn folder(n: u128) -> JournalFolderId {
    JournalFolderId(v7(0x6000 + n))
}

fn entry(n: u128) -> JournalEntryId {
    JournalEntryId(v7(0x5000 + n))
}

fn mkdir(state: &mut GameState, n: u128, parent: Option<u128>, name: &str) {
    run(
        state,
        Role::Gm,
        Intent::CreateJournalFolder {
            id: folder(n),
            parent: parent.map(folder),
            name: name.into(),
        },
        None,
    )
    .unwrap();
}

fn put(state: &mut GameState, n: u128, folder_n: Option<u128>) {
    run(
        state,
        Role::Gm,
        Intent::CreateJournalEntry {
            id: entry(n),
            folder_id: folder_n.map(folder),
            title: format!("Entry {n}"),
            markdown: String::new(),
            visibility: JournalVisibility::Players,
            players_can_edit: false,
        },
        None,
    )
    .unwrap();
}

/// Lore (0) / Places (1) / Towns (2); entries: 0 in Lore, 1 in Towns, 2 at the root.
fn tree() -> GameState {
    let mut state = table_state();
    mkdir(&mut state, 0, None, "Lore");
    mkdir(&mut state, 1, Some(0), "Places");
    mkdir(&mut state, 2, Some(1), "Towns");
    put(&mut state, 0, Some(0));
    put(&mut state, 1, Some(2));
    put(&mut state, 2, None);
    state
}

fn version(state: &GameState, n: u128) -> Option<Version> {
    Some(state.journal_folders[&folder(n)].version)
}

#[test]
fn only_gm_roles_manage_folders() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = tree();
        let v = version(&state, 2);
        let intents = [
            (
                Intent::CreateJournalFolder {
                    id: folder(9),
                    parent: None,
                    name: "New".into(),
                },
                None,
            ),
            (
                Intent::RenameJournalFolder {
                    id: folder(2),
                    name: "Cities".into(),
                },
                v,
            ),
        ];
        for (intent, expected) in intents {
            let result = run(&mut state, role, intent, expected);
            if allowed {
                assert!(result.is_ok(), "{role:?}: {result:?}");
            } else {
                assert_eq!(result, Err(RejectReason::Forbidden), "{role:?}");
            }
        }
        let v = version(&state, 2);
        let moved = run(
            &mut state,
            role,
            Intent::MoveJournalFolder {
                id: folder(2),
                parent: None,
                sort_order: 0,
            },
            v,
        );
        assert_eq!(moved.is_ok(), allowed, "{role:?} move: {moved:?}");
        let v = version(&state, 0);
        let deleted = run(
            &mut state,
            role,
            Intent::DeleteJournalFolder { id: folder(0) },
            v,
        );
        assert_eq!(deleted.is_ok(), allowed, "{role:?} delete: {deleted:?}");
    }
}

#[test]
fn sibling_folders_have_different_names() {
    let mut state = tree();
    let create = |n, parent: Option<u128>, name: &str| Intent::CreateJournalFolder {
        id: folder(n),
        parent: parent.map(folder),
        name: name.into(),
    };
    // Case-insensitive, after trimming.
    assert_eq!(
        run(&mut state, Role::Gm, create(9, None, "  lore "), None),
        Err(RejectReason::AlreadyExists)
    );
    // The same name in another folder is fine.
    run(&mut state, Role::Gm, create(9, Some(0), "Towns"), None).unwrap();
    // Moving next to a namesake is refused too.
    let v = version(&state, 9);
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::MoveJournalFolder {
                id: folder(9),
                parent: Some(folder(1)),
                sort_order: 0,
            },
            v
        ),
        Err(RejectReason::AlreadyExists)
    );
    // Renaming to its own name (other case) is allowed.
    let v = version(&state, 0);
    run(
        &mut state,
        Role::Gm,
        Intent::RenameJournalFolder {
            id: folder(0),
            name: "LORE".into(),
        },
        v,
    )
    .unwrap();
    assert_eq!(state.journal_folders[&folder(0)].name, "LORE");
}

#[test]
fn a_folder_never_moves_into_itself_or_its_subfolders() {
    let mut state = tree();
    for target in [0, 1, 2] {
        let v = version(&state, 0);
        assert_eq!(
            run(
                &mut state,
                Role::Gm,
                Intent::MoveJournalFolder {
                    id: folder(0),
                    parent: Some(folder(target)),
                    sort_order: 0,
                },
                v
            ),
            Err(RejectReason::invalid("parent")),
            "into {target}"
        );
    }
    // Towns to the root keeps its own content.
    let v = version(&state, 2);
    run(
        &mut state,
        Role::Gm,
        Intent::MoveJournalFolder {
            id: folder(2),
            parent: None,
            sort_order: 3,
        },
        v,
    )
    .unwrap();
    let towns = &state.journal_folders[&folder(2)];
    assert_eq!((towns.parent, towns.sort_order), (None, 3));
    assert_eq!(state.journal[&entry(1)].folder_id, Some(folder(2)));
}

#[test]
fn nesting_is_limited() {
    let mut state = table_state();
    for n in 0..MAX_FOLDER_DEPTH as u128 {
        mkdir(&mut state, n, n.checked_sub(1), &format!("Level {n}"));
    }
    let deepest = MAX_FOLDER_DEPTH as u128 - 1;
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateJournalFolder {
                id: folder(100),
                parent: Some(folder(deepest)),
                name: "Too deep".into(),
            },
            None
        ),
        Err(RejectReason::invalid("parent"))
    );
    // A two-level subtree cannot go where it would end up too deep either.
    mkdir(&mut state, 101, None, "Pair");
    mkdir(&mut state, 102, Some(101), "Child");
    let v = version(&state, 101);
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::MoveJournalFolder {
                id: folder(101),
                parent: Some(folder(deepest - 1)),
                sort_order: 0,
            },
            v
        ),
        Err(RejectReason::invalid("parent"))
    );
}

#[test]
fn deleting_a_folder_deletes_everything_in_it_entries_first() {
    let mut state = tree();
    let v = version(&state, 0);
    let events = run(
        &mut state,
        Role::Gm,
        Intent::DeleteJournalFolder { id: folder(0) },
        v,
    )
    .unwrap();
    // Entries, then folders deepest first: no event leaves something without its parent.
    let mut entries: Vec<_> = events
        .iter()
        .take(2)
        .map(|e| match e {
            Event::JournalEntryDeleted { id } => *id,
            other => panic!("expected an entry first, got {other:?}"),
        })
        .collect();
    entries.sort();
    assert_eq!(entries, vec![entry(0), entry(1)]);
    assert_eq!(
        events[2..],
        [
            Event::JournalFolderDeleted { id: folder(2) },
            Event::JournalFolderDeleted { id: folder(1) },
            Event::JournalFolderDeleted { id: folder(0) },
        ]
    );
    assert!(state.journal_folders.is_empty());
    // Only the root entry is left.
    assert_eq!(
        state.journal.keys().copied().collect::<Vec<_>>(),
        vec![entry(2)]
    );
}

#[test]
fn unknown_parents_and_folders() {
    let mut state = tree();
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateJournalFolder {
                id: folder(9),
                parent: Some(folder(77)),
                name: "Orphan".into(),
            },
            None
        ),
        Err(RejectReason::invalid("parent"))
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::DeleteJournalFolder { id: folder(77) },
            Some(Version(1))
        ),
        Err(RejectReason::NotFound)
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::MoveJournalEntry {
                id: entry(2),
                folder_id: Some(folder(77)),
                sort_order: 0,
            },
            Some(Version(1))
        ),
        Err(RejectReason::invalid("folder_id"))
    );
    // A blank name.
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateJournalFolder {
                id: folder(9),
                parent: None,
                name: "   ".into(),
            },
            None
        ),
        Err(RejectReason::invalid("name"))
    );
}

#[test]
fn a_stale_folder_rename_is_rejected() {
    let mut state = tree();
    run(
        &mut state,
        Role::Gm,
        Intent::RenameJournalFolder {
            id: folder(1),
            name: "Regions".into(),
        },
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        run(
            &mut state,
            Role::CoGm,
            Intent::RenameJournalFolder {
                id: folder(1),
                name: "Areas".into(),
            },
            Some(Version(1))
        ),
        Err(RejectReason::StaleVersion {
            current: Version(2)
        })
    );
}
