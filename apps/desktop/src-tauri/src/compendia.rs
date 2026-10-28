//! Compendia of the campaign's plugins (CLAUDE.md §12, ADR-0025): read from the packages kept in
//! the campaign (`assets/plugins/<sha256>.dndplugin`), not from the game state, so a snapshot never
//! carries thousands of entries. Packages are content-addressed, so parsed compendia are cached
//! by hash and never go stale.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use dnd_plugin::{Compendium, PackageLimits, read_package};
use dnd_protocol::Label;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::commands::AppState;
use crate::error::IpcError;
use crate::local_server::ASSETS_DIR;

/// A compendium of an installed plugin.
#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct CompendiumInfo {
    pub plugin_id: String,
    pub id: String,
    pub name: Label,
    /// What its entries are (`spell`, `monster`, …), as the plugin calls it.
    pub entry_type: String,
    pub count: u32,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct CompendiumEntryDto {
    pub id: String,
    pub name: Label,
    /// The entry's fields as the plugin wrote them.
    #[specta(type = specta_typescript::Unknown)]
    pub data: Value,
}

/// One page of matching entries and how many match in all.
#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct CompendiumPage {
    pub entries: Vec<CompendiumEntryDto>,
    pub total: u32,
}

/// Longest page the UI may ask for.
const MAX_PAGE: u32 = 500;

type Parsed = Arc<Vec<Compendium>>;
static CACHE: LazyLock<Mutex<HashMap<String, Parsed>>> = LazyLock::new(Mutex::default);

/// The compendia of the package `sha256` in `campaign` (empty if it cannot be read).
fn compendia_of(campaign: &Path, sha256: &str) -> Parsed {
    if let Ok(cache) = CACHE.lock()
        && let Some(found) = cache.get(sha256)
    {
        return Arc::clone(found);
    }
    let path = campaign
        .join(ASSETS_DIR)
        .join("plugins")
        .join(format!("{sha256}.dndplugin"));
    let parsed: Parsed = match std::fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| read_package(&bytes, PackageLimits::default()).map_err(|e| e.to_string()))
    {
        Ok(package) => Arc::new(package.compendia),
        Err(error) => {
            tracing::warn!(%sha256, %error, "plugin package unreadable; no compendia");
            Arc::new(Vec::new())
        }
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(sha256.to_owned(), Arc::clone(&parsed));
    }
    parsed
}

/// `(plugin id, compendia)` of every plugin installed in the open campaign.
async fn installed(state: &AppState) -> Result<Vec<(String, Parsed)>, IpcError> {
    // In a session the packages stay on the GM's machine; viewing them there comes with stage 4.
    if state.remote.lock().await.is_some() {
        return Ok(Vec::new());
    }
    let (folder, plugins) = {
        let guard = state.server.lock().await;
        let server = guard.as_ref().ok_or_else(IpcError::no_campaign)?;
        let plugins: Vec<(String, String)> = server
            .state()
            .plugins
            .values()
            .map(|p| (p.id.clone(), p.archive_sha256.clone()))
            .collect();
        (server.folder().to_owned(), plugins)
    };
    tauri::async_runtime::spawn_blocking(move || {
        plugins
            .into_iter()
            .map(|(id, sha)| {
                let parsed = compendia_of(&folder, &sha);
                (id, parsed)
            })
            .collect()
    })
    .await
    .map_err(|e| IpcError::internal("compendia_failed", e.to_string()))
}

/// The compendia of the campaign's plugins.
#[tauri::command]
#[specta::specta]
pub async fn compendium_list(state: State<'_, AppState>) -> Result<Vec<CompendiumInfo>, IpcError> {
    Ok(list(&installed(&state).await?))
}

fn list(installed: &[(String, Parsed)]) -> Vec<CompendiumInfo> {
    installed
        .iter()
        .flat_map(|(plugin_id, compendia)| {
            compendia.iter().map(move |c| CompendiumInfo {
                plugin_id: plugin_id.clone(),
                id: c.id.clone(),
                name: c.name.clone(),
                entry_type: c.entry_type.clone(),
                count: u32::try_from(c.entries.len()).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

/// Entries of a compendium whose name (in any language) contains `query`, ignoring case.
#[tauri::command]
#[specta::specta]
pub async fn compendium_entries(
    state: State<'_, AppState>,
    plugin_id: String,
    compendium_id: String,
    query: String,
    offset: u32,
    limit: u32,
) -> Result<CompendiumPage, IpcError> {
    let installed = installed(&state).await?;
    page(
        &installed,
        &plugin_id,
        &compendium_id,
        &query,
        offset,
        limit,
    )
}

/// [`compendium_entries`] over already read compendia.
pub fn page(
    installed: &[(String, Parsed)],
    plugin_id: &str,
    compendium_id: &str,
    query: &str,
    offset: u32,
    limit: u32,
) -> Result<CompendiumPage, IpcError> {
    let compendium = installed
        .iter()
        .filter(|(id, _)| id == plugin_id)
        .flat_map(|(_, compendia)| compendia.iter())
        .find(|c| c.id == compendium_id)
        .ok_or_else(|| IpcError::user("not_found", "no such compendium"))?;
    let query = query.trim().to_lowercase();
    let matching: Vec<_> = compendium
        .entries
        .iter()
        .filter(|e| {
            query.is_empty() || label_texts(&e.name).any(|t| t.to_lowercase().contains(&query))
        })
        .collect();
    let limit = limit.min(MAX_PAGE) as usize;
    Ok(CompendiumPage {
        total: u32::try_from(matching.len()).unwrap_or(u32::MAX),
        entries: matching
            .into_iter()
            .skip(offset as usize)
            .take(limit)
            .map(|e| CompendiumEntryDto {
                id: e.id.clone(),
                name: e.name.clone(),
                data: Value::Object(e.data.clone()),
            })
            .collect(),
    })
}

fn label_texts(label: &Label) -> impl Iterator<Item = &str> {
    let (one, ru, en) = match label {
        Label::Text(text) => (Some(text.as_str()), None, None),
        Label::Localized(l) => (None, l.ru.as_deref(), l.en.as_deref()),
    };
    one.into_iter().chain(ru).chain(en)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use super::*;

    fn srd() -> Vec<(String, Parsed)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../plugins/system-srd5");
        let bytes = dnd_plugin::pack_directory(&dir).unwrap();
        let package = read_package(&bytes, PackageLimits::default()).unwrap();
        vec![("srd5".to_owned(), Arc::new(package.compendia))]
    }

    #[test]
    fn lists_and_searches_in_any_language() {
        let installed = srd();
        let compendia = list(&installed);
        assert!(
            compendia
                .iter()
                .any(|c| c.id == "srd5-spells" && c.count > 0)
        );
        let rus = page(&installed, "srd5", "srd5-spells", "огнен", 0, 50).unwrap();
        let eng = page(&installed, "srd5", "srd5-spells", "FIRE", 0, 50).unwrap();
        assert_eq!(rus.total, 1);
        assert_eq!(eng.entries[0].id, rus.entries[0].id);
        let all = page(&installed, "srd5", "srd5-spells", "", 1, 2).unwrap();
        assert_eq!(
            (all.total as usize, all.entries.len()),
            (installed[0].1[1].entries.len(), 2)
        );
        assert!(page(&installed, "srd5", "nope", "", 0, 10).is_err());
    }
}
