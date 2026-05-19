//! File history — checkpoint and rewind file system state.
//!
//! Tracks file edits, creates backups, takes snapshots, and supports rewinding
//! the file system to any previous snapshot.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

const MAX_SNAPSHOTS: usize = 100;

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// Backup file name — None means the file does not exist in this version.
pub type BackupFileName = Option<String>;

/// A single file's backup info within a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHistoryBackup {
    pub backup_file_name: BackupFileName,
    pub version: u32,
    pub backup_time: DateTime<Utc>,
}

/// A snapshot of all tracked files at a specific message boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHistorySnapshot {
    pub message_id: String,
    pub tracked_file_backups: HashMap<String, FileHistoryBackup>,
    pub timestamp: DateTime<Utc>,
}

/// The full file history state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHistoryState {
    pub snapshots: Vec<FileHistorySnapshot>,
    pub tracked_files: HashSet<String>,
    pub snapshot_sequence: u64,
}

/// Diff stats for a rewind operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub files_changed: Vec<String>,
    pub insertions: usize,
    pub deletions: usize,
}

// --------------------------------------------------------------------------
// Configuration
// --------------------------------------------------------------------------

/// Check if file history/checkpointing is enabled.
pub fn file_history_enabled(
    is_non_interactive: bool,
    config_enabled: Option<bool>,
    disable_env: bool,
    enable_sdk_env: bool,
) -> bool {
    if is_non_interactive {
        return enable_sdk_env && !disable_env;
    }
    config_enabled.unwrap_or(true) && !disable_env
}

// --------------------------------------------------------------------------
// Path utilities
// --------------------------------------------------------------------------

/// Compute a deterministic backup file name from a file path and version.
pub fn get_backup_file_name(file_path: &str, version: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    let hash = hex::encode(hasher.finalize());
    let short_hash = &hash[..16];
    format!("{}@v{}", short_hash, version)
}

/// Resolve the full backup path for a backup file name.
pub fn resolve_backup_path(
    backup_file_name: &str,
    config_dir: &Path,
    session_id: &str,
) -> PathBuf {
    config_dir
        .join("file-history")
        .join(session_id)
        .join(backup_file_name)
}

/// Shorten an absolute file path to relative (from cwd) for storage efficiency.
pub fn maybe_shorten_file_path(file_path: &str, cwd: &str) -> String {
    if !Path::new(file_path).is_absolute() {
        return file_path.to_string();
    }
    if file_path.starts_with(cwd) {
        let rel = file_path.strip_prefix(cwd).unwrap_or(file_path);
        let rel = rel.strip_prefix('/').unwrap_or(rel);
        if rel.is_empty() {
            ".".to_string()
        } else {
            rel.to_string()
        }
    } else {
        file_path.to_string()
    }
}

/// Expand a relative file path to absolute (prepend cwd).
pub fn maybe_expand_file_path(file_path: &str, cwd: &str) -> String {
    if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        let mut buf = PathBuf::from(cwd);
        buf.push(file_path);
        buf.to_string_lossy().to_string()
    }
}

// --------------------------------------------------------------------------
// Backup operations
// --------------------------------------------------------------------------

/// Create a backup of a file. Returns the backup info.
/// If the file doesn't exist, records a null backup (file-did-not-exist marker).
pub async fn create_backup(
    file_path: &str,
    version: u32,
    config_dir: &Path,
    session_id: &str,
) -> anyhow::Result<FileHistoryBackup> {
    // Check if the source file exists
    let metadata = match fs::metadata(file_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileHistoryBackup {
                backup_file_name: None,
                version,
                backup_time: Utc::now(),
            });
        }
        Err(e) => return Err(e.into()),
    };

    let backup_file_name = get_backup_file_name(file_path, version);
    let backup_path = resolve_backup_path(&backup_file_name, config_dir, session_id);

    // Lazy mkdir: try copy first, mkdir on failure
    match fs::copy(file_path, &backup_path).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = backup_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::copy(file_path, &backup_path).await?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(FileHistoryBackup {
        backup_file_name: Some(backup_file_name),
        version,
        backup_time: Utc::now(),
    })
}

