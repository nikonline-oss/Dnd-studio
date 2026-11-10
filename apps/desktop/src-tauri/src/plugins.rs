//! Plugins of the open campaign (ADR-0023): the desktop reads and checks a `.dndplugin`, keeps the
//! package in the campaign (`assets/plugins/<sha256>.dndplugin`) and installs it through the core,
//! which checks the rules again. Also the values a character sheet shows (computed fields,
//! resources), evaluated by the core's own rules.

use std::collections::BTreeMap;
use std::path::Path;

use dnd_game_core::{GameState, public_sheet_values, sheet_values};
use dnd_plugin::semver::Version;
use dnd_plugin::{PackageLimits, PluginError, PluginPackage, check_app_compat, read_package};
use dnd_protocol::{
    EventBatch, Intent, IntentEnvelope, IntentId, IntentOutcome, Label, PluginKind,
};
use serde::{Deserialize, Serialize};

use crate::error::IpcError;
use crate::local_server::{ASSETS_DIR, LocalServer};

impl From<PluginError> for IpcError {
    fn from(e: PluginError) -> Self {
        let message = e.to_string();
        match e {
            PluginError::Io(_) | PluginError::Internal { .. } => {
                tracing::error!(error = %message, "plugin error");
                Self::internal(e.code(), message)
            }
            other => Self::user(other.code(), message),
        }
    }
}

/// A plugin shipped with the app.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct BundledPlugin {
    pub id: String,
    pub name: Label,
    pub version: String,
    pub kind: PluginKind,
    pub license: String,
}

/// The plugins shipped with the app.
#[must_use]
pub fn bundled() -> Vec<BundledPlugin> {
    crate::bundled::BUNDLED
        .iter()
        .filter_map(|(_, bytes)| read_package(bytes, PackageLimits::default()).ok())
        .map(|p| BundledPlugin {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.to_string(),
            kind: p.manifest.kind,
            license: p.manifest.license.clone(),
        })
        .collect()
}

/// Checks a package against the app and the campaign's plugins, keeps it in the campaign and
/// installs it through the core. The caller broadcasts the batch.
pub async fn install(
    server: &mut LocalServer,
    bytes: &[u8],
) -> Result<(IntentOutcome, Option<EventBatch>), IpcError> {
    let package = prepare(bytes)?;
    check_dependencies(server.state(), &package.manifest)?;
    keep_package(
        &server.folder().join(ASSETS_DIR).join("plugins"),
        &package.sha256,
        bytes,
    )
    .await?;
    let envelope = IntentEnvelope {
        intent_id: IntentId::new_v7(),
        expected_version: None,
        ts_ms: 0,
        intent: Intent::InstallPlugin {
            plugin: Box::new(package.to_dto()),
        },
    };
    let result = server.submit(&envelope).await?;
    tracing::info!(id = %package.manifest.id, version = %package.manifest.version, "plugin installed");
    Ok(result)
}

pub(crate) fn prepare(bytes: &[u8]) -> Result<PluginPackage, IpcError> {
    let package = read_package(bytes, PackageLimits::default())?;
    let app = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| IpcError::internal("app_version", e.to_string()))?;
    check_app_compat(&package.manifest, &app)?;
    Ok(package)
}

pub(crate) async fn keep_package(dir: &Path, sha: &str, bytes: &[u8]) -> Result<(), IpcError> {
    tokio::fs::create_dir_all(dir).await?;
    let path = dir.join(format!("{sha}.dndplugin"));
    if tokio::fs::metadata(&path)
        .await
        .is_ok_and(|m| m.len() <= PackageLimits::DEFAULT.max_package_bytes)
        && let Ok(existing) = tokio::fs::read(&path).await
        && dnd_assets::sha256_hex(&existing) == sha
    {
        return Ok(());
    }
    let partial = dir.join(format!("{sha}.{}.partial", uuid::Uuid::now_v7()));
    let result: Result<(), std::io::Error> = async {
        tokio::fs::write(&partial, bytes).await?;
        tokio::fs::rename(&partial, path).await
    }
    .await;
    if result.is_err() {
        tokio::fs::remove_file(partial).await.ok();
    }
    result?;
    Ok(())
}

/// Every dependency is installed in a matching version.
pub(crate) fn check_dependencies(
    state: &GameState,
    manifest: &dnd_plugin::Manifest,
) -> Result<(), IpcError> {
    dnd_plugin::check_dependency_versions(
        manifest,
        state
            .plugins
            .values()
            .map(|p| (p.id.as_str(), p.version.as_str())),
    )?;
    Ok(())
}

