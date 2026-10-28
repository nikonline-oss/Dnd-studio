//! Reading a `.dndplugin` package (a ZIP) defensively (CLAUDE.md §9).
//!
//! The package comes from a user's file or, later, from the catalog. Same approach as campaign
//! archives (`dnd-db/src/archive.rs`): only whitelisted names are accepted (whole-name matching,
//! so no entry can point outside the package), sizes are counted on the bytes actually read rather
//! than trusted from headers (zip bombs), assets must match the SHA-256 in their names, encrypted
//! entries and symbolic links are refused. Nothing is written to disk here.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::error::{PluginError, PluginResult};
use crate::meta::MetaSchema;
use crate::model::{Compendium, LayoutNode, Manifest, PLUGIN_FORMAT, PluginKind, SystemRules};
use crate::sheet;
use dnd_protocol::{MAX_TRIGGER_TEMPLATES, TriggerTemplateDto};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const SYSTEM_FILE: &str = "system.json";
const SHEET_DIR: &str = "sheet/";
const COMPENDIA_DIR: &str = "compendia/";
const TRIGGERS_DIR: &str = "triggers/";
const ASSETS_DIR: &str = "assets/";
/// Longest file name (without the folder) of a document in `sheet/` or `compendia/`.
const MAX_DOC_NAME: usize = 64;

/// Size limits for reading a package. All counted on bytes actually read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageLimits {
    /// The `.dndplugin` file itself (compressed).
    pub max_package_bytes: u64,
    pub max_entries: usize,
    /// One JSON document, uncompressed.
    pub max_json_bytes: u64,
    /// One asset, uncompressed.
    pub max_asset_bytes: u64,
    /// Everything, uncompressed.
    pub max_total_bytes: u64,
}

impl PackageLimits {
    pub const DEFAULT: Self = Self {
        max_package_bytes: 256 * 1024 * 1024,
        max_entries: 5_000,
        max_json_bytes: 16 * 1024 * 1024,
        max_asset_bytes: 32 * 1024 * 1024,
        max_total_bytes: 512 * 1024 * 1024,
    };
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A package that passed every check that does not depend on the environment (app version and
/// other installed plugins are checked by [`crate::check_install`] and [`crate::resolve`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PluginPackage {
    /// Lowercase hex SHA-256 of the whole `.dndplugin` file: the package's identity in a campaign.
    pub sha256: String,
    pub manifest: Manifest,
    /// Present exactly when `manifest.kind` is `system`.
    pub system: Option<SystemPackage>,
    /// Sorted by file name.
    pub compendia: Vec<Compendium>,
    /// Trigger templates (ADR-0024), sorted by file name.
    pub trigger_templates: Vec<TriggerTemplateDto>,
    /// Sorted by hash.
    pub assets: Vec<PackageAsset>,
}

/// The rules of a system plugin with its sheet files loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemPackage {
    pub rules: SystemRules,
    /// The data schema exactly as in the package (JSON Schema subset).
    pub sheet_schema: Value,
    pub sheet_layout: LayoutNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageAsset {
    /// Lowercase hex SHA-256 of `bytes`, as in the file name.
    pub sha256: String,
    pub format: AssetFormat,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetFormat {
    Webp,
    Png,
}

impl AssetFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Png => "png",
        }
    }

    fn matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            Self::Webp => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        }
    }
}

/// Reads a package from a file, refusing files over `max_package_bytes` before reading them.
/// Returns the file's bytes too: the caller stores exactly those (addressed by `sha256`).
pub fn read_package_file(
    path: &Path,
    limits: PackageLimits,
) -> PluginResult<(PluginPackage, Vec<u8>)> {
    let file = File::open(path)?;
    if file.metadata()?.len() > limits.max_package_bytes {
        return Err(too_large("the package", limits.max_package_bytes));
    }
    let bytes = read_limited(file, limits.max_package_bytes)
        .map_err(|e| io_or_too_large(e, "the package", limits.max_package_bytes))?;
    let package = read_package(&bytes, limits)?;
    Ok((package, bytes))
}

/// Reads and validates a package from its bytes.
pub fn read_package(bytes: &[u8], limits: PackageLimits) -> PluginResult<PluginPackage> {
    if bytes.len() as u64 > limits.max_package_bytes {
        return Err(too_large("the package", limits.max_package_bytes));
    }
    let sha256 = hex_sha256(bytes);
    let files = extract(bytes, limits)?;
    assemble(sha256, files)
}

