//! Packing a plugin source folder into a `.dndplugin` (bundled plugins; `plugin-tool` in 3b).
//!
//! Only names the format allows are taken (README, attribution notes and the like stay out), in
//! sorted order with a fixed timestamp, so the same sources always give the same bytes and the
//! same SHA-256.

use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::error::{PluginError, PluginResult};
use crate::package::{MANIFEST_FILE, SYSTEM_FILE, is_package_name};

/// Packs the whitelisted files of `dir` into `.dndplugin` bytes. The result still has to pass
/// [`crate::read_package`]; this only builds the ZIP.
pub fn pack_directory(dir: &Path) -> PluginResult<Vec<u8>> {
    let mut names = Vec::new();
    for top in [MANIFEST_FILE, SYSTEM_FILE] {
        if dir.join(top).is_file() {
            names.push(top.to_owned());
        }
    }
    for folder in ["sheet", "compendia", "triggers", "assets"] {
        let path = dir.join(folder);
        if !path.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            if let Some(file) = entry.file_name().to_str() {
                let name = format!("{folder}/{file}");
                if is_package_name(&name) {
                    names.push(name);
                }
            }
        }
    }
    names.sort();

    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    let archive_error = |e: zip::result::ZipError| PluginError::InvalidArchive {
        reason: e.to_string(),
    };
    for name in &names {
        zip.start_file(name.as_str(), options)
            .map_err(archive_error)?;
        zip.write_all(&std::fs::read(dir.join(name))?)?;
    }
    Ok(zip.finish().map_err(archive_error)?.into_inner())
}
