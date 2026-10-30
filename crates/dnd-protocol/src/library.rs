//! Asset libraries with folders and sounds played to everyone (ADR-0026).

use serde::{Deserialize, Serialize};

use crate::ids::{AssetId, LibraryFolderId, Version};

/// Which library a folder belongs to: pictures (map and token images) or audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Library {
    Images,
    Audio,
}

/// A folder of a library. Folders nest; sibling names differ in more than case; they are sorted by
/// name. GM roles only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LibraryFolderDto {
    pub id: LibraryFolderId,
    pub library: Library,
    /// `None` for a folder at the root of its library.
    pub parent: Option<LibraryFolderId>,
    pub name: String,
    pub version: Version,
}

/// Most folders in all libraries of a campaign.
pub const MAX_LIBRARY_FOLDERS: usize = 1000;

/// A sound being played to everyone. The file is the audio asset's own (`sha256`), named neither
/// here nor to players: the hash is enough to fetch it (ADR-0026).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SoundDto {
    pub asset_id: AssetId,
    pub sha256: String,
    pub mime: String,
    /// 0..=`MAX_VOLUME`, scaled by each listener's own volume.
    pub volume: u8,
    /// Plays until stopped (music); a sound that does not loop plays once.
    pub looped: bool,
    /// Last commanded position; no wall clock or drift correction (ADR-0028).
    #[serde(default)]
    pub position_ms: u32,
    #[serde(default)]
    pub paused: bool,
}

/// Loudest volume of a sound.
pub const MAX_VOLUME: u8 = 100;
/// Most retained sounds (looping or paused) at once.
pub const MAX_LOOPING_SOUNDS: usize = 8;
/// Largest commanded playback position: 24 hours.
pub const MAX_SOUND_POSITION_MS: u32 = 24 * 60 * 60 * 1000;
