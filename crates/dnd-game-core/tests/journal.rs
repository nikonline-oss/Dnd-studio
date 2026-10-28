//! Journal: permissions of all four roles, editing by players, folders, ordering, limits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

mod common;

use common::*;
use dnd_protocol::arbitrary::v7;
use dnd_protocol::*;

fn entry_id(n: u128) -> JournalEntryId {
    JournalEntryId(v7(0x5000 + n))
}

fn folder_id(n: u128) -> JournalFolderId {
    JournalFolderId(v7(0x6000 + n))
}

fn create(
    n: u128,
    folder: Option<JournalFolderId>,
    visibility: JournalVisibility,
    players_can_edit: bool,
) -> Intent {
    Intent::CreateJournalEntry {
        id: entry_id(n),
        folder_id: folder,
        title: format!("Entry {n}"),
        markdown: "# Hello".into(),
        visibility,
        players_can_edit,
        table: None,
    }
}

fn edit_text(n: u128, text: &str) -> Intent {
    Intent::UpdateJournalEntry {
        id: entry_id(n),
        title: None,
        markdown: Some(text.into()),
        visibility: None,
        players_can_edit: None,
        table: None,
    }
}

/// Folders 0 "Secrets" and 1 "Lore". Entry 0: GM-only in Secrets. Entry 1: shared, read-only,
/// in Lore. Entry 2: shared, players may edit, in Lore.
fn journal_state() -> dnd_game_core::GameState {
    let mut state = table_state();
    for (n, name) in [(0, "Secrets"), (1, "Lore")] {
        run(
            &mut state,
            Role::Gm,
            Intent::CreateJournalFolder {
                id: folder_id(n),
                parent: None,
                name: name.into(),
            },
            None,
        )
        .unwrap();
    }
    let (secrets, lore) = (Some(folder_id(0)), Some(folder_id(1)));
    for intent in [
        create(0, secrets, JournalVisibility::Gm, false),
        create(1, lore, JournalVisibility::Players, false),
        create(2, lore, JournalVisibility::Players, true),
    ] {
        run(&mut state, Role::Gm, intent, None).unwrap();
    }
    state
}

#[test]
fn only_gm_roles_create_move_and_delete() {
    for (role, allowed) in [
        (Role::Gm, true),
        (Role::CoGm, true),
        (Role::Player, false),
        (Role::Spectator, false),
    ] {
        let mut state = journal_state();
        let created = run(
            &mut state,
            role,
            create(9, None, JournalVisibility::Gm, false),
            None,
        );
        assert_eq!(created.is_ok(), allowed, "{role:?} create: {created:?}");

        let mut state = journal_state();
        let moved = run(
            &mut state,
            role,
            Intent::MoveJournalEntry {
                id: entry_id(2),
                folder_id: Some(folder_id(0)),
                sort_order: 5,
            },
            Some(Version(1)),
        );
        assert_eq!(moved.is_ok(), allowed, "{role:?} move: {moved:?}");

        let mut state = journal_state();
        let deleted = run(
            &mut state,
            role,
            Intent::DeleteJournalEntry { id: entry_id(2) },
            Some(Version(1)),
        );
        assert_eq!(deleted.is_ok(), allowed, "{role:?} delete: {deleted:?}");
        if !allowed {
            assert_eq!(deleted, Err(RejectReason::Forbidden));
        }
    }
}