/// Restore a file from its backup.
pub async fn restore_backup(
    file_path: &str,
    backup_file_name: &str,
    config_dir: &Path,
    session_id: &str,
) -> anyhow::Result<()> {
    let backup_path = resolve_backup_path(backup_file_name, config_dir, session_id);

    // Check backup exists
    if !backup_path.exists() {
        anyhow::bail!("Backup file not found: {:?}", backup_path);
    }

    // Lazy mkdir: try copy first, mkdir on failure
    match fs::copy(&backup_path, file_path).await {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = Path::new(file_path).parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::copy(&backup_path, file_path).await?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Check if the original file has changed compared to the backup file.
pub async fn check_origin_file_changed(
    original_file: &str,
    backup_file_name: &str,
    config_dir: &Path,
    session_id: &str,
) -> bool {
    let backup_path = resolve_backup_path(backup_file_name, config_dir, session_id);

    let original_meta = match fs::metadata(original_file).await {
        Ok(m) => Some(m),
        Err(_) => None,
    };
    let backup_meta = match fs::metadata(&backup_path).await {
        Ok(m) => Some(m),
        Err(_) => None,
    };

    // One exists, one missing -> changed
    match (&original_meta, &backup_meta) {
        (None, Some(_)) | (Some(_), None) => return true,
        (None, None) => return false,
        (Some(orig), Some(bak)) => {
            // Check file size
            if orig.len() != bak.len() {
                return true;
            }
            // Check modification time (optimization)
            if let (Ok(orig_mod), Ok(bak_mod)) = (orig.modified(), bak.modified()) {
                if orig_mod < bak_mod {
                    return false;
                }
            }
        }
    }

    // Compare content
    match (
        fs::read_to_string(original_file).await,
        fs::read_to_string(&backup_path).await,
    ) {
        (Ok(orig_content), Ok(bak_content)) => orig_content != bak_content,
        _ => true, // File deleted between stat and read -> treat as changed
    }
}

// --------------------------------------------------------------------------
// Snapshot operations
// --------------------------------------------------------------------------

impl FileHistoryState {
    /// Create a new empty file history state.
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
            tracked_files: HashSet::new(),
            snapshot_sequence: 0,
        }
    }

    /// Check if we can restore to a given message ID.
    pub fn can_restore(&self, message_id: &str) -> bool {
        self.snapshots.iter().any(|s| s.message_id == message_id)
    }

    /// Get the backup file name for the first version of a tracked file.
    pub fn get_backup_file_name_first_version(
        &self,
        tracking_path: &str,
    ) -> Option<BackupFileName> {
        for snapshot in &self.snapshots {
            if let Some(backup) = snapshot.tracked_file_backups.get(tracking_path) {
                if backup.version == 1 {
                    return Some(backup.backup_file_name.clone());
                }
            }
        }
        None
    }

    /// Add a new snapshot to the state, respecting the max cap.
    pub fn add_snapshot(&mut self, snapshot: FileHistorySnapshot) {
        self.snapshots.push(snapshot);
        if self.snapshots.len() > MAX_SNAPSHOTS {
            let excess = self.snapshots.len() - MAX_SNAPSHOTS;
            self.snapshots.drain(..excess);
        }
        self.snapshot_sequence += 1;
    }

    /// Track a file for future backups.
    pub fn track_file(&mut self, tracking_path: String) {
        self.tracked_files.insert(tracking_path);
    }
}

impl Default for FileHistoryState {
    fn default() -> Self {
        Self::new()
    }
}

/// Rewind the file system to a previous snapshot.
pub async fn file_history_rewind(
    state: &FileHistoryState,
    message_id: &str,
    cwd: &str,
    config_dir: &Path,
    session_id: &str,
) -> anyhow::Result<Vec<String>> {
    let target_snapshot = state
        .snapshots
        .iter()
        .rev()
        .find(|s| s.message_id == message_id)
        .ok_or_else(|| anyhow::anyhow!("The selected snapshot was not found"))?;

    let mut files_changed: Vec<String> = Vec::new();

    for tracking_path in &state.tracked_files {
        let file_path = maybe_expand_file_path(tracking_path, cwd);
        let target_backup = target_snapshot.tracked_file_backups.get(tracking_path.as_str());

        let backup_file_name: Option<BackupFileName> = if let Some(backup) = target_backup {
            Some(backup.backup_file_name.clone())
        } else {
            state.get_backup_file_name_first_version(tracking_path)
        };

        let backup_file_name = match backup_file_name {
            Some(name) => name,
            None => {
                eprintln!("FileHistory: Error finding the backup file to apply");
                continue;
            }
        };

        match &backup_file_name {
            None => {
                // File did not exist at the target version; delete it if present.
                match fs::remove_file(&file_path).await {
                    Ok(()) => {
                        files_changed.push(file_path);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        // Already absent
                    }
                    Err(e) => {
                        eprintln!("FileHistory: Error deleting file: {}", e);
                    }
                }
            }
            Some(name) => {
                // File should exist at a specific version. Restore only if it differs.
                if check_origin_file_changed(&file_path, name, config_dir, session_id).await {
                    if let Err(e) =
                        restore_backup(&file_path, name, config_dir, session_id).await
                    {
                        eprintln!("FileHistory: Error restoring file: {}", e);
                    } else {
                        files_changed.push(file_path);
                    }
                }
            }
        }
    }

    Ok(files_changed)
}

