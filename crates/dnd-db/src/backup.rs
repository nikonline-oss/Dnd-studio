//! Campaign backups (CLAUDE.md §10): taken with `VACUUM INTO`, stored next to the campaign in
//! `<name>.backups/`, at most [`MAX_BACKUPS`] kept.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::error::{DbError, DbResult};

/// Number of backups kept per campaign.
pub const MAX_BACKUPS: usize = 5;

/// `C:/games/Crypt.sqlite` → `C:/games/Crypt.backups/`.
#[must_use]
pub fn backup_dir(campaign_path: &Path) -> PathBuf {
    let stem = campaign_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "campaign".to_owned());
    campaign_path.with_file_name(format!("{stem}.backups"))
}

/// Backups of a campaign, oldest first.
pub fn list_backups(campaign_path: &Path) -> DbResult<Vec<PathBuf>> {
    let dir = backup_dir(campaign_path);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("backup-") && n.ends_with(".sqlite"))
        })
        .collect();
    // File names start with a zero-padded timestamp, so lexical order is chronological.
    backups.sort();
    Ok(backups)
}

/// Writes a consistent copy of the open database and prunes old backups.
///
/// `reason` becomes part of the file name (`pre-migration`, `manual`, ...).
pub async fn create_backup(
    pool: &SqlitePool,
    campaign_path: &Path,
    reason: &str,
    now_ms: i64,
) -> DbResult<PathBuf> {
    let dir = backup_dir(campaign_path);
    std::fs::create_dir_all(&dir)?;
    let reason: String = reason
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut target = dir.join(format!("backup-{now_ms:015}-{reason}.sqlite"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("backup-{now_ms:015}-{reason}-{n}.sqlite"));
        n += 1;
    }
    vacuum_into(pool, &target).await?;
    prune(campaign_path)?;
    tracing::info!(backup = %target.display(), "campaign backup created");
    Ok(target)
}

/// Writes a consistent copy of the open database to `target` (which must not exist).
pub(crate) async fn vacuum_into(pool: &SqlitePool, target: &Path) -> DbResult<()> {
    sqlx::query("VACUUM INTO ?")
        .bind(target.to_string_lossy().into_owned())
        .execute(pool)
        .await?;
    Ok(())
}

fn prune(campaign_path: &Path) -> DbResult<()> {
    let backups = list_backups(campaign_path)?;
    let excess = backups.len().saturating_sub(MAX_BACKUPS);
    for old in backups.iter().take(excess) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

/// Replaces the campaign file with its most recent backup. The campaign must be closed.
///
/// The damaged file is kept as `<name>.sqlite.corrupt` for diagnostics.
pub fn restore_latest_backup(campaign_path: &Path) -> DbResult<PathBuf> {
    let latest = list_backups(campaign_path)?
        .pop()
        .ok_or_else(|| DbError::NoBackup(campaign_path.to_owned()))?;
    if campaign_path.exists() {
        let mut corrupt = campaign_path.as_os_str().to_owned();
        corrupt.push(".corrupt");
        std::fs::rename(campaign_path, PathBuf::from(corrupt))?;
    }
    for suffix in ["-wal", "-shm"] {
        let mut side = campaign_path.as_os_str().to_owned();
        side.push(suffix);
        let side = PathBuf::from(side);
        if side.exists() {
            std::fs::remove_file(side)?;
        }
    }
    std::fs::copy(&latest, campaign_path)?;
    tracing::warn!(backup = %latest.display(), "campaign restored from backup");
    Ok(latest)
}
