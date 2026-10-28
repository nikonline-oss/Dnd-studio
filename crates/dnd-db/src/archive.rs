//! Campaign archive (CLAUDE.md §6): a ZIP with `campaign.sqlite`, `assets/` (with the campaign's
//! plugin packages in `assets/plugins/`, ADR-0023) and `manifest.json`.
//! Used for export/import and for uploading a campaign to the session server.
//!
//! Reading is defensive: the archive comes from a user's file or over the network (CLAUDE.md §9).
//! Only whitelisted names are extracted (so no path can escape the target folder), sizes are
//! counted on the bytes actually read rather than trusted from headers (zip bombs), and asset files
//! must match the SHA-256 in their names.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{DbError, DbResult};

/// Version of the archive layout written into `manifest.json`.
pub const ARCHIVE_FORMAT: u32 = 1;
/// Name of the database inside an archive.
pub const ARCHIVE_DB: &str = "campaign.sqlite";
const MANIFEST: &str = "manifest.json";

/// `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveManifest {
    pub format: u32,
    /// Campaign name (a suggestion for the folder name on import).
    pub name: String,
    pub exported_at_ms: i64,
    /// Application that wrote it, e.g. `dndstudio 0.1.0`.
    pub generator: String,
}

/// Size limits for reading an archive.
#[derive(Clone, Copy, Debug)]
pub struct ArchiveLimits {
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl ArchiveLimits {
    pub const DEFAULT: Self = Self {
        max_entries: 50_000,
        max_file_bytes: 1024 * 1024 * 1024,
        max_total_bytes: 4 * 1024 * 1024 * 1024,
    };
}

/// What an archive holds (for logs and UI).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveStats {
    pub files: usize,
    pub bytes: u64,
}

/// Writes an archive to `out` from a database copy (see `CampaignDb::copy_to`) and the campaign's
/// assets folder. Originals (`assets/originals/`) are included only when asked: the session server
/// serves derived files only. Written to a temporary file and renamed, so `out` is never partial.
pub fn write_archive(
    out: &Path,
    db_copy: &Path,
    assets_dir: &Path,
    manifest: &ArchiveManifest,
    include_originals: bool,
) -> DbResult<ArchiveStats> {
    let partial = with_suffix(out, ".partial");
    let result = write_to(&partial, db_copy, assets_dir, manifest, include_originals);
    match result {
        Ok(stats) => {
            std::fs::rename(&partial, out)?;
            Ok(stats)
        }
        Err(e) => {
            std::fs::remove_file(&partial).ok();
            Err(e)
        }
    }
}

fn write_to(
    out: &Path,
    db_copy: &Path,
    assets_dir: &Path,
    manifest: &ArchiveManifest,
    include_originals: bool,
) -> DbResult<ArchiveStats> {
    let mut zip = ZipWriter::new(File::create(out)?);
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    // Images are already compressed.
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);
    let mut stats = ArchiveStats::default();

    zip.start_file(MANIFEST, deflated).map_err(zip_error)?;
    let json = serde_json::to_vec_pretty(manifest)?;
    zip.write_all(&json)?;
    stats.add(json.len() as u64);

    zip.start_file(ARCHIVE_DB, deflated).map_err(zip_error)?;
    stats.add(io::copy(&mut File::open(db_copy)?, &mut zip)?);

    let mut files = asset_files(assets_dir, "assets", is_served_name)?;
    // Plugin packages: the campaign's rules came from them (ADR-0023).
    files.extend(asset_files(
        &assets_dir.join("plugins"),
        "assets/plugins",
        is_plugin_name,
    )?);
    if include_originals {
        files.extend(asset_files(
            &assets_dir.join("originals"),
            "assets/originals",
            is_original_name,
        )?);
    }
    for (name, path) in files {
        zip.start_file(name, stored).map_err(zip_error)?;
        stats.add(io::copy(&mut File::open(path)?, &mut zip)?);
    }
    zip.finish().map_err(zip_error)?.sync_all()?;
    Ok(stats)
}

/// Archive names and paths of the files in `dir` whose names pass `keep`, sorted.
fn asset_files(
    dir: &Path,
    prefix: &str,
    keep: fn(&str) -> bool,
) -> DbResult<Vec<(String, PathBuf)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if keep(&name) {
            files.push((format!("{prefix}/{name}"), entry.path()));
        }
    }
    files.sort();
    Ok(files)
}

/// Extracts an archive into `target`, which must not exist yet: `target/campaign.sqlite` and
/// `target/assets/`. On any error `target` is removed again. The database itself is not opened
/// here; the caller opens it (integrity check, migrations).
pub fn read_archive(
    archive: &Path,
    target: &Path,
    limits: ArchiveLimits,
) -> DbResult<(ArchiveManifest, ArchiveStats)> {
    if target.exists() {
        return Err(DbError::AlreadyExists(target.to_owned()));
    }
    std::fs::create_dir_all(target)?;
    let result = extract(archive, target, limits);
    if result.is_err() {
        std::fs::remove_dir_all(target).ok();
    }
    result
}

