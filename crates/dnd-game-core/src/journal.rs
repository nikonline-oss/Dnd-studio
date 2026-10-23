//! Journal intents (CLAUDE.md §12): Markdown entries in nested folders (ADR-0021) with visibility
//! and optional editing by players. Permissions are checked in `permissions::authorize` before
//! this runs; folders are GM-only.

use dnd_protocol::{
    Event, Intent, JournalEntryDto, JournalEntryId, JournalFolderDto, JournalFolderId,
    MAX_FOLDER_DEPTH, MAX_JOURNAL_BYTES, RejectReason, Version,
};

use crate::apply::{bump, valid_name};
use crate::permissions::Actor;
use crate::state::GameState;

/// Events for a journal intent; `None` if `intent` is not about the journal.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
    actor: &Actor,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateJournalEntry {
            id,
            folder_id,
            title,
            markdown,
            visibility,
            players_can_edit,
        } => (|| {
            known_folder(state, *folder_id, "folder_id")?;
            Ok(vec![Event::JournalEntryCreated {
                entry: JournalEntryDto {
                    id: *id,
                    folder_id: *folder_id,
                    title: valid_name(title, "title")?,
                    markdown: valid_markdown(markdown)?,
                    // Last in its folder; deterministic, no client-supplied order.
                    sort_order: next_entry_order(state, *folder_id),
                    visibility: *visibility,
                    players_can_edit: *players_can_edit,
                    author: Some(actor.user_id),
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::UpdateJournalEntry {
            id,
            title,
            markdown,
            visibility,
            players_can_edit,
        } => change(state, *id, |e| {
            if title.is_none()
                && markdown.is_none()
                && visibility.is_none()
                && players_can_edit.is_none()
            {
                return Err(RejectReason::invalid("intent"));
            }
            if let Some(title) = title {
                e.title = valid_name(title, "title")?;
            }
            if let Some(markdown) = markdown {
                e.markdown = valid_markdown(markdown)?;
            }
            if let Some(visibility) = visibility {
                e.visibility = *visibility;
            }
            if let Some(can_edit) = players_can_edit {
                e.players_can_edit = *can_edit;
            }
            Ok(())
        }),
        Intent::MoveJournalEntry {
            id,
            folder_id,
            sort_order,
        } => change(state, *id, |e| {
            known_folder(state, *folder_id, "folder_id")?;
            e.folder_id = *folder_id;
            e.sort_order = *sort_order;
            Ok(())
        }),
        Intent::DeleteJournalEntry { id } => Ok(vec![Event::JournalEntryDeleted { id: *id }]),
        Intent::CreateJournalFolder { id, parent, name } => (|| {
            known_folder(state, *parent, "parent")?;
            if depth(state, *parent) + 1 > MAX_FOLDER_DEPTH {
                return Err(RejectReason::invalid("parent"));
            }
            let name = sibling_name(state, *parent, name, None)?;
            Ok(vec![Event::JournalFolderCreated {
                folder: JournalFolderDto {
                    id: *id,
                    parent: *parent,
                    name,
                    sort_order: next_folder_order(state, *parent),
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::RenameJournalFolder { id, name } => change_folder(state, *id, |f| {
            f.name = sibling_name(state, f.parent, name, Some(f.id))?;
            Ok(())
        }),
        Intent::MoveJournalFolder {
            id,
            parent,
            sort_order,
        } => change_folder(state, *id, |f| {
            known_folder(state, *parent, "parent")?;
            // Never into itself or one of its own subfolders.
            if parent.is_some_and(|p| p == f.id || is_inside(state, p, f.id)) {
                return Err(RejectReason::invalid("parent"));
            }
            if depth(state, *parent) + subtree_height(state, f.id) > MAX_FOLDER_DEPTH {
                return Err(RejectReason::invalid("parent"));
            }
            f.name = sibling_name(state, *parent, &f.name, Some(f.id))?;
            f.parent = *parent;
            f.sort_order = *sort_order;
            Ok(())
        }),
        Intent::DeleteJournalFolder { id } => (|| {
            if !state.journal_folders.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            Ok(delete_folder(state, *id))
        })(),
        _ => return None,
    };
    Some(result)
}

fn change(
    state: &GameState,
    id: JournalEntryId,
    edit: impl FnOnce(&mut JournalEntryDto) -> Result<(), RejectReason>,
) -> Result<Vec<Event>, RejectReason> {
    let mut entry = state
        .journal
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    edit(&mut entry)?;
    entry.version = bump(entry.version)?;
    Ok(vec![Event::JournalEntryUpdated { entry }])
}

fn change_folder(
    state: &GameState,
    id: JournalFolderId,
    edit: impl FnOnce(&mut JournalFolderDto) -> Result<(), RejectReason>,
) -> Result<Vec<Event>, RejectReason> {
    let mut folder = state
        .journal_folders
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    edit(&mut folder)?;
    folder.version = bump(folder.version)?;
    Ok(vec![Event::JournalFolderUpdated { folder }])
}

/// The folder and everything in it: entries first, then folders deepest first, so that applying
/// the events in order never leaves an entry or a folder without its parent.
fn delete_folder(state: &GameState, id: JournalFolderId) -> Vec<Event> {
    let mut folders = vec![id];
    let mut i = 0;
    while let Some(&current) = folders.get(i) {
        folders.extend(
            state
                .journal_folders
                .values()
                .filter(|f| f.parent == Some(current))
                .map(|f| f.id),
        );
        i += 1;
    }
    let mut events: Vec<Event> = state
        .journal
        .values()
        .filter(|e| e.folder_id.is_some_and(|f| folders.contains(&f)))
        .map(|e| Event::JournalEntryDeleted { id: e.id })
        .collect();
    events.extend(
        folders
            .iter()
            .rev()
            .map(|&id| Event::JournalFolderDeleted { id }),
    );
    events
}

/// `None` (the root) or an existing folder.
fn known_folder(
    state: &GameState,
    folder: Option<JournalFolderId>,
    field: &str,
) -> Result<(), RejectReason> {
    match folder {
        Some(id) if !state.journal_folders.contains_key(&id) => Err(RejectReason::invalid(field)),
        _ => Ok(()),
    }
}

/// Nesting level of a folder's contents: 0 for the root, 1 inside a top-level folder, ...
fn depth(state: &GameState, folder: Option<JournalFolderId>) -> usize {
    let mut depth = 0;
    let mut current = folder;
    while let Some(id) = current {
        depth += 1;
        // Parents form a tree (moves never create cycles); the bound guards bad data.
        if depth > MAX_FOLDER_DEPTH {
            break;
        }
        current = state.journal_folders.get(&id).and_then(|f| f.parent);
    }
    depth
}

/// Levels of folders in the subtree of `id`, itself included (1 for a folder without subfolders).
fn subtree_height(state: &GameState, id: JournalFolderId) -> usize {
    1 + state
        .journal_folders
        .values()
        .filter(|f| f.parent == Some(id))
        .map(|f| subtree_height(state, f.id))
        .max()
        .unwrap_or(0)
}

/// `folder` lies somewhere below `ancestor`.
fn is_inside(state: &GameState, folder: JournalFolderId, ancestor: JournalFolderId) -> bool {
    let mut current = state.journal_folders.get(&folder).and_then(|f| f.parent);
    let mut steps = 0;
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        steps += 1;
        if steps > MAX_FOLDER_DEPTH {
            break;
        }
        current = state.journal_folders.get(&id).and_then(|f| f.parent);
    }
    false
}

/// A valid name not used by another folder in `parent` (case-insensitive, as file explorers do).
fn sibling_name(
    state: &GameState,
    parent: Option<JournalFolderId>,
    name: &str,
    except: Option<JournalFolderId>,
) -> Result<String, RejectReason> {
    let name = valid_name(name, "name")?;
    let taken = state.journal_folders.values().any(|f| {
        f.parent == parent && Some(f.id) != except && f.name.to_lowercase() == name.to_lowercase()
    });
    if taken {
        return Err(RejectReason::AlreadyExists);
    }
    Ok(name)
}

fn next_entry_order(state: &GameState, folder: Option<JournalFolderId>) -> i64 {
    state
        .journal
        .values()
        .filter(|e| e.folder_id == folder)
        .map(|e| e.sort_order)
        .max()
        .map_or(0, |max| max.saturating_add(1))
}

fn next_folder_order(state: &GameState, parent: Option<JournalFolderId>) -> i64 {
    state
        .journal_folders
        .values()
        .filter(|f| f.parent == parent)
        .map(|f| f.sort_order)
        .max()
        .map_or(0, |max| max.saturating_add(1))
}

fn valid_markdown(markdown: &str) -> Result<String, RejectReason> {
    if markdown.len() > MAX_JOURNAL_BYTES {
        return Err(RejectReason::invalid("markdown"));
    }
    Ok(markdown.to_owned())
}
