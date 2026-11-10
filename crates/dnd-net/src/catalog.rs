//! Fetching and verifying the signed plugin catalog (ADR-0006, ADR-0060).
//!
//! The network part is deliberately thin: fetch `index.json` and its detached `index.sig`, then
//! hand both to `dnd_plugin::catalog`, which checks the signature *before* parsing. Packages are
//! cached by their content hash and re-verified on every use, so a corrupted cache entry is
//! replaced rather than installed.

use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt as _;

use dnd_plugin::catalog::{Catalog, CatalogEntry};

use crate::error::NetError;

const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;
const MAX_PACKAGE_BYTES: u64 = dnd_plugin::PackageLimits::DEFAULT.max_package_bytes;

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
        let index = self
            .get(&format!("{}/index.json", self.base), MAX_INDEX_BYTES)
            .await?;
        let signature = self
            .get(&format!("{}/index.sig", self.base), MAX_SIGNATURE_BYTES)
            .await?;
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
        if let Some(bytes) = read_cache(&path, MAX_PACKAGE_BYTES).await
            && entry.verify_package(&bytes, &self.public_key).is_ok()
        {
            tracing::debug!(id = %entry.id, version = %entry.version, "catalog package from cache");
            return Ok(bytes);
        }
        let bytes = self.get(&entry.archive_url, MAX_PACKAGE_BYTES).await?;
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

    async fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
        let mut response = self
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
        if response.content_length().is_some_and(|size| size > limit) {
            return Err(NetError::DownloadTooLarge { limit });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?
        {
            if chunk.len() as u64 > limit.saturating_sub(bytes.len() as u64) {
                return Err(NetError::DownloadTooLarge { limit });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

// Bound actual reads as well as metadata: a growing or corrupted cache is never trusted.
async fn read_cache(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = tokio::fs::File::open(path).await.ok()?;
    let metadata = file.metadata().await.ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).await.ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cache_reads_accept_the_limit_and_refuse_larger_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package");
        tokio::fs::write(&path, b"1234").await.unwrap();
        assert_eq!(read_cache(&path, 4).await, Some(b"1234".to_vec()));
        assert_eq!(read_cache(&path, 3).await, None);
    }
}

/// Cache file for one package: `<id>-<version>-<sha256 prefix>.dndplugin`.
#[must_use]
pub fn cached_path(entry: &CatalogEntry, cache_dir: &Path) -> PathBuf {
    let prefix: String = entry.sha256.chars().take(16).collect();
    cache_dir.join(format!("{}-{}-{prefix}.dndplugin", entry.id, entry.version))
}
