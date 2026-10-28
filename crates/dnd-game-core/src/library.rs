//! Libraries with folders and sounds played to everyone (ADR-0026). GM roles only: players are
//! refused in `permissions::authorize` before this runs.

use dnd_protocol::{
    AssetDto, AssetId, AssetUsage, Event, Intent, Library, LibraryFolderDto, LibraryFolderId,
    MAX_FOLDER_DEPTH, MAX_LIBRARY_FOLDERS, MAX_LOOPING_SOUNDS, MAX_VOLUME, RejectReason, SoundDto,
    Version,
};

use crate::apply::{bump, valid_name};
use crate::state::GameState;

/// Events for a library or sound intent; `None` if `intent` is about something else.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::CreateLibraryFolder {
            id,
            library,
            parent,
            name,
        } => (|| {
            if state.library_folders.len() >= MAX_LIBRARY_FOLDERS {
                return Err(RejectReason::invalid("id"));
            }
            known_folder(state, *library, *parent, "parent")?;
            if depth(state, *parent) + 1 > MAX_FOLDER_DEPTH {
                return Err(RejectReason::invalid("parent"));
            }
            Ok(vec![Event::LibraryFolderCreated {
                folder: LibraryFolderDto {
                    id: *id,
                    library: *library,
                    parent: *parent,
                    name: sibling_name(state, *library, *parent, name, None)?,
                    version: Version::INITIAL,
                },
            }])
        })(),
        Intent::RenameLibraryFolder { id, name } => change_folder(state, *id, |f| {
            f.name = sibling_name(state, f.library, f.parent, name, Some(f.id))?;
            Ok(())
        }),
        Intent::MoveLibraryFolder { id, parent } => change_folder(state, *id, |f| {
            known_folder(state, f.library, *parent, "parent")?;
            if parent.is_some_and(|p| p == f.id || is_inside(state, p, f.id)) {
                return Err(RejectReason::invalid("parent"));
            }
            if depth(state, *parent) + subtree_height(state, f.id) > MAX_FOLDER_DEPTH {
                return Err(RejectReason::invalid("parent"));
            }
            f.name = sibling_name(state, f.library, *parent, &f.name, Some(f.id))?;
            f.parent = *parent;
            Ok(())
        }),
        Intent::DeleteLibraryFolder { id } => (|| {
            if !state.library_folders.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            delete_folder(state, *id)
        })(),
        Intent::MoveAsset { id, folder_id } => change_asset(state, *id, |a| {
            known_folder(state, library_of(a.usage), *folder_id, "folder_id")?;
            a.folder_id = *folder_id;
            Ok(())
        }),
        Intent::RenameAsset { id, name } => change_asset(state, *id, |a| {
            a.original_name = Some(valid_name(name, "name")?);
            Ok(())
        }),
        Intent::PlaySound {
            asset_id,
            volume,
            looped,
        } => (|| {
            let asset = state
                .assets
                .get(asset_id)
                .ok_or_else(|| RejectReason::invalid("asset_id"))?;
            Ok(vec![Event::SoundPlayed {
                sound: sound(state, asset, *volume, *looped)?,
            }])
        })(),
        Intent::StopSound { asset_id } => Ok(vec![Event::SoundStopped {
            asset_id: *asset_id,
        }]),
        _ => return None,
    };
    Some(result)
}

/// The library an asset is shown in.
pub(crate) fn library_of(usage: AssetUsage) -> Library {
    match usage {
        AssetUsage::Map | AssetUsage::Token => Library::Images,
        AssetUsage::Audio => Library::Audio,
    }
}

/// A sound of an audio asset, within the limits (also used by triggers).
pub(crate) fn sound(
    state: &GameState,
    asset: &AssetDto,
    volume: u8,
    looped: bool,
) -> Result<SoundDto, RejectReason> {
    if asset.usage != AssetUsage::Audio {
        return Err(RejectReason::invalid("asset_id"));
    }
    if volume > MAX_VOLUME {
        return Err(RejectReason::invalid("volume"));
    }
    if looped && !state.sounds.contains_key(&asset.id) && state.sounds.len() >= MAX_LOOPING_SOUNDS {
        return Err(RejectReason::invalid("looped"));
    }
    Ok(SoundDto {
        asset_id: asset.id,
        sha256: asset.sha256.clone(),
        mime: asset.mime.clone(),
        volume,
        looped,
    })
}

