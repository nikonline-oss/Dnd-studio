//! Independent audio queues: tracks keep content addresses, not live library references.
use crate::{AssetId, LibraryFolderId, PlaylistId, Version};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistTrackDto {
    /// Origin only; deleting the library row does not remove this copied track.
    pub asset_id: AssetId,
    pub sha256: String,
    pub mime: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistDto {
    pub id: PlaylistId,
    pub name: String,
    pub folder_id: Option<LibraryFolderId>,
    pub tracks: Vec<PlaylistTrackDto>,
    pub version: Version,
}

pub const MAX_PLAYLISTS: usize = 500;
pub const MAX_PLAYLIST_TRACKS: usize = 100;
