//! App compatibility and dependency resolution (CLAUDE.md §8: incompatible plugins are deactivated
//! with a notice). Version ranges follow Cargo's semver flavour (`semver` crate): `^0.1` means
//! `>=0.1.0, <0.2.0`, a bare `1.2` means `^1.2`.

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};

use crate::error::{PluginError, PluginResult};
use crate::model::{Manifest, PluginKind};

/// Checks `manifest.app_compat` against the running app.
///
/// Pre-release and build parts of the *app* version are ignored (`0.1.0-beta.2` counts as
/// `0.1.0`), so a beta build loads the plugins written for its release.
pub fn check_app_compat(manifest: &Manifest, app: &Version) -> PluginResult<()> {
    let release = Version::new(app.major, app.minor, app.patch);
    if manifest.app_compat.matches(&release) {
        Ok(())
    } else {
        Err(PluginError::IncompatibleApp {
            id: manifest.id.clone(),
            required: manifest.app_compat.to_string(),
            app: app.to_string(),
        })
    }
}

/// Checks whether `candidate` can be installed next to the `installed` (active) plugins: it fits
/// the app, its dependencies are present in matching versions, and it is not a second rule system.
/// A plugin with the same id among `installed` is the one being updated and is ignored.
///
/// Updating a plugin can break plugins that depend on it; run [`resolve`] on the resulting set
/// to find out which ones get deactivated.
pub fn check_install(
    candidate: &Manifest,
    installed: &[&Manifest],
    app: &Version,
) -> PluginResult<()> {
    check_app_compat(candidate, app)?;
    let others: BTreeMap<&str, &Manifest> = installed
        .iter()
        .filter(|m| m.id != candidate.id)
        .map(|m| (m.id.as_str(), *m))
        .collect();
    if candidate.kind == PluginKind::System
        && let Some(existing) = others.values().find(|m| m.kind == PluginKind::System)
    {
        return Err(PluginError::MultipleSystems {
            id: candidate.id.clone(),
            existing: existing.id.clone(),
        });
    }
    for (dependency, required) in &candidate.dependencies {
        check_dependency(
            candidate,
            dependency,
            required,
            others.get(dependency.as_str()),
        )?;
    }
    Ok(())
}

fn check_dependency(
    plugin: &Manifest,
    dependency: &str,
    required: &VersionReq,
    found: Option<&&Manifest>,
) -> PluginResult<()> {
    let Some(found) = found else {
        return Err(PluginError::MissingDependency {
            id: plugin.id.clone(),
            dependency: dependency.to_owned(),
            required: required.to_string(),
        });
    };
    if required.matches(&found.version) {
        Ok(())
    } else {
        Err(PluginError::DependencyVersionMismatch {
            id: plugin.id.clone(),
            dependency: dependency.to_owned(),
            required: required.to_string(),
            found: found.version.to_string(),
        })
    }
}

/// Which of a campaign's plugins can be active.
#[derive(Debug, Default)]
pub struct Resolution {
    /// Ids of the plugins that stay active, in input order.
    pub active: Vec<String>,
    /// Plugins that must be deactivated, each with its first reason, in input order.
    pub deactivated: Vec<Deactivated>,
}

#[derive(Debug)]
pub struct Deactivated {
    pub id: String,
    pub reason: PluginError,
}

/// Resolves the plugins a campaign wants active: drops those that do not fit the app, whose
/// dependencies are missing or in a wrong version, and then — until nothing changes — those whose
/// dependencies were dropped. Dependency cycles are allowed (all members stay if each is fine).
/// A repeated id is dropped (`duplicate_plugin`), the first occurrence stays.
///
/// "Exactly one system plugin" is a campaign invariant kept at install time ([`check_install`]),
/// not decided here.
pub fn resolve<'a>(plugins: impl IntoIterator<Item = &'a Manifest>, app: &Version) -> Resolution {
    let mut order: Vec<&Manifest> = Vec::new();
    let mut reasons: BTreeMap<usize, PluginError> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for manifest in plugins {
        let index = order.len();
        order.push(manifest);
        if !ids.insert(manifest.id.as_str()) {
            reasons.insert(
                index,
                PluginError::DuplicatePlugin {
                    id: manifest.id.clone(),
                },
            );
        }
    }
    let by_id: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .filter(|(i, _)| !reasons.contains_key(i))
        .map(|(i, m)| (m.id.as_str(), i))
        .collect();

    for (i, manifest) in order.iter().enumerate() {
        if reasons.contains_key(&i) {
            continue;
        }
        let problem = check_app_compat(manifest, app).err().or_else(|| {
            manifest
                .dependencies
                .iter()
                .find_map(|(dependency, required)| {
                    let found = by_id.get(dependency.as_str()).map(|&j| &order[j]);
                    check_dependency(manifest, dependency, required, found).err()
                })
        });
        if let Some(reason) = problem {
            reasons.insert(i, reason);
        }
    }

    // Cascade: a plugin whose dependency got deactivated is deactivated too.
    loop {
        let mut changed = false;
        for (i, manifest) in order.iter().enumerate() {
            if reasons.contains_key(&i) {
                continue;
            }
            let inactive = manifest.dependencies.keys().find(|dependency| {
                by_id
                    .get(dependency.as_str())
                    .is_some_and(|j| reasons.contains_key(j))
            });
            if let Some(dependency) = inactive {
                reasons.insert(
                    i,
                    PluginError::DependencyInactive {
                        id: manifest.id.clone(),
                        dependency: dependency.clone(),
                    },
                );
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut resolution = Resolution::default();
    for (i, manifest) in order.iter().enumerate() {
        match reasons.remove(&i) {
            Some(reason) => resolution.deactivated.push(Deactivated {
                id: manifest.id.clone(),
                reason,
            }),
            None => resolution.active.push(manifest.id.clone()),
        }
    }
    resolution
}