/// What a sheet shows besides its data (numbers as JSON numbers for the UI).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SheetView {
    #[serde(default)]
    pub economy: Option<dnd_protocol::TurnEconomyView>,
    #[serde(default)]
    pub modifiers: Vec<Label>,
    pub computed: BTreeMap<String, f64>,
    pub resources: Vec<ResourceValue>,
    /// Integer fields after adjustments from conditions and items (ADR-0062), by dotted path.
    /// A field missing here keeps its stored value.
    #[serde(default)]
    pub adjusted: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ResourceValue {
    pub id: String,
    pub value: f64,
    pub max: f64,
}

/// [`SheetView`] of a sheet under the campaign's system; `None` without one.
#[must_use]
#[allow(clippy::cast_precision_loss)] // sheet integers are JS-safe (ADR-0016)
pub fn sheet_view(state: &GameState, sheet_json: &str) -> Option<SheetView> {
    let values = sheet_values(state, sheet_json)?;
    Some(SheetView {
        economy: None,
        modifiers: Vec::new(),
        adjusted: BTreeMap::new(),
        computed: values
            .computed
            .into_iter()
            .map(|(k, v)| (k, v as f64))
            .collect(),
        resources: values
            .resources
            .into_iter()
            .map(|(id, value, max)| ResourceValue {
                id,
                value: value as f64,
                max: max as f64,
            })
            .collect(),
    })
}

/// What the caller knows about the sheet it asks for (ADR-0054).
///
/// `Full` means the state behind it is authoritative for this character — local play, or a
/// session where the recipient is a GM or the owner. `Partial` means the state is this client's
/// filtered view: values may be derived from what it legitimately sees, but authority-only
/// overrides (conditions, maintained effects, the turn budget) are never mixed in, because the
/// view is incomplete by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetAuthority {
    Full,
    Partial,
}

/// A sheet's derived values; edits still retain the unmodified base sheet.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn sheet_view_for_character(
    state: &GameState,
    sheet_json: &str,
    id: Option<dnd_protocol::CharacterId>,
    authority: SheetAuthority,
) -> Option<SheetView> {
    let mut view = sheet_view(state, sheet_json)?;
    if authority == SheetAuthority::Partial {
        // A filtered state sees only the public part of the sheet; computing from the full
        // schema would fill hidden fields with defaults and hand out invented numbers
        // (ADR-0049, ADR-0062).
        if let Some(public) = public_sheet_values(state, sheet_json) {
            view.computed = public
                .computed
                .into_iter()
                .map(|(k, v)| (k, v as f64))
                .collect();
            view.resources = public
                .resources
                .into_iter()
                .map(|(id, value, max)| ResourceValue {
                    id,
                    value: value as f64,
                    max: max as f64,
                })
                .collect();
        } else {
            view.computed.clear();
            view.resources.clear();
        }
        return Some(view);
    }
    if let Some(mut character) = id.and_then(|id| state.characters.get(&id)).cloned() {
        character.sheet_json = sheet_json.to_owned();
        let fields = dnd_game_core::effective_fields(state, &character).ok()?;
        view.computed = fields
            .computed()
            .iter()
            .map(|(k, v)| (k.clone(), *v as f64))
            .collect();
        view.modifiers = dnd_game_core::active_modifier_labels(state, &character);
        let mut adjusted = BTreeMap::new();
        if integer_leaves(fields.adjusted_sheet(), "", &mut adjusted).is_some() {
            view.adjusted = adjusted;
        }
        view.economy = dnd_game_core::turn_economy_view(state, &character);
    }
    Some(view)
}