#[test]
fn players_edit_only_shared_editable_entries_and_only_text() {
    let mut state = journal_state();
    // GM-only, shared read-only: forbidden (and players cannot tell which one exists).
    for n in [0, 1] {
        assert_eq!(
            run(
                &mut state,
                Role::Player,
                edit_text(n, "x"),
                Some(Version(1))
            ),
            Err(RejectReason::Forbidden),
            "entry {n}"
        );
    }
    // Shared editable: text and title are fine…
    run(
        &mut state,
        Role::Player,
        edit_text(2, "Players' notes"),
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(state.journal[&entry_id(2)].markdown, "Players' notes");
    // …but not visibility or the edit flag.
    for intent in [
        Intent::UpdateJournalEntry {
            id: entry_id(2),
            title: None,
            markdown: None,
            visibility: Some(JournalVisibility::Gm),
            players_can_edit: None,
            table: None,
        },
        Intent::UpdateJournalEntry {
            id: entry_id(2),
            title: None,
            markdown: None,
            visibility: None,
            players_can_edit: Some(false),
            table: None,
        },
    ] {
        assert_eq!(
            run(&mut state, Role::Player, intent, Some(Version(2))),
            Err(RejectReason::Forbidden)
        );
    }
    // Spectators never edit.
    assert_eq!(
        run(
            &mut state,
            Role::Spectator,
            edit_text(2, "y"),
            Some(Version(2))
        ),
        Err(RejectReason::Forbidden)
    );
    // Once the GM hides it, the player loses access.
    run(
        &mut state,
        Role::Gm,
        Intent::UpdateJournalEntry {
            id: entry_id(2),
            title: None,
            markdown: None,
            visibility: Some(JournalVisibility::Gm),
            players_can_edit: None,
            table: None,
        },
        Some(Version(2)),
    )
    .unwrap();
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            edit_text(2, "z"),
            Some(Version(3))
        ),
        Err(RejectReason::Forbidden)
    );
}

#[test]
fn concurrent_edits_conflict_by_version() {
    let mut state = journal_state();
    run(&mut state, Role::Gm, edit_text(2, "GM"), Some(Version(1))).unwrap();
    assert_eq!(
        run(
            &mut state,
            Role::Player,
            edit_text(2, "Player"),
            Some(Version(1))
        ),
        Err(RejectReason::StaleVersion {
            current: Version(2)
        })
    );
    assert_eq!(state.journal[&entry_id(2)].markdown, "GM");
}

#[test]
fn new_entries_go_last_in_their_folder() {
    let mut state = journal_state();
    run(
        &mut state,
        Role::Gm,
        create(3, Some(folder_id(1)), JournalVisibility::Gm, false),
        None,
    )
    .unwrap();
    let e = &state.journal[&entry_id(3)];
    assert_eq!((e.folder_id, e.sort_order), (Some(folder_id(1)), 2));
    assert_eq!(e.author, Some(GM));

    run(
        &mut state,
        Role::Gm,
        create(4, None, JournalVisibility::Gm, false),
        None,
    )
    .unwrap();
    let e = &state.journal[&entry_id(4)];
    assert_eq!((e.folder_id, e.sort_order), (None, 0));
}

#[test]
fn invalid_values_are_rejected() {
    let cases = [
        // An unknown folder.
        (
            create(9, Some(folder_id(99)), JournalVisibility::Gm, false),
            "folder_id",
        ),
        (
            Intent::CreateJournalEntry {
                id: entry_id(9),
                folder_id: None,
                title: "  ".into(),
                markdown: String::new(),
                visibility: JournalVisibility::Gm,
                players_can_edit: false,
                table: None,
            },
            "title",
        ),
        (
            Intent::CreateJournalEntry {
                id: entry_id(9),
                folder_id: None,
                title: "Big".into(),
                markdown: "x".repeat(MAX_JOURNAL_BYTES + 1),
                visibility: JournalVisibility::Gm,
                players_can_edit: false,
                table: None,
            },
            "markdown",
        ),
    ];
    for (intent, field) in cases {
        let mut state = journal_state();
        assert_eq!(
            run(&mut state, Role::Gm, intent, None),
            Err(RejectReason::invalid(field))
        );
    }
    let mut state = journal_state();
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::UpdateJournalEntry {
                id: entry_id(2),
                title: None,
                markdown: None,
                visibility: None,
                players_can_edit: None,
                table: None,
            },
            Some(Version(1))
        ),
        Err(RejectReason::invalid("intent"))
    );
}

#[test]
fn delete_removes_the_entry_and_its_id_cannot_collide() {
    let mut state = journal_state();
    run(
        &mut state,
        Role::Gm,
        Intent::DeleteJournalEntry { id: entry_id(0) },
        Some(Version(1)),
    )
    .unwrap();
    assert!(!state.journal.contains_key(&entry_id(0)));
    // A map may not take an existing journal entry's id (ADR-0015).
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            Intent::CreateMap {
                id: MapId(entry_id(1).0),
                name: "M".into(),
                background_asset_id: None,
                width: 10,
                height: 10,
                grid: Grid::default(),
                visible_to_players: false,
            },
            None
        ),
        Err(RejectReason::AlreadyExists)
    );
}