/// Raw contents of the whitelisted entries.
#[derive(Default)]
struct Files {
    manifest: Option<Vec<u8>>,
    system: Option<Vec<u8>>,
    /// Full name (`sheet/x.json`) → bytes.
    sheets: BTreeMap<String, Vec<u8>>,
    compendia: BTreeMap<String, Vec<u8>>,
    triggers: BTreeMap<String, Vec<u8>>,
    assets: BTreeMap<String, PackageAsset>,
}

enum Entry {
    Manifest,
    System,
    Sheet,
    Compendium,
    TriggerTemplate,
    Asset { sha256: String, format: AssetFormat },
}

fn extract(bytes: &[u8], limits: PackageLimits) -> PluginResult<Files> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if zip.len() > limits.max_entries {
        return Err(too_large("the number of files", limits.max_entries as u64));
    }
    // Duplicate names (the zip crate would silently keep one of them), local headers that
    // disagree with the central directory, overlapping entries: anything other readers could
    // interpret differently.
    zip.validate_header_consistency().map_err(zip_error)?;
    let mut files = Files::default();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(zip_error)?;
        let name = file.name().map_err(zip_error)?.into_owned();
        if file.is_dir() {
            if matches!(
                name.as_str(),
                SHEET_DIR | COMPENDIA_DIR | TRIGGERS_DIR | ASSETS_DIR
            ) {
                continue;
            }
            return Err(PluginError::UnexpectedFile { name });
        }
        if file.encrypted() {
            return Err(unsafe_entry(name, "encrypted"));
        }
        if file.is_symlink() {
            return Err(unsafe_entry(name, "a symbolic link"));
        }
        let entry =
            classify(&name).ok_or_else(|| PluginError::UnexpectedFile { name: name.clone() })?;
        let per_file = match entry {
            Entry::Asset { .. } => limits.max_asset_bytes,
            _ => limits.max_json_bytes,
        };
        let remaining = limits.max_total_bytes.saturating_sub(total);
        let (limit, what) = if per_file <= remaining {
            (per_file, name.clone())
        } else {
            (remaining, "the unpacked package".to_owned())
        };
        let content = read_limited(&mut file, limit).map_err(|e| match e.kind() {
            io::ErrorKind::FileTooLarge => too_large(&what, limit),
            // Corrupt data or a CRC mismatch.
            _ => PluginError::InvalidArchive {
                reason: format!("{name}: {e}"),
            },
        })?;
        total += content.len() as u64;
        match entry {
            Entry::Manifest => files.manifest = Some(content),
            Entry::System => files.system = Some(content),
            Entry::Sheet => {
                files.sheets.insert(name, content);
            }
            Entry::Compendium => {
                files.compendia.insert(name, content);
            }
            Entry::TriggerTemplate => {
                files.triggers.insert(name, content);
            }
            Entry::Asset { sha256, format } => {
                if hex_sha256(&content) != sha256 {
                    return Err(PluginError::HashMismatch { name });
                }
                if !format.matches(&content) {
                    return Err(PluginError::InvalidAsset {
                        name,
                        format: format.extension().to_owned(),
                    });
                }
                files.assets.insert(
                    sha256.clone(),
                    PackageAsset {
                        sha256,
                        format,
                        bytes: content,
                    },
                );
            }
        }
    }
    Ok(files)
}

/// Whether a package may contain this name (see [`classify`]).
pub(crate) fn is_package_name(name: &str) -> bool {
    classify(name).is_some()
}

/// The only names a package may contain.
fn classify(name: &str) -> Option<Entry> {
    match name {
        MANIFEST_FILE => return Some(Entry::Manifest),
        SYSTEM_FILE => return Some(Entry::System),
        _ => {}
    }
    if let Some(file) = name.strip_prefix(SHEET_DIR) {
        return is_doc_name(file).then_some(Entry::Sheet);
    }
    if let Some(file) = name.strip_prefix(COMPENDIA_DIR) {
        return is_doc_name(file).then_some(Entry::Compendium);
    }
    if let Some(file) = name.strip_prefix(TRIGGERS_DIR) {
        return is_doc_name(file).then_some(Entry::TriggerTemplate);
    }
    let file = name.strip_prefix(ASSETS_DIR)?;
    let (sha256, ext) = file.split_once('.')?;
    let format = match ext {
        "webp" => AssetFormat::Webp,
        "png" => AssetFormat::Png,
        _ => return None,
    };
    is_sha256_hex(sha256).then(|| Entry::Asset {
        sha256: sha256.to_owned(),
        format,
    })
}

