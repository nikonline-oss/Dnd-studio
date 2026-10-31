//! Playlist tracks are copied content addresses; later library changes never rewrite them.
use crate::{
    apply::{bump, valid_name},
    state::GameState,
};
use dnd_protocol::{
    AssetUsage, Event, Intent, Library, MAX_PLAYLIST_TRACKS, MAX_PLAYLISTS, PlaylistDto,
    PlaylistTrackDto, RejectReason, Version,
};

pub(crate) fn apply(state: &GameState, intent: &Intent) -> Result<Vec<Event>, RejectReason> {
    match intent {
        Intent::CreatePlaylist {
            id,
            name,
            folder_id,
            asset_ids,
        } => {
            if state.playlists.len() >= MAX_PLAYLISTS {
                return Err(RejectReason::LimitExceeded {
                    limit: "playlists".into(),
                });
            }
            folder(state, *folder_id)?;
            if asset_ids.len() > MAX_PLAYLIST_TRACKS {
                return Err(RejectReason::invalid("tracks"));
            }
            let tracks = asset_ids
                .iter()
                .map(|id| {
                    let asset = state
                        .assets
                        .get(id)
                        .filter(|a| a.usage == AssetUsage::Audio)
                        .ok_or_else(|| RejectReason::invalid("asset_ids"))?;
                    Ok(PlaylistTrackDto {
                        asset_id: *id,
                        sha256: asset.sha256.clone(),
                        mime: asset.mime.clone(),
                        title: asset
                            .original_name
                            .clone()
                            .unwrap_or_else(|| "Audio".into()),
                    })
                })
                .collect::<Result<Vec<_>, RejectReason>>()?;
            Ok(vec![Event::PlaylistCreated {
                playlist: PlaylistDto {
                    id: *id,
                    name: valid_name(name, "name")?,
                    folder_id: *folder_id,
                    tracks,
                    version: Version::INITIAL,
                },
            }])
        }
        Intent::UpdatePlaylist { id, name, tracks } => {
            let mut playlist = state
                .playlists
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            if tracks.len() > MAX_PLAYLIST_TRACKS {
                return Err(RejectReason::invalid("tracks"));
            }
            for track in tracks {
                valid_name(&track.title, "tracks.title")?;
                let library = state.assets.get(&track.asset_id).is_some_and(|a| {
                    a.usage == AssetUsage::Audio && a.sha256 == track.sha256 && a.mime == track.mime
                });
                let retained = state.playlists.values().any(|p| {
                    p.tracks.iter().any(|t| {
                        t.sha256 == track.sha256
                            && t.mime == track.mime
                            && t.asset_id == track.asset_id
                    })
                });
                if !library && !retained {
                    return Err(RejectReason::invalid("tracks"));
                }
            }
            playlist.name = valid_name(name, "name")?;
            playlist.tracks = tracks.clone();
            playlist.version = bump(playlist.version)?;
            Ok(vec![Event::PlaylistUpdated { playlist }])
        }
        Intent::MovePlaylist { id, folder_id } => {
            folder(state, *folder_id)?;
            let mut playlist = state
                .playlists
                .get(id)
                .cloned()
                .ok_or(RejectReason::NotFound)?;
            playlist.folder_id = *folder_id;
            playlist.version = bump(playlist.version)?;
            Ok(vec![Event::PlaylistUpdated { playlist }])
        }
        Intent::DeletePlaylist { id } => Ok(vec![Event::PlaylistDeleted { id: *id }]),
        _ => Err(RejectReason::invalid("intent")),
    }
}

fn folder(
    state: &GameState,
    id: Option<dnd_protocol::LibraryFolderId>,
) -> Result<(), RejectReason> {
    if id.is_some_and(|id| {
        !state
            .library_folders
            .get(&id)
            .is_some_and(|f| f.library == Library::Audio)
    }) {
        return Err(RejectReason::invalid("folder_id"));
    }
    Ok(())
}