/// Dotted paths of every integer leaf of an adjusted sheet.
#[allow(clippy::cast_precision_loss)] // sheet integers are JS-safe (ADR-0016)
fn integer_leaves(
    value: &serde_json::Value,
    prefix: &str,
    out: &mut BTreeMap<String, f64>,
) -> Option<()> {
    match value {
        serde_json::Value::Number(number) => {
            out.insert(prefix.to_owned(), number.as_i64()? as f64);
        }
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                integer_leaves(child, &path, out)?;
            }
        }
        serde_json::Value::Array(items) => {
            // Lists are not adjustable by conditions or items; skip rather than fail.
            for (index, child) in items.iter().enumerate() {
                integer_leaves(child, &format!("{prefix}.{index}"), out)?;
            }
        }
        _ => {}
    }
    Some(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use dnd_protocol::{CharacterDto, CharacterKind, CharacterStatus, Version};

    fn state_with_srd() -> (GameState, dnd_protocol::CharacterId) {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../plugins/system-srd5");
        let package = dnd_plugin::read_package(
            &dnd_plugin::pack_directory(&dir).unwrap(),
            dnd_plugin::PackageLimits::default(),
        )
        .unwrap()
        .to_dto();
        let mut state = GameState::new("Boundary");
        state.system_plugin = Some(package.id.clone());
        state.plugins.insert(package.id.clone(), package);
        let id = dnd_protocol::CharacterId::new_v7();
        let sheet = dnd_game_core::default_sheet(state.rules().unwrap()).to_string();
        state.characters.insert(
            id,
            CharacterDto {
                id,
                kind: CharacterKind::Pc,
                name: "Hero".into(),
                owner: None,
                status: CharacterStatus::Active,
                sheet_json: sheet,
                conditions: vec!["prone".into()],
                version: Version::INITIAL,
            },
        );
        (state, id)
    }

    /// A partial view derives only from what its recipient may see (ADR-0049, ADR-0054): no
    /// authority-only overrides, and no numbers invented from hidden fields — the full schema's
    /// defaults would hand a player 10/10 hit points.
    #[test]
    fn a_partial_view_carries_no_authority_only_values() {
        let (state, id) = state_with_srd();
        let sheet = state.characters[&id].sheet_json.clone();
        let partial =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Partial).unwrap();
        assert!(partial.modifiers.is_empty());
        assert!(partial.economy.is_none());
        assert!(partial.adjusted.is_empty());
        // Hit points are private in the SRD schema: no resource, not even a default one.
        assert!(!partial.resources.iter().any(|r| r.id == "hp"));
        // What the owner gets is untouched.
        let full =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Full).unwrap();
        assert!(full.resources.iter().any(|r| r.id == "hp"));
    }

    #[test]
    fn a_full_view_may_apply_condition_aware_values() {
        let (state, id) = state_with_srd();
        let sheet = state.characters[&id].sheet_json.clone();
        let full =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Full).unwrap();
        // Resources always come from the sheet; the computed block is evaluated by the authority.
        assert_eq!(
            full.resources.len(),
            sheet_view(&state, &sheet).unwrap().resources.len()
        );
        assert!(!full.computed.is_empty());
        assert!(
            !full.modifiers.is_empty(),
            "the prone condition adds a modifier label under SRD rules"
        );
    }

    /// ADR-0062: the sheet view carries the adjusted numbers, so the client can show "10 → 12"
    /// next to the stored value without recomputing anything.
    #[test]
    fn a_full_view_reports_adjusted_integer_fields() {
        use dnd_protocol::{ItemDto, ItemEffect, ItemId, ItemKind};
        let (mut state, id) = state_with_srd();
        let item = ItemId::new_v7();
        state.items.insert(
            item,
            ItemDto {
                id: item,
                kind: ItemKind::Item,
                name: "Ring".into(),
                type_id: None,
                is_template: false,
                template: None,
                owner: Some(id),
                visible_to_players: false,
                description: String::new(),
                state: None,
                effects: vec![ItemEffect {
                    path: "ac".into(),
                    add: 2,
                    minimum: None,
                    maximum: None,
                }],
                grants: Vec::new(),
                values: Vec::new(),
                sheet_json: "{}".into(),
                version: Version::INITIAL,
            },
        );
        let sheet = state.characters[&id].sheet_json.clone();
        let stored: serde_json::Value = serde_json::from_str(&sheet).unwrap();
        let stored_ac = stored["ac"].as_i64().unwrap();
        let full =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Full).unwrap();
        assert_eq!(full.adjusted.get("ac"), Some(&((stored_ac + 2) as f64)));
        // The label says where the number comes from.
        assert!(
            full.modifiers
                .iter()
                .any(|label| matches!(label, dnd_protocol::Label::Text(name) if name == "Ring"))
        );

        // A broken item changes nothing, and a partial view never sees the adjustment.
        state.items.get_mut(&item).unwrap().state = Some("broken".into());
        let full =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Full).unwrap();
        assert_eq!(full.adjusted.get("ac"), Some(&(stored_ac as f64)));
        let partial =
            sheet_view_for_character(&state, &sheet, Some(id), SheetAuthority::Partial).unwrap();
        assert!(partial.adjusted.is_empty());
    }

    #[test]
    fn without_a_system_plugin_no_view_is_offered() {
        let state = GameState::new("Plain");
        assert!(sheet_view_for_character(&state, "{}", None, SheetAuthority::Full).is_none());
        assert!(sheet_view_for_character(&state, "{}", None, SheetAuthority::Partial).is_none());
    }
}