fn extract(
    archive: &Path,
    target: &Path,
    limits: ArchiveLimits,
) -> DbResult<(ArchiveManifest, ArchiveStats)> {
    let mut zip = ZipArchive::new(File::open(archive)?).map_err(zip_error)?;
    if zip.len() > limits.max_entries {
        return Err(invalid(format!("more than {} files", limits.max_entries)));
    }
    let mut seen = BTreeSet::new();
    let mut manifest = None;
    let mut stats = ArchiveStats::default();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(zip_error)?;
        let name = file.name().map_err(zip_error)?.into_owned();
        if file.is_dir() {
            if matches!(
                name.as_str(),
                "assets/" | "assets/originals/" | "assets/plugins/"
            ) {
                continue;
            }
            return Err(invalid(format!("unexpected folder {name:?}")));
        }
        if file.encrypted() || file.is_symlink() {
            return Err(invalid(format!("{name:?} is encrypted or a link")));
        }
        let kind = classify(&name).ok_or_else(|| invalid(format!("unexpected file {name:?}")))?;
        if !seen.insert(name.clone()) {
            return Err(invalid(format!("{name:?} appears twice")));
        }
        let budget = limits
            .max_file_bytes
            .min(limits.max_total_bytes.saturating_sub(stats.bytes));
        let bytes = read_limited(&mut file, budget).map_err(|e| match e.kind() {
            io::ErrorKind::FileTooLarge => invalid(format!("{name:?} is too large")),
            _ => DbError::Io(e),
        })?;
        stats.add(bytes.len() as u64);
        match kind {
            Entry::Manifest => {
                let parsed: ArchiveManifest = serde_json::from_slice(&bytes)
                    .map_err(|e| invalid(format!("manifest.json: {e}")))?;
                if parsed.format != ARCHIVE_FORMAT {
                    return Err(invalid(format!(
                        "archive format {} is not supported",
                        parsed.format
                    )));
                }
                manifest = Some(parsed);
            }
            Entry::Database => std::fs::write(target.join(ARCHIVE_DB), &bytes)?,
            Entry::Served(sha) | Entry::Original(sha) | Entry::Plugin(sha) => {
                if hex_sha256(&bytes) != sha {
                    return Err(invalid(format!("{name:?} does not match its hash")));
                }
                let path = target.join(&name);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, &bytes)?;
            }
        }
    }
    let manifest = manifest.ok_or_else(|| invalid("manifest.json is missing".to_owned()))?;
    if !seen.contains(ARCHIVE_DB) {
        return Err(invalid(format!("{ARCHIVE_DB} is missing")));
    }
    std::fs::create_dir_all(target.join("assets"))?;
    Ok((manifest, stats))
}

enum Entry {
    Manifest,
    Database,
    /// `assets/<sha256>.webp` or `assets/<sha256>.audio`
    Served(String),
    /// `assets/originals/<sha256>.<png|jpg|jpeg|webp>`
    Original(String),
    /// `assets/plugins/<sha256>.dndplugin`
    Plugin(String),
}

/// The only names an archive may contain. Matching whole names (not sanitizing paths) means no
/// entry can point outside the target folder.
fn classify(name: &str) -> Option<Entry> {
    match name {
        MANIFEST => return Some(Entry::Manifest),
        ARCHIVE_DB => return Some(Entry::Database),
        _ => {}
    }
    if let Some(file) = name.strip_prefix("assets/plugins/") {
        return is_plugin_name(file).then(|| Entry::Plugin(file[..64].to_owned()));
    }
    if let Some(file) = name.strip_prefix("assets/originals/") {
        return is_original_name(file).then(|| Entry::Original(file[..64].to_owned()));
    }
    let file = name.strip_prefix("assets/")?;
    is_served_name(file).then(|| Entry::Served(file[..64].to_owned()))
}

/// Served files: `<sha256>.webp` pictures and `<sha256>.audio` sounds (ADR-0026).
fn is_served_name(file: &str) -> bool {
    file.strip_suffix(".webp")
        .or_else(|| file.strip_suffix(".audio"))
        .is_some_and(is_sha256_hex)
}

fn is_plugin_name(file: &str) -> bool {
    file.strip_suffix(".dndplugin").is_some_and(is_sha256_hex)
}

fn is_original_name(file: &str) -> bool {
    file.rsplit_once('.').is_some_and(|(sha, ext)| {
        is_sha256_hex(sha) && matches!(ext, "png" | "jpg" | "jpeg" | "webp")
    })
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reads at most `limit` bytes; more is an error (`FileTooLarge`), whatever the header claimed.
fn read_limited(reader: &mut impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::from(io::ErrorKind::FileTooLarge));
    }
    Ok(bytes)
}

impl ArchiveStats {
    fn add(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn invalid(reason: String) -> DbError {
    DbError::InvalidArchive { reason }
}

fn zip_error(e: zip::result::ZipError) -> DbError {
    match e {
        zip::result::ZipError::Io(e) => DbError::Io(e),
        other => invalid(other.to_string()),
    }
}
