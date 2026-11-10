//! The plugin catalog screen's backend (ADR-0006, ADR-0060, roadmap R3.4).
//!
//! Fetching and verification live in `dnd-net`/`dnd-plugin`; this module only adapts their result
//! to the UI and installs what the user picked through the same `install_package` path as the file
//! dialog. A package is verified twice: when downloaded and again by the installer's own checks.

use dnd_net::CatalogClient;
use dnd_plugin::catalog::{Catalog, CatalogEntry};
use dnd_plugin::semver::Version;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::commands::{AppState, install_catalog_package};
use crate::error::IpcError;

/// One row of the catalog as the UI sees it.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct CatalogEntryView {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub license: String,
    pub app_compat: String,
    pub sha256: String,
    /// New installations and updates are refused; an installed copy keeps running.
    pub revoked: bool,
    /// The version installed in the open campaign, if any.
    pub installed_version: Option<String>,
    /// This build satisfies `app_compat`.
    pub compatible: bool,
    /// A newer compatible version than the installed one, or the first install.
    pub can_install: bool,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct CatalogView {
    pub key_id: String,
    pub generated_at: String,
    pub plugins: Vec<CatalogEntryView>,
}

/// App version this build advertises for `app_compat` (matches the bundle's version).
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

fn decode_public_key(hex: &str) -> Result<Vec<u8>, IpcError> {
    let trimmed = hex.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(IpcError::user(
            "catalog_key",
            "the catalog public key must be 64 hexadecimal characters",
        ));
    }
    (0..trimmed.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&trimmed[index..index + 2], 16)
                .map_err(|_| IpcError::user("catalog_key", "the catalog public key is not hex"))
        })
        .collect()
}

fn client(base_url: &str, public_key: &str) -> Result<CatalogClient, IpcError> {
    let key = decode_public_key(public_key)?;
    CatalogClient::new(base_url, key)
        .map_err(|error| IpcError::user("catalog_unreachable", format!("catalog address: {error}")))
}

/// The real source: `dnd-net` over HTTP with the app's cache directory.
#[derive(Debug)]
pub struct HttpSource;

impl HttpSource {
    async fn index(base_url: &str, public_key: &str) -> Result<Catalog, IpcError> {
        client(base_url, public_key)?
            .fetch()
            .await
            .map_err(|error| IpcError::user(error.code(), error.to_string()))
    }

    async fn package_bytes(
        base_url: &str,
        public_key: &str,
        entry: &CatalogEntry,
        cache: &std::path::Path,
    ) -> Result<Vec<u8>, IpcError> {
        client(base_url, public_key)?
            .package_bytes(entry, cache)
            .await
            .map_err(|error| IpcError::user(error.code(), error.to_string()))
    }
}

/// Builds the UI view: installability is decided here, not in the UI.
#[must_use]
pub fn view(catalog: &Catalog, installed: &[(String, String)]) -> CatalogView {
    let app = Version::parse(&app_version()).ok();
    let plugins = catalog
        .plugins
        .iter()
        .map(|entry| {
            let installed_version = installed
                .iter()
                .find(|(id, _)| *id == entry.id)
                .map(|(_, version)| version.clone());
            let compatible = app
                .as_ref()
                .is_some_and(|app| entry.app_compat.matches(app));
            let newer = installed_version
                .as_deref()
                .and_then(|version| Version::parse(version).ok())
                .is_none_or(|current| entry.version > current);
            CatalogEntryView {
                id: entry.id.clone(),
                version: entry.version.to_string(),
                kind: entry.kind.as_str().to_owned(),
                license: entry.license.clone(),
                app_compat: entry.app_compat.to_string(),
                sha256: entry.sha256.clone(),
                revoked: entry.revoked,
                installed_version,
                compatible,
                can_install: !entry.revoked && compatible && newer,
                dependencies: entry
                    .dependencies
                    .iter()
                    .map(|(id, range)| format!("{id} {range}"))
                    .collect(),
            }
        })
        .collect();
    CatalogView {
        key_id: catalog.key_id.clone(),
        generated_at: catalog.generated_at.clone(),
        plugins,
    }
}

/// Downloads and verifies the catalog index. `base_url` and `public_key` come from the app's
/// settings; an unset pair is reported as such rather than silently ignored.
#[tauri::command]
#[specta::specta]
pub async fn catalog_fetch(
    state: State<'_, AppState>,
    base_url: String,
    public_key: String,
) -> Result<CatalogView, IpcError> {
    if base_url.trim().is_empty() || public_key.trim().is_empty() {
        return Err(IpcError::user(
            "catalog_unconfigured",
            "set the catalog address and public key first",
        ));
    }
    let catalog = HttpSource::index(base_url.trim(), public_key.trim()).await?;
    let installed = installed_plugins(&state).await?;
    Ok(view(&catalog, &installed))
}

