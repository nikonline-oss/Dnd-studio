//! Fetching and verifying the signed plugin catalog (ADR-0006, ADR-0060).
//!
//! The network part is deliberately thin: fetch `index.json` and its detached `index.sig`, then
//! hand both to `dnd_plugin::catalog`, which checks the signature *before* parsing. Packages are
//! cached by their content hash and re-verified on every use, so a corrupted cache entry is
//! replaced rather than installed.

use std::path::{Path, PathBuf};

use dnd_plugin::catalog::{Catalog, CatalogEntry};

use crate::error::NetError;

/// Reads the catalog over HTTP from a base URL (the directory holding `index.json`).
#[derive(Clone, Debug)]
pub struct CatalogClient {
    http: reqwest::Client,
    base: String,
    public_key: Vec<u8>,
}

impl CatalogClient {
    /// `base` is the directory URL, e.g. `https://example.org/plugins`; `public_key` is the
    /// catalog's 32-byte Ed25519 key baked into the application.
    pub fn new(base: &str, public_key: Vec<u8>) -> Result<Self, NetError> {
        crate::http::tls_ready();
        let base = base.trim().trim_end_matches('/').to_owned();
        if base.is_empty() || base.contains(char::is_whitespace) {
            return Err(NetError::InvalidAddress(base));
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("dndstudio/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|error| NetError::Transport(error.to_string()))?;
        Ok(Self {
            http,
            base,
            public_key,
        })
    }

    /// Downloads and verifies the index. A wrong key, a tampered file or a newer format is an
    /// error, never a partially trusted index.
    pub async fn fetch(&self) -> Result<Catalog, NetError> {
        let index = self.get(&format!("{}/index.json", self.base)).await?;
        let signature = self.get(&format!("{}/index.sig", self.base)).await?;
        let signature = String::from_utf8(signature)
            .map_err(|_| NetError::Protocol("index.sig is not text".to_owned()))?;
        Ok(dnd_plugin::catalog::read_index(
            &index,
            &self.public_key,
            signature.trim(),
        )?)
    }

    /// The verified archive bytes for `entry`, from the cache when they verify there, otherwise
    /// downloaded. Bytes are returned (not a parsed package) because installation must receive the
    /// exact archive it can store and serve, not a re-encoding of it.
    ///
    /// The cache key includes the content hash, so two versions of one plugin never share a file.
    pub async fn package_bytes(
        &self,
        entry: &CatalogEntry,
        cache_dir: &Path,
    ) -> Result<Vec<u8>, NetError> {
        let path = cached_path(entry, cache_dir);
        if let Ok(bytes) = tokio::fs::read(&path).await
            && entry.verify_package(&bytes, &self.public_key).is_ok()
        {
            tracing::debug!(id = %entry.id, version = %entry.version, "catalog package from cache");
            return Ok(bytes);
        }
        let bytes = self.get(&entry.archive_url).await?;
        entry.verify_package(&bytes, &self.public_key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| NetError::Transport(error.to_string()))?;
        }
        // Write via a temporary name: a crash must not leave a half file that later verifies as
        // "present but corrupt".
        let partial = path.with_extension("partial");
        tokio::fs::write(&partial, &bytes)
            .await
            .map_err(|error| NetError::Transport(error.to_string()))?;
        tokio::fs::rename(&partial, &path)
            .await
            .map_err(|error| NetError::Transport(error.to_string()))?;
        tracing::info!(id = %entry.id, version = %entry.version, "catalog package downloaded");
        Ok(bytes)
    }

    async fn get(&self, url: &str) -> Result<Vec<u8>, NetError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| NetError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(NetError::Http {
                status: response.status().as_u16(),
                code: "catalog".to_owned(),
                message: url.to_owned(),
            });
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| NetError::Transport(error.to_string()))
    }
}

/// Cache file for one package: `<id>-<version>-<sha256 prefix>.dndplugin`.
#[must_use]
pub fn cached_path(entry: &CatalogEntry, cache_dir: &Path) -> PathBuf {
    let prefix: String = entry.sha256.chars().take(16).collect();
    cache_dir.join(format!("{}-{}-{prefix}.dndplugin", entry.id, entry.version))
}
