//! Journal intents (CLAUDE.md §12): Markdown entries in folders with visibility and optional
//! editing by players. Permissions are checked in `permissions::authorize` before this runs.

use dnd_protocol::{
    Event, Intent, JournalEntryDto, MAX_FOLDER_LEN, MAX_JOURNAL_BYTES, RejectReason, Version,
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
            folder,
            title,
            markdown,
            visibility,
            players_can_edit,
        } => (|| {
            let folder = valid_folder(folder)?;
            // New entries go last in their folder; deterministic, no client-supplied order.
            let sort_order = state
                .journal
                .values()
                .filter(|e| e.folder == folder)
                .map(|e| e.sort_order)
                .max()
                .map_or(0, |max| max.saturating_add(1));
            Ok(vec![Event::JournalEntryCreated {
                entry: JournalEntryDto {
                    id: *id,
                    folder,
                    title: valid_name(title, "title")?,
                    markdown: valid_markdown(markdown)?,
                    sort_order,
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
            folder,
            sort_order,
        } => change(state, *id, |e| {
            e.folder = valid_folder(folder)?;
            e.sort_order = *sort_order;
            Ok(())
        }),
        Intent::DeleteJournalEntry { id } => Ok(vec![Event::JournalEntryDeleted { id: *id }]),
        _ => return None,
    };
    Some(result)
}

fn change(
    state: &GameState,
    id: dnd_protocol::JournalEntryId,
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

fn valid_markdown(markdown: &str) -> Result<String, RejectReason> {
    if markdown.len() > MAX_JOURNAL_BYTES {
        return Err(RejectReason::invalid("markdown"));
    }
    Ok(markdown.to_owned())
}

/// `/`-separated path of non-empty, trimmed segments (`""` = root), at most
/// [`MAX_FOLDER_LEN`] characters, no control characters. Normalizes `" a / b "` to `"a/b"`.
pub(crate) fn valid_folder(folder: &str) -> Result<String, RejectReason> {
    let invalid = || RejectReason::invalid("folder");
    if folder.trim().is_empty() {
        return Ok(String::new());
    }
    let segments: Vec<&str> = folder.split('/').map(str::trim).collect();
    if segments.iter().any(|s| s.is_empty()) || folder.chars().any(char::is_control) {
        return Err(invalid());
    }
    let normalized = segments.join("/");
    if normalized.chars().count() > MAX_FOLDER_LEN {
        return Err(invalid());
    }
    Ok(normalized)
}
