//! Plugins of the open campaign (ADR-0023): the desktop reads and checks a `.dndplugin`, keeps the
//! package in the campaign (`assets/plugins/<sha256>.dndplugin`) and installs it through the core,
//! which checks the rules again. Also the values a character sheet shows (computed fields,
//! resources), evaluated by the core's own rules.

use std::collections::BTreeMap;
use std::path::Path;

use dnd_game_core::{GameState, sheet_values};
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
    pub computed: BTreeMap<String, f64>,
    pub resources: Vec<ResourceValue>,
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