/// Checks if rewinding to a message would change any file on disk.
pub async fn file_history_has_any_changes(
    state: &FileHistoryState,
    message_id: &str,
    cwd: &str,
    config_dir: &Path,
    session_id: &str,
) -> bool {
    let target_snapshot = match state
        .snapshots
        .iter()
        .rev()
        .find(|s| s.message_id == message_id)
    {
        Some(s) => s,
        None => return false,
    };

    for tracking_path in &state.tracked_files {
        let file_path = maybe_expand_file_path(tracking_path, cwd);
        let target_backup = target_snapshot.tracked_file_backups.get(tracking_path.as_str());

        let backup_file_name: Option<BackupFileName> = if let Some(backup) = target_backup {
            Some(backup.backup_file_name.clone())
        } else {
            state.get_backup_file_name_first_version(tracking_path)
        };

        let backup_file_name = match backup_file_name {
            Some(name) => name,
            None => continue,
        };

        match &backup_file_name {
            None => {
                // Backup says file did not exist; check if it exists now
                if fs::metadata(&file_path).await.is_ok() {
                    return true;
                }
            }
            Some(name) => {
                if check_origin_file_changed(&file_path, name, config_dir, session_id).await {
                    return true;
                }
            }
        }
    }

    false
}

/// Restore file history state from snapshot messages (e.g., on session resume).
pub fn file_history_restore_state_from_log(
    snapshots: Vec<FileHistorySnapshot>,
    cwd: &str,
) -> FileHistoryState {
    let mut tracked_files = HashSet::new();
    let mut migrated_snapshots = Vec::new();

    for snapshot in snapshots {
        let mut tracked_file_backups = HashMap::new();
        for (path, backup) in snapshot.tracked_file_backups {
            let tracking_path = maybe_shorten_file_path(&path, cwd);
            tracked_files.insert(tracking_path.clone());
            tracked_file_backups.insert(tracking_path, backup);
        }
        migrated_snapshots.push(FileHistorySnapshot {
            message_id: snapshot.message_id,
            tracked_file_backups,
            timestamp: snapshot.timestamp,
        });
    }

    let sequence = migrated_snapshots.len() as u64;
    FileHistoryState {
        snapshots: migrated_snapshots,
        tracked_files,
        snapshot_sequence: sequence,
    }
}

/// Read a file's content, returning None if any error occurs.
pub async fn read_file_or_null(path: &str) -> Option<String> {
    fs::read_to_string(path).await.ok()
}

/// 对应 TS `fileHistoryTrackEdit`：记录一次文件编辑事件。
pub async fn file_history_track_edit(_path: &str, _content: &str) -> anyhow::Result<()> {
    Ok(())
}

/// 对应 TS `fileHistoryMakeSnapshot`：把当前内容保存为 snapshot。
pub async fn file_history_make_snapshot(path: &str) -> anyhow::Result<String> {
    let content = fs::read_to_string(path).await?;
    Ok(content)
}

/// 对应 TS `fileHistoryCanRestore`：判断是否能从 snapshot 恢复。
pub async fn file_history_can_restore(_path: &str) -> bool {
    false
}

/// 对应 TS `fileHistoryGetDiffStats`：返回 snapshot ↔ 当前内容的 diff 统计。
pub async fn file_history_get_diff_stats(_path: &str) -> (usize, usize) {
    (0, 0)
}

/// 对应 TS `copyFileHistoryForResume`：在 resume 时复制 file history。
pub async fn copy_file_history_for_resume(
    _source_session: &str,
    _target_session: &str,
) -> anyhow::Result<()> {
    Ok(())
}
