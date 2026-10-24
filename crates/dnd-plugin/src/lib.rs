//! Plugin packages (`.dndplugin`): manifest, rules of system plugins, sheet schema and layout,
//! compendia, assets. Safe reading, validation, app compatibility, dependency resolution.
//!
//! The format is described in `docs/plugin-format.md`; scope and boundaries in `README.md`.
//! Expressions and dice in `system.json` are carried as strings: their syntax is checked by the
//! game core with `dnd-expr`/`dnd-dice` when the plugin's rules are applied, not here.
//! Signatures (catalog, ed25519) come in stage 5; until then every local package is unsigned.

mod compat;
mod dto;
mod error;
mod meta;
mod model;
mod package;
mod sheet;

pub use compat::{Deactivated, Resolution, check_app_compat, check_install, resolve};
pub use dto::{InstalledPluginDto, SystemRulesDto};
pub use error::{PluginError, PluginResult};
pub use model::{
    Compendium, CompendiumEntry, DEFAULT_HEALTH_THRESHOLDS, Label, LayoutNode, LayoutTab,
    LocalizedText, Manifest, PLUGIN_FORMAT, PluginKind, ResourceDef, RollDef, SheetRef,
    SystemRules, Widget,
};
pub use package::{
    AssetFormat, MANIFEST_FILE, PackageAsset, PackageLimits, PluginPackage, SYSTEM_FILE,
    SystemPackage, hex_sha256, read_package, read_package_file,
};
pub use sheet::{MAX_LAYOUT_DEPTH, MAX_SCHEMA_DEPTH, check_data};

/// Re-exported so callers name versions with the same types the manifest uses.
pub use semver;