/// `[a-z0-9][a-z0-9._-]*\.json`, no folders.
fn is_doc_name(file: &str) -> bool {
    file.len() <= MAX_DOC_NAME
        && file.strip_suffix(".json").is_some_and(|stem| {
            stem.bytes()
                .next()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && stem.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
                })
        })
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn assemble(sha256: String, mut files: Files) -> PluginResult<PluginPackage> {
    let manifest_bytes = files
        .manifest
        .take()
        .ok_or_else(|| PluginError::MissingFile {
            name: MANIFEST_FILE.to_owned(),
        })?;
    let manifest = parse_manifest(&manifest_bytes)?;

    let system = match (manifest.kind, files.system.take()) {
        (PluginKind::System, Some(bytes)) => Some(parse_system(&bytes, &mut files.sheets)?),
        (PluginKind::System, None) => {
            return Err(PluginError::MissingFile {
                name: SYSTEM_FILE.to_owned(),
            });
        }
        (_, Some(_)) => {
            return Err(PluginError::UnexpectedFile {
                name: SYSTEM_FILE.to_owned(),
            });
        }
        (_, None) => None,
    };
    // Sheet files are only those system.json references.
    if let Some(name) = files.sheets.into_keys().next() {
        return Err(PluginError::UnexpectedFile { name });
    }

    let mut compendia = Vec::with_capacity(files.compendia.len());
    let mut ids = BTreeSet::new();
    for (name, bytes) in &files.compendia {
        let compendium = parse_compendium(name, bytes)?;
        if !ids.insert(compendium.id.clone()) {
            return Err(invalid_document(
                name,
                format!("compendium id \"{}\" is used twice", compendium.id),
            ));
        }
        compendia.push(compendium);
    }

    if files.triggers.len() > MAX_TRIGGER_TEMPLATES {
        return Err(too_large(
            "the number of trigger templates",
            MAX_TRIGGER_TEMPLATES as u64,
        ));
    }
    let mut trigger_templates = Vec::with_capacity(files.triggers.len());
    let mut template_ids = BTreeSet::new();
    for (name, bytes) in &files.triggers {
        let template: TriggerTemplateDto =
            parse_document(name, bytes, MetaSchema::TriggerTemplate)?;
        if !template_ids.insert(template.id.clone()) {
            return Err(invalid_document(
                name,
                format!("trigger template id \"{}\" is used twice", template.id),
            ));
        }
        unique(
            template.params.iter().map(|p| p.id.as_str()),
            "template parameter",
        )
        .map_err(|reason| invalid_document(name, reason))?;
        trigger_templates.push(template);
    }

    Ok(PluginPackage {
        sha256,
        manifest,
        system,
        compendia,
        trigger_templates,
        assets: files.assets.into_values().collect(),
    })
}

fn parse_manifest(bytes: &[u8]) -> PluginResult<Manifest> {
    let invalid = |reason: String| PluginError::InvalidManifest { reason };
    let value: Value = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
    // A newer format gets its own error ("update the app"), not a list of schema violations.
    if let Some(format) = value.get("format").and_then(Value::as_u64)
        && format != u64::from(PLUGIN_FORMAT)
    {
        return Err(PluginError::UnsupportedFormat {
            format,
            supported: PLUGIN_FORMAT,
        });
    }
    MetaSchema::Manifest.check(&value)?.map_err(invalid)?;
    let manifest: Manifest = serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
    if manifest.dependencies.contains_key(&manifest.id) {
        return Err(invalid("a plugin cannot depend on itself".to_owned()));
    }
    Ok(manifest)
}