/// Installs or updates one catalog entry into the open campaign (local or hosted).
#[tauri::command]
#[specta::specta]
pub async fn catalog_install<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    base_url: String,
    public_key: String,
    id: String,
    version: String,
) -> Result<dnd_protocol::IntentOutcome, IpcError> {
    let catalog = HttpSource::index(base_url.trim(), public_key.trim()).await?;
    let version = Version::parse(&version)
        .map_err(|_| IpcError::user("invalid", "the version is not semver"))?;
    let entry = catalog
        .installable(&id, &version)
        .map_err(|error| IpcError::user(error.code(), error.to_string()))?;
    let cache = state
        .dirs
        .get()
        .map(|dirs| dirs.catalog_cache.clone())
        .ok_or_else(|| IpcError::internal("not_ready", "app directories are not initialized"))?;
    let bytes =
        HttpSource::package_bytes(base_url.trim(), public_key.trim(), entry, &cache).await?;
    let installed = installed_plugins(&state).await?;
    if let Some(current) = installed
        .iter()
        .find(|(installed_id, _)| *installed_id == entry.id)
        .map(|(_, version)| version.clone())
        && Version::parse(&current).is_ok_and(|current| current > entry.version)
    {
        // Downgrades erase data a newer version wrote; they are a deliberate act elsewhere.
        return Err(IpcError::user(
            "downgrade",
            format!(
                "{} {current} is newer than {} {}",
                entry.id, entry.id, entry.version
            ),
        ));
    }
    install_catalog_package(&app, &state, bytes).await
}

async fn installed_plugins(state: &AppState) -> Result<Vec<(String, String)>, IpcError> {
    let remote = state.remote.lock().await.clone();
    if let Some(remote) = remote {
        return Ok(remote
            .with_state(|game| {
                game.plugins
                    .values()
                    .map(|plugin| (plugin.id.clone(), plugin.version.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default());
    }
    let guard = state.server.lock().await;
    Ok(guard
        .as_ref()
        .map(|local| {
            local
                .state()
                .plugins
                .values()
                .map(|plugin| (plugin.id.clone(), plugin.version.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_plugin::PluginKind;
    use std::collections::BTreeMap;

    fn entry(id: &str, version: &str, compat: &str, revoked: bool) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            version: Version::parse(version).unwrap(),
            kind: PluginKind::Content,
            license: "MIT".to_owned(),
            app_compat: dnd_plugin::semver::VersionReq::parse(compat).unwrap(),
            sha256: "a".repeat(64),
            archive_url: "https://example.org/p.dndplugin".to_owned(),
            signature: "b".repeat(128),
            revoked,
            dependencies: BTreeMap::new(),
        }
    }

    fn catalog(entries: Vec<CatalogEntry>) -> Catalog {
        Catalog {
            key_id: "catalog".to_owned(),
            generated_at: "2026-11-08T00:00:00Z".to_owned(),
            plugins: entries,
        }
    }

    #[test]
    fn a_fresh_compatible_package_can_be_installed() {
        let view = view(
            &catalog(vec![entry("demo", "1.0.0", ">=0.0.1", false)]),
            &[],
        );
        let row = &view.plugins[0];
        assert!(row.compatible);
        assert!(row.can_install);
        assert_eq!(row.installed_version, None);
    }

    #[test]
    fn updates_show_the_installed_version_and_only_newer_ones_install() {
        let entries = vec![
            entry("demo", "1.0.0", ">=0.0.1", false),
            entry("demo", "1.2.0", ">=0.0.1", false),
        ];
        let installed = vec![("demo".to_owned(), "1.1.0".to_owned())];
        let view = view(&catalog(entries), &installed);
        assert_eq!(view.plugins[0].installed_version.as_deref(), Some("1.1.0"));
        assert!(!view.plugins[0].can_install, "1.0.0 is older");
        assert!(view.plugins[1].can_install, "1.2.0 is newer");
    }

    #[test]
    fn revoked_and_incompatible_entries_are_shown_but_not_installable() {
        let view = view(
            &catalog(vec![
                entry("gone", "1.0.0", ">=0.0.1", true),
                entry("future", "1.0.0", ">=9.0.0", false),
            ]),
            &[],
        );
        assert!(view.plugins[0].revoked && !view.plugins[0].can_install);
        assert!(!view.plugins[1].compatible && !view.plugins[1].can_install);
    }

    #[test]
    fn a_public_key_must_be_32_hex_bytes() {
        assert!(decode_public_key(&"a".repeat(64)).is_ok());
        assert!(decode_public_key("abc").is_err());
        assert!(decode_public_key(&"z".repeat(64)).is_err());
    }
}