fn table(rows: &[&[&str]]) -> JournalTable {
    let width = rows.first().map_or(1, |r| r.len());
    JournalTable {
        columns: (0..width)
            .map(|i| TableColumn {
                name: format!("Col {i}"),
                width: 120,
            })
            .collect(),
        rows: rows
            .iter()
            .map(|r| r.iter().map(|c| (*c).to_owned()).collect())
            .collect(),
    }
}

fn create_table(n: u128, players_can_edit: bool, cells: JournalTable) -> Intent {
    Intent::CreateJournalEntry {
        id: entry_id(n),
        folder_id: None,
        title: format!("Table {n}"),
        markdown: String::new(),
        visibility: JournalVisibility::Players,
        players_can_edit,
        table: Some(cells),
    }
}

fn edit_table(n: u128, cells: JournalTable) -> Intent {
    Intent::UpdateJournalEntry {
        id: entry_id(n),
        title: None,
        markdown: None,
        visibility: None,
        players_can_edit: None,
        table: Some(cells),
    }
}

#[test]
fn table_entries_hold_cells_and_stay_tables() {
    let mut state = journal_state();
    run(
        &mut state,
        Role::Gm,
        create_table(7, true, table(&[&["Sword", "15 gp"], &["=A1", ""]])),
        None,
    )
    .unwrap();
    let entry = &state.journal[&entry_id(7)];
    assert_eq!(entry.kind(), JournalEntryKind::Table);
    // A formula-looking cell is kept as typed (formulas come later, ADR-0026).
    assert_eq!(entry.table.as_ref().unwrap().rows[1][0], "=A1");
    assert_eq!(JournalEntryMeta::from(entry).kind, JournalEntryKind::Table);

    // Players edit the cells of a shared editable table, like the text of an entry.
    run(
        &mut state,
        Role::Player,
        edit_table(7, table(&[&["Axe", "10 gp"]])),
        Some(Version(1)),
    )
    .unwrap();
    assert_eq!(
        state.journal[&entry_id(7)].table.as_ref().unwrap().rows,
        vec![vec!["Axe".to_owned(), "10 gp".to_owned()]]
    );

    // A table gets no text, a text gets no table, and a new table has no text.
    assert_eq!(
        run(&mut state, Role::Gm, edit_text(7, "x"), Some(Version(2))),
        Err(RejectReason::invalid("markdown"))
    );
    assert_eq!(
        run(
            &mut state,
            Role::Gm,
            edit_table(2, table(&[&["a"]])),
            Some(Version(1))
        ),
        Err(RejectReason::invalid("table"))
    );
    let mut with_text = create_table(8, false, table(&[&["a"]]));
    if let Intent::CreateJournalEntry { markdown, .. } = &mut with_text {
        *markdown = "text".into();
    }
    assert_eq!(
        run(&mut state, Role::Gm, with_text, None),
        Err(RejectReason::invalid("markdown"))
    );
}

#[test]
fn tables_out_of_shape_or_limits_are_rejected() {
    let ragged = JournalTable {
        rows: vec![vec!["only one".into()]],
        ..table(&[&["a", "b"]])
    };
    let no_columns = JournalTable {
        columns: Vec::new(),
        rows: Vec::new(),
    };
    let mut narrow = table(&[&["a"]]);
    narrow.columns[0].width = MIN_COLUMN_WIDTH - 1;
    let long_cell = table(&[&["x".repeat(MAX_CELL_LEN + 1).as_str()]]);
    let control = table(&[&["bell\u{7}"]]);
    let too_many_rows = JournalTable {
        rows: vec![vec![String::new()]; MAX_TABLE_ROWS + 1],
        ..table(&[&["a"]])
    };
    for cells in [
        ragged,
        no_columns,
        narrow,
        long_cell,
        control,
        too_many_rows,
    ] {
        let mut state = journal_state();
        assert_eq!(
            run(&mut state, Role::Gm, create_table(9, false, cells), None),
            Err(RejectReason::invalid("table"))
        );
    }
    // Line breaks and tabs inside a cell are fine.
    let mut state = journal_state();
    run(
        &mut state,
        Role::Gm,
        create_table(9, false, table(&[&["two\nlines\tand a tab"]])),
        None,
    )
    .unwrap();
}