fn parse_system(
    bytes: &[u8],
    sheets: &mut BTreeMap<String, Vec<u8>>,
) -> PluginResult<SystemPackage> {
    let rules: SystemRules = parse_document(SYSTEM_FILE, bytes, MetaSchema::System)?;
    check_rules(&rules).map_err(|reason| invalid_document(SYSTEM_FILE, reason))?;

    let schema_name = &rules.sheet.schema;
    let layout_name = &rules.sheet.layout;
    if schema_name == layout_name {
        return Err(invalid_document(
            SYSTEM_FILE,
            "sheet schema and layout must be different files".to_owned(),
        ));
    }
    let schema_bytes = take_file(sheets, schema_name)?;
    let layout_bytes = take_file(sheets, layout_name)?;

    let sheet_schema = parse_json(schema_name, &schema_bytes)?;
    let unsupported = |reason: String| PluginError::UnsupportedSchema {
        file: schema_name.clone(),
        reason,
    };
    MetaSchema::SheetSchema
        .check(&sheet_schema)?
        .map_err(unsupported)?;
    sheet::check_schema(&sheet_schema).map_err(unsupported)?;

    // `@name` in expressions must mean one thing.
    let top_level = sheet_schema.get("properties").and_then(Value::as_object);
    for name in rules.computed.keys() {
        if top_level.is_some_and(|p| p.contains_key(name)) {
            return Err(invalid_document(
                SYSTEM_FILE,
                format!("computed \"{name}\" has the same name as a sheet field"),
            ));
        }
        if rules.resources.iter().any(|r| &r.id == name) {
            return Err(invalid_document(
                SYSTEM_FILE,
                format!("computed \"{name}\" has the same name as a resource"),
            ));
        }
    }

    let invalid_layout = |reason: String| PluginError::InvalidLayout {
        file: layout_name.clone(),
        reason,
    };
    let layout_value = parse_json(layout_name, &layout_bytes)?;
    MetaSchema::Layout
        .check(&layout_value)?
        .map_err(invalid_layout)?;
    let sheet_layout: LayoutNode =
        serde_json::from_value(layout_value).map_err(|e| invalid_layout(e.to_string()))?;
    sheet::check_layout(&sheet_layout, &sheet_schema, &rules).map_err(invalid_layout)?;

    Ok(SystemPackage {
        rules,
        sheet_schema,
        sheet_layout,
    })
}

/// What the meta-schema cannot express about `system.json`.
fn check_rules(rules: &SystemRules) -> Result<(), String> {
    unique(rules.resources.iter().map(|r| r.id.as_str()), "resource")?;
    unique(rules.rolls.iter().map(|r| r.id.as_str()), "roll")?;
    if rules.health_thresholds.windows(2).any(|w| w[0] <= w[1]) {
        return Err("health_thresholds must be strictly descending".to_owned());
    }
    Ok(())
}

fn unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("{what} id \"{id}\" is used twice"));
        }
    }
    Ok(())
}

fn parse_compendium(name: &str, bytes: &[u8]) -> PluginResult<Compendium> {
    let compendium: Compendium = parse_document(name, bytes, MetaSchema::Compendium)?;
    unique(
        compendium.entries.iter().map(|e| e.id.as_str()),
        "compendium entry",
    )
    .map_err(|reason| invalid_document(name, reason))?;
    Ok(compendium)
}

fn parse_document<T: DeserializeOwned>(
    name: &str,
    bytes: &[u8],
    meta: MetaSchema,
) -> PluginResult<T> {
    let value = parse_json(name, bytes)?;
    meta.check(&value)?
        .map_err(|reason| invalid_document(name, reason))?;
    serde_json::from_value(value).map_err(|e| invalid_document(name, e.to_string()))
}

fn parse_json(name: &str, bytes: &[u8]) -> PluginResult<Value> {
    serde_json::from_slice(bytes).map_err(|e| invalid_document(name, e.to_string()))
}

fn take_file(files: &mut BTreeMap<String, Vec<u8>>, name: &str) -> PluginResult<Vec<u8>> {
    files.remove(name).ok_or_else(|| PluginError::MissingFile {
        name: name.to_owned(),
    })
}

/// Lowercase hex SHA-256.
pub fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reads at most `limit` bytes; more is an error (`FileTooLarge`), whatever the header claimed.
fn read_limited(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::from(io::ErrorKind::FileTooLarge));
    }
    Ok(bytes)
}

fn too_large(what: &str, limit: u64) -> PluginError {
    PluginError::TooLarge {
        what: what.to_owned(),
        limit,
    }
}

fn io_or_too_large(e: io::Error, what: &str, limit: u64) -> PluginError {
    match e.kind() {
        io::ErrorKind::FileTooLarge => too_large(what, limit),
        _ => PluginError::Io(e),
    }
}

fn unsafe_entry(name: String, reason: &str) -> PluginError {
    PluginError::UnsafeEntry {
        name,
        reason: reason.to_owned(),
    }
}

fn invalid_document(file: &str, reason: String) -> PluginError {
    PluginError::InvalidDocument {
        file: file.to_owned(),
        reason,
    }
}

fn zip_error(e: zip::result::ZipError) -> PluginError {
    match e {
        zip::result::ZipError::Io(e) => PluginError::InvalidArchive {
            reason: e.to_string(),
        },
        other => PluginError::InvalidArchive {
            reason: other.to_string(),
        },
    }
}
