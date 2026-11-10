//! The signed plugin catalog index (ADR-0006, ADR-0060).
//!
//! Reading is deliberately strict: the signature is checked over the exact bytes before anything
//! is parsed, every entry is checked again when its package arrives, and a revoked version is
//! never installable. Nothing here performs I/O — `dnd-net` fetches, this module decides.

use std::collections::BTreeMap;

use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::error::{PluginError, PluginResult};
use crate::model::PluginKind;
use crate::package::{PackageLimits, PluginPackage, read_package};
use crate::signature;

/// Index format this build understands.
pub const CATALOG_FORMAT: u32 = 1;

/// A parsed, signature-checked index.
#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub key_id: String,
    pub generated_at: String,
    pub plugins: Vec<CatalogEntry>,
}

/// One published version.
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogEntry {
    pub id: String,
    pub version: Version,
    pub kind: PluginKind,
    pub license: String,
    pub app_compat: VersionReq,
    pub sha256: String,
    pub archive_url: String,
    pub signature: String,
    /// New installations and updates are refused; an installed copy keeps running.
    pub revoked: bool,
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIndex {
    format: u32,
    key_id: String,
    generated_at: String,
    plugins: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    id: String,
    version: String,
    kind: PluginKind,
    license: String,
    app_compat: String,
    sha256: String,
    archive_url: String,
    signature: String,
    revoked: bool,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

fn invalid(reason: impl Into<String>) -> PluginError {
    PluginError::InvalidDocument {
        file: "index.json".to_owned(),
        reason: reason.into(),
    }
}

/// Checks the index signature over `bytes`, then parses it.
///
/// Ordering matters: parsing before verifying would let an unsigned document influence error
/// messages or memory use.
pub fn read_index(bytes: &[u8], public_key: &[u8], signature_hex: &str) -> PluginResult<Catalog> {
    signature::verify_index(public_key, bytes, signature_hex)?;
    let raw: RawIndex =
        serde_json::from_slice(bytes).map_err(|error| invalid(error.to_string()))?;
    if raw.format != CATALOG_FORMAT {
        return Err(PluginError::UnsupportedFormat {
            format: u64::from(raw.format),
            supported: CATALOG_FORMAT,
        });
    }
    let mut plugins = Vec::with_capacity(raw.plugins.len());
    for entry in raw.plugins {
        let version = Version::parse(&entry.version)
            .map_err(|error| invalid(format!("{}: {error}", entry.id)))?;
        let app_compat = VersionReq::parse(&entry.app_compat)
            .map_err(|error| invalid(format!("{} {}: {error}", entry.id, entry.version)))?;
        if entry.sha256.len() != 64
            || !entry
                .sha256
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(invalid(format!(
                "{} {}: archive_sha256 is not lower-case hex",
                entry.id, entry.version
            )));
        }
        if entry.signature.len() != 128 || !entry.signature.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid(format!(
                "{} {}: signature is not 128 hex characters",
                entry.id, entry.version
            )));
        }
        plugins.push(CatalogEntry {
            id: entry.id,
            version,
            kind: entry.kind,
            license: entry.license,
            app_compat,
            sha256: entry.sha256,
            archive_url: entry.archive_url,
            signature: entry.signature.to_ascii_lowercase(),
            revoked: entry.revoked,
            dependencies: entry.dependencies,
        });
    }
    Ok(Catalog {
        key_id: raw.key_id,
        generated_at: raw.generated_at,
        plugins,
    })
}

impl Catalog {
    /// The newest non-revoked version of `id` that this app satisfies, or the entry to update from
    /// `installed`.
    #[must_use]
    pub fn newest(&self, id: &str, app: &Version) -> Option<&CatalogEntry> {
        self.plugins
            .iter()
            .filter(|entry| entry.id == id && !entry.revoked && entry.app_compat.matches(app))
            .max_by(|a, b| a.version.cmp(&b.version))
    }

    /// The entry a client is allowed to install: it must not be revoked.
    pub fn installable(&self, id: &str, version: &Version) -> PluginResult<&CatalogEntry> {
        let entry = self
            .plugins
            .iter()
            .find(|entry| entry.id == id && entry.version == *version)
            .ok_or_else(|| invalid(format!("{id} {version} is not in the index")))?;
        if entry.revoked {
            return Err(PluginError::InvalidManifest {
                reason: format!("{id} {version} has been revoked"),
            });
        }
        Ok(entry)
    }

    /// Versions that are listed but revoked — for the UI's warning on an installed copy.
    #[must_use]
    pub fn revoked(&self, id: &str) -> Vec<&CatalogEntry> {
        self.plugins
            .iter()
            .filter(|entry| entry.id == id && entry.revoked)
            .collect()
    }
}

impl CatalogEntry {
    /// Verifies the downloaded package against this entry and returns it parsed.
    ///
    /// The order is the point: hash, then signature over id/version/hash, then the package's own
    /// validation. A package that arrives from the catalog is never opened before both checks.
    pub fn verify_package(&self, bytes: &[u8], public_key: &[u8]) -> PluginResult<PluginPackage> {
        use sha2::{Digest as _, Sha256};
        let hash = signature::encode_hex(&Sha256::digest(bytes));
        if hash != self.sha256 {
            return Err(PluginError::HashMismatch {
                name: self.archive_url.clone(),
            });
        }
        signature::verify_plugin(
            public_key,
            &self.id,
            &self.version.to_string(),
            &self.sha256,
            &self.signature,
        )?;
        let package = read_package(bytes, PackageLimits::default())?;
        if package.manifest.id != self.id || package.manifest.version != self.version {
            return Err(PluginError::InvalidManifest {
                reason: format!(
                    "the index lists {} {}, the package says {} {}",
                    self.id, self.version, package.manifest.id, package.manifest.version
                ),
            });
        }
        Ok(package)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair as _};

