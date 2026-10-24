use thiserror::Error;

/// Everything that can go wrong while reading, validating or activating a plugin.
///
/// Messages are for logs; the UI shows a localized text by [`PluginError::code`] (stable strings).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PluginError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Not a ZIP, a corrupt ZIP, a ZIP feature we do not read, or an ambiguous ZIP (duplicate
    /// names, local headers that disagree with the central directory, overlapping entries).
    #[error("not a valid .dndplugin archive: {reason}")]
    InvalidArchive { reason: String },
    #[error("{what} is too large (limit {limit})")]
    TooLarge { what: String, limit: u64 },
    /// A name outside the whitelist (this is also how `../` and absolute paths are rejected).
    #[error("unexpected file {name:?} in the package")]
    UnexpectedFile { name: String },
    /// An encrypted entry or a symbolic link.
    #[error("{name:?} is not allowed: {reason}")]
    UnsafeEntry { name: String, reason: String },
    #[error("{name} is missing from the package")]
    MissingFile { name: String },
    /// An asset whose content does not match the SHA-256 in its name.
    #[error("{name:?} does not match its SHA-256 name")]
    HashMismatch { name: String },
    /// An asset whose content is not the image format its extension says.
    #[error("{name:?} is not a valid {format} image")]
    InvalidAsset { name: String, format: String },
    #[error("plugin format {format} is not supported (this app reads format {supported})")]
    UnsupportedFormat { format: u64, supported: u32 },
    #[error("manifest.json: {reason}")]
    InvalidManifest { reason: String },
    /// `system.json` or a compendium is malformed.
    #[error("{file}: {reason}")]
    InvalidDocument { file: String, reason: String },
    /// The sheet data schema uses something outside the supported JSON Schema subset.
    #[error("{file}: {reason}")]
    UnsupportedSchema { file: String, reason: String },
    /// The layout is malformed or references a field, computed value, roll or resource that does
    /// not exist.
    #[error("{file}: {reason}")]
    InvalidLayout { file: String, reason: String },
    #[error("plugin {id} requires app version {required}, this is {app}")]
    IncompatibleApp {
        id: String,
        required: String,
        app: String,
    },
    #[error("plugin {id} requires {dependency} {required}, which is not installed")]
    MissingDependency {
        id: String,
        dependency: String,
        required: String,
    },
    #[error("plugin {id} requires {dependency} {required}, installed is {found}")]
    DependencyVersionMismatch {
        id: String,
        dependency: String,
        required: String,
        found: String,
    },
    /// The dependency is installed in a matching version but is itself deactivated.
    #[error("plugin {id} requires {dependency}, which is deactivated")]
    DependencyInactive { id: String, dependency: String },
    #[error("plugin {id} is listed more than once")]
    DuplicatePlugin { id: String },
    /// A campaign has exactly one system plugin (ADR-0005).
    #[error("plugin {id} is a rule system, but the campaign already uses {existing}")]
    MultipleSystems { id: String, existing: String },
    /// A bug in this crate (e.g. an embedded meta-schema that does not compile).
    #[error("internal error: {reason}")]
    Internal { reason: String },
}

impl PluginError {
    /// Stable machine-readable code for the UI and IPC.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::InvalidArchive { .. } => "invalid_archive",
            Self::TooLarge { .. } => "too_large",
            Self::UnexpectedFile { .. } => "unexpected_file",
            Self::UnsafeEntry { .. } => "unsafe_entry",
            Self::MissingFile { .. } => "missing_file",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::InvalidAsset { .. } => "invalid_asset",
            Self::UnsupportedFormat { .. } => "unsupported_format",
            Self::InvalidManifest { .. } => "invalid_manifest",
            Self::InvalidDocument { .. } => "invalid_document",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::InvalidLayout { .. } => "invalid_layout",
            Self::IncompatibleApp { .. } => "incompatible_app",
            Self::MissingDependency { .. } => "missing_dependency",
            Self::DependencyVersionMismatch { .. } => "dependency_version_mismatch",
            Self::DependencyInactive { .. } => "dependency_inactive",
            Self::DuplicatePlugin { .. } => "duplicate_plugin",
            Self::MultipleSystems { .. } => "multiple_systems",
            Self::Internal { .. } => "internal",
        }
    }
}

pub type PluginResult<T> = Result<T, PluginError>;