fn change_folder(
    state: &GameState,
    id: LibraryFolderId,
    edit: impl FnOnce(&mut LibraryFolderDto) -> Result<(), RejectReason>,
) -> Result<Vec<Event>, RejectReason> {
    let mut folder = state
        .library_folders
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    edit(&mut folder)?;
    folder.version = bump(folder.version)?;
    Ok(vec![Event::LibraryFolderUpdated { folder }])
}

fn change_asset(
    state: &GameState,
    id: AssetId,
    edit: impl FnOnce(&mut AssetDto) -> Result<(), RejectReason>,
) -> Result<Vec<Event>, RejectReason> {
    let mut asset = state
        .assets
        .get(&id)
        .cloned()
        .ok_or(RejectReason::NotFound)?;
    edit(&mut asset)?;
    asset.version = bump(asset.version)?;
    Ok(vec![Event::AssetUpdated {
        asset: Box::new(asset),
    }])
}

/// The folder's assets go back to the root first, then the folders go, deepest first.
fn delete_folder(state: &GameState, id: LibraryFolderId) -> Result<Vec<Event>, RejectReason> {
    let mut folders = vec![id];
    let mut i = 0;
    while let Some(&current) = folders.get(i) {
        folders.extend(
            state
                .library_folders
                .values()
                .filter(|f| f.parent == Some(current))
                .map(|f| f.id),
        );
        i += 1;
    }
    let mut events = Vec::new();
    for asset in state.assets.values() {
        if asset.folder_id.is_some_and(|f| folders.contains(&f)) {
            let mut moved = asset.clone();
            moved.folder_id = None;
            moved.version = bump(moved.version)?;
            events.push(Event::AssetUpdated {
                asset: Box::new(moved),
            });
        }
    }
    events.extend(
        folders
            .iter()
            .rev()
            .map(|&id| Event::LibraryFolderDeleted { id }),
    );
    Ok(events)
}

/// `None` (the root) or an existing folder of `library`.
fn known_folder(
    state: &GameState,
    library: Library,
    folder: Option<LibraryFolderId>,
    field: &str,
) -> Result<(), RejectReason> {
    match folder {
        Some(id)
            if state
                .library_folders
                .get(&id)
                .is_none_or(|f| f.library != library) =>
        {
            Err(RejectReason::invalid(field))
        }
        _ => Ok(()),
    }
}

fn depth(state: &GameState, folder: Option<LibraryFolderId>) -> usize {
    let mut depth = 0;
    let mut current = folder;
    while let Some(id) = current {
        depth += 1;
        if depth > MAX_FOLDER_DEPTH {
            break;
        }
        current = state.library_folders.get(&id).and_then(|f| f.parent);
    }
    depth
}

fn subtree_height(state: &GameState, id: LibraryFolderId) -> usize {
    1 + state
        .library_folders
        .values()
        .filter(|f| f.parent == Some(id))
        .map(|f| subtree_height(state, f.id))
        .max()
        .unwrap_or(0)
}

fn is_inside(state: &GameState, folder: LibraryFolderId, ancestor: LibraryFolderId) -> bool {
    let mut current = state.library_folders.get(&folder).and_then(|f| f.parent);
    let mut steps = 0;
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        steps += 1;
        if steps > MAX_FOLDER_DEPTH {
            break;
        }
        current = state.library_folders.get(&id).and_then(|f| f.parent);
    }
    false
}

fn sibling_name(
    state: &GameState,
    library: Library,
    parent: Option<LibraryFolderId>,
    name: &str,
    except: Option<LibraryFolderId>,
) -> Result<String, RejectReason> {
    let name = valid_name(name, "name")?;
    let taken = state.library_folders.values().any(|f| {
        f.library == library
            && f.parent == parent
            && Some(f.id) != except
            && f.name.to_lowercase() == name.to_lowercase()
    });
    if taken {
        return Err(RejectReason::AlreadyExists);
    }
    Ok(name)
}
