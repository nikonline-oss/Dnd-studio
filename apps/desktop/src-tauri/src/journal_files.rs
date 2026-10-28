//! Journal import and export as files (ADR-0026). This layer only reads and writes: a folder the
//! user picks in a native dialog (CLAUDE.md §9) is read into a list of Markdown and CSV texts, or a
//! list of texts is written into it. Which folders and entries they become, and how links are
//! rewritten, is planned by the UI with pure functions and sent as ordinary intents — the same way
//! in a local campaign and in a remote session.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use crate::error::IpcError;

/// Most files read from (or written to) one folder.
pub const MAX_FILES: usize = 2000;
/// Largest file read on import; bigger ones are skipped (a journal text is at most 1 MiB).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Largest text written on export (a table as CSV may be a little larger than its cells).
const MAX_WRITE_BYTES: usize = 2 * 1024 * 1024;
/// Deepest folder nesting read (the journal allows 16 levels below the import's own folder).
pub const MAX_DEPTH: usize = 15;
/// Longest name of one file or folder.
const MAX_SEGMENT_CHARS: usize = 200;

/// A file of a journal tree: folder names, then the file name with its extension.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JournalFile {
    pub path: Vec<String>,
    pub text: String,
}

/// What an import read: the picked folder's name and its Markdown and CSV files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct JournalImport {
    pub name: String,
    pub files: Vec<JournalFile>,
    /// Files left out: too large, not UTF-8, or over the file limit.
    pub skipped: u32,
}

fn is_journal_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "csv"))
}

/// Reads the Markdown and CSV files below `root`, sorted by path. Hidden folders and files
/// (`.obsidian`, `.git`) and symbolic links are not followed.
pub fn read_tree(root: &Path) -> std::io::Result<(Vec<JournalFile>, u32)> {
    let mut files = Vec::new();
    let mut skipped = 0u32;
    let mut stack: Vec<(PathBuf, Vec<String>)> = vec![(root.to_owned(), Vec::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                skipped += 1;
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let kind = entry.file_type()?;
            let mut path = prefix.clone();
            path.push(name);
            if kind.is_dir() {
                if prefix.len() < MAX_DEPTH {
                    stack.push((entry.path(), path));
                }
            } else if kind.is_file() && is_journal_file(&entry.path()) {
                if files.len() >= MAX_FILES || entry.metadata()?.len() > MAX_FILE_BYTES {
                    skipped += 1;
                    continue;
                }
                match String::from_utf8(std::fs::read(entry.path())?) {
                    Ok(text) => files.push(JournalFile { path, text }),
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, skipped))
}

/// A file or folder name that stays inside the export folder and is valid on every system.
pub fn valid_segment(name: &str) -> bool {
    const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = RESERVED.contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    !name.is_empty()
        && name.chars().count() <= MAX_SEGMENT_CHARS
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.starts_with(' ')
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        && !reserved
}

/// Writes `files` below `root`, creating folders; existing files of the same names are replaced.
pub fn write_tree(root: &Path, files: &[JournalFile]) -> Result<(), IpcError> {
    if files.len() > MAX_FILES * 2 {
        return Err(IpcError::user("too_many_files", "too many journal files"));
    }
    for file in files {
        if file.path.is_empty() || !file.path.iter().all(|s| valid_segment(s)) {
            return Err(IpcError::user(
                "invalid_name",
                format!("invalid file name: {}", file.path.join("/")),
            ));
        }
        if file.text.len() > MAX_WRITE_BYTES {
            return Err(IpcError::user(
                "file_too_large",
                "a journal file is too large",
            ));
        }
    }
    for file in files {
        let path = file.path.iter().fold(root.to_owned(), |p, s| p.join(s));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| IpcError::internal("io", e.to_string()))?;
        }
        std::fs::write(&path, &file.text).map_err(|e| IpcError::internal("io", e.to_string()))?;
    }
    Ok(())
}

async fn pick_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder.and_then(|f| f.into_path().ok()));
    });
    rx.await.ok().flatten()
}

/// Asks for a folder and reads its Markdown and CSV files. `None` if the dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn journal_import_read<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<JournalImport>, IpcError> {
    let Some(root) = pick_folder(&app).await else {
        return Ok(None);
    };
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Import".to_owned());
    let (files, skipped) = tokio::task::spawn_blocking(move || read_tree(&root))
        .await
        .map_err(|e| IpcError::internal("io", e.to_string()))?
        .map_err(|e| IpcError::user("read_failed", e.to_string()))?;
    Ok(Some(JournalImport {
        name,
        files,
        skipped,
    }))
}

/// Asks for a folder and writes the journal's files into it. Returns the folder, or `None` if the
/// dialog was cancelled.
#[tauri::command]
#[specta::specta]
pub async fn journal_export_write<R: Runtime>(
    app: AppHandle<R>,
    files: Vec<JournalFile>,
) -> Result<Option<String>, IpcError> {
    let Some(root) = pick_folder(&app).await else {
        return Ok(None);
    };
    let shown = root.display().to_string();
    tokio::task::spawn_blocking(move || write_tree(&root, &files))
        .await
        .map_err(|e| IpcError::internal("io", e.to_string()))??;
    Ok(Some(shown))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn file(path: &[&str], text: &str) -> JournalFile {
        JournalFile {
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn writes_and_reads_back_a_tree_skipping_hidden_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let files = vec![
            file(&["Lore", "Gods.md"], "# Gods"),
            file(&["Lore", "Prices.csv"], "Item\r\nRope\r\n"),
            file(&["Start.md"], "See [Gods](Lore/Gods.md)"),
        ];
        write_tree(dir.path(), &files).unwrap();
        std::fs::create_dir(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join(".obsidian").join("x.md"), "hidden").unwrap();
        std::fs::write(dir.path().join("picture.png"), [0u8, 1, 2]).unwrap();
        std::fs::write(dir.path().join("bad.md"), [0xffu8, 0xfe]).unwrap();

        let (read, skipped) = read_tree(dir.path()).unwrap();
        let mut expected = files;
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(read, expected);
        assert_eq!(skipped, 1, "the file that is not UTF-8");
    }

    #[test]
    fn names_never_leave_the_export_folder() {
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "c:",
            "x?",
            "trail.",
            "trail ",
            "CON",
            "com1.md",
            "lpt9",
            "tab\there",
        ] {
            assert!(!valid_segment(bad), "{bad:?}");
        }
        for good in [
            "Gods.md",
            "Таверна «Гусь».csv",
            "com10.md",
            "console.md",
            "a.b.md",
        ] {
            assert!(valid_segment(good), "{good:?}");
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(write_tree(dir.path(), &[file(&["..", "escape.md"], "x")]).is_err());
        assert!(!dir.path().parent().unwrap().join("escape.md").exists());
    }
}