    fn key() -> (Ed25519KeyPair, Vec<u8>) {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
        let public = pair.public_key().as_ref().to_vec();
        (pair, public)
    }

    fn index_json(version: &str, revoked: bool, signature: &str) -> String {
        format!(
            r#"{{
  "format": 1,
  "key_id": "catalog",
  "generated_at": "2026-11-08T00:00:00Z",
  "plugins": [
    {{
      "id": "demo",
      "version": "{version}",
      "kind": "content",
      "license": "MIT",
      "app_compat": ">=0.1.0, <0.2.0",
      "sha256": "{hash}",
      "archive_url": "https://example.org/demo.dndplugin",
      "signature": "{signature}",
      "revoked": {revoked},
      "dependencies": {{}}
    }}
  ]
}}
"#,
            hash = "a".repeat(64)
        )
    }

    fn signed_index(pair: &Ed25519KeyPair, json: &str) -> String {
        signature::encode_hex(
            pair.sign(&signature::index_transcript(json.as_bytes()))
                .as_ref(),
        )
    }

    #[test]
    fn a_signed_index_parses_and_answers_queries() {
        let (pair, public) = key();
        let json = index_json("1.2.3", false, &"b".repeat(128));
        let signature = signed_index(&pair, &json);
        let catalog = read_index(json.as_bytes(), &public, &signature).unwrap();
        assert_eq!(catalog.key_id, "catalog");
        let app = Version::parse("0.1.5").unwrap();
        assert_eq!(
            catalog.newest("demo", &app).unwrap().version.to_string(),
            "1.2.3"
        );
        assert!(
            catalog
                .installable("demo", &Version::parse("1.2.3").unwrap())
                .is_ok()
        );
        assert!(catalog.revoked("demo").is_empty());

        let missing = Version::parse("9.9.9").unwrap();
        assert!(catalog.installable("demo", &missing).is_err());
        let too_new = Version::parse("0.2.0").unwrap();
        assert!(catalog.newest("demo", &too_new).is_none());
    }

    #[test]
    fn a_revoked_version_is_listed_but_not_installable() {
        let (pair, public) = key();
        let json = index_json("1.2.3", true, &"b".repeat(128));
        let signature = signed_index(&pair, &json);
        let catalog = read_index(json.as_bytes(), &public, &signature).unwrap();
        assert_eq!(catalog.revoked("demo").len(), 1);
        assert!(
            catalog
                .installable("demo", &Version::parse("1.2.3").unwrap())
                .is_err()
        );
        assert!(
            catalog
                .newest("demo", &Version::parse("0.1.5").unwrap())
                .is_none()
        );
    }

    #[test]
    fn an_unsigned_or_tampered_index_never_parses() {
        let (pair, public) = key();
        let json = index_json("1.2.3", false, &"b".repeat(128));
        let signature = signed_index(&pair, &json);
        // Another key.
        let (_, other) = key();
        assert!(read_index(json.as_bytes(), &other, &signature).is_err());
        // Changed bytes.
        let tampered = json.replace("\"MIT\"", "\"GPL\"");
        assert!(read_index(tampered.as_bytes(), &public, &signature).is_err());
        // Missing signature.
        assert!(read_index(json.as_bytes(), &public, "").is_err());
    }

    #[test]
    fn a_future_format_is_refused_with_its_own_error() {
        let (pair, public) = key();
        let json =
            index_json("1.2.3", false, &"b".repeat(128)).replace("\"format\": 1", "\"format\": 2");
        let signature = signed_index(&pair, &json);
        assert!(matches!(
            read_index(json.as_bytes(), &public, &signature),
            Err(PluginError::UnsupportedFormat { format: 2, .. })
        ));
    }

    #[test]
    fn a_package_must_match_the_entry() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/theme-midnight");
        let bytes = crate::pack_directory(&dir).unwrap();
        let package = read_package(&bytes, PackageLimits::default()).unwrap();
        let (pair, public) = key();
        let sha = package.sha256.clone();
        let signature = signature::encode_hex(
            pair.sign(&signature::plugin_transcript(
                "theme-midnight",
                "0.0.1",
                &sha,
            ))
            .as_ref(),
        );
        let entry = CatalogEntry {
            id: "theme-midnight".into(),
            version: Version::parse("0.0.1").unwrap(),
            kind: PluginKind::Theme,
            license: "MIT".into(),
            app_compat: VersionReq::parse(">=0.0.1").unwrap(),
            sha256: sha.clone(),
            archive_url: "https://example.org/theme.dndplugin".into(),
            signature,
            revoked: false,
            dependencies: BTreeMap::new(),
        };
        assert_eq!(entry.verify_package(&bytes, &public).unwrap().sha256, sha);
        // A different package under the same entry fails the hash check.
        let other = crate::pack_directory(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../plugins/system-generic-d6"),
        )
        .unwrap();
        assert!(matches!(
            entry.verify_package(&other, &public),
            Err(PluginError::HashMismatch { .. })
        ));
        // A wrong entry signature fails even for the right bytes.
        let mut wrong = entry.clone();
        wrong.signature = "0".repeat(128);
        assert!(matches!(
            wrong.verify_package(&bytes, &public),
            Err(PluginError::SignatureInvalid { .. })
        ));
    }
}
