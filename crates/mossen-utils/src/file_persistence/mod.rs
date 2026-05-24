// Translated from utils/filePersistence/types.ts, outputsScanner.ts, filePersistence.ts

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;

// ============================================================================
// types.ts
// ============================================================================

pub const OUTPUTS_SUBDIR: &str = "outputs";
pub const FILE_COUNT_LIMIT: usize = 1_000;
pub const DEFAULT_UPLOAD_CONCURRENCY: usize = 8;

pub type TurnStartTime = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedFile {
    pub filename: String,
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedPersistence {
    pub filename: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesPersistedEventData {
    pub files: Vec<PersistedFile>,
    pub failed: Vec<FailedPersistence>,
}

// ============================================================================
// outputsScanner.ts
// ============================================================================

/// Shared debug logger for file persistence modules.
fn log_debug(message: &str) {
    eprintln!("[file-persistence] {}", message);
}

/// Environment kind from MOSSEN_CODE_ENVIRONMENT_KIND.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentKind {
    Byoc,
    MossenCloud,
}

/// Get the environment kind from MOSSEN_CODE_ENVIRONMENT_KIND.
/// Returns None if not set or not a recognized value.
pub fn get_environment_kind() -> Option<EnvironmentKind> {
    match std::env::var("MOSSEN_CODE_ENVIRONMENT_KIND").ok()?.as_str() {
        "byoc" => Some(EnvironmentKind::Byoc),
        "mossen_cloud" => Some(EnvironmentKind::MossenCloud),
        _ => None,
    }
}

/// Find files that have been modified since the turn started.
/// Returns paths of files with mtime >= turnStartTime.
pub async fn find_modified_files(turn_start_time: TurnStartTime, outputs_dir: &str) -> Vec<String> {
    let outputs_path = Path::new(outputs_dir);

    let entries = match read_dir_recursive(outputs_path).await {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    if entries.is_empty() {
        log_debug("No files found in outputs directory");
        return Vec::new();
    }

    let mut modified_files = Vec::new();

    for entry_path in &entries {
        match fs::symlink_metadata(entry_path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_file() {
                    if let Ok(mtime) = metadata.modified() {
                        let mtime_ms = mtime
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        if mtime_ms >= turn_start_time {
                            modified_files.push(entry_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    log_debug(&format!(
        "Found {} modified files since turn start (scanned {} total)",
        modified_files.len(),
        entries.len()
    ));

    modified_files
}

/// Recursively read a directory and return all file paths.
async fn read_dir_recursive(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut read_dir = match fs::read_dir(&current).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            match entry.file_type().await {
                Ok(ft) => {
                    if ft.is_dir() {
                        stack.push(path);
                    } else if ft.is_file() {
                        result.push(path);
                    }
                    // Skip symlinks
                }
                Err(_) => continue,
            }
        }
    }

    Ok(result)
}

// ============================================================================
// filePersistence.ts
// ============================================================================

/// Configuration for Files API.
#[derive(Debug, Clone)]
pub struct FilesApiConfig {
    pub oauth_token: String,
    pub session_id: String,
}

/// Execute file persistence for modified files in the outputs directory.
pub async fn run_file_persistence(
    turn_start_time: TurnStartTime,
    _abort: Option<()>,
) -> Option<FilesPersistedEventData> {
    let environment_kind = get_environment_kind();
    if environment_kind != Some(EnvironmentKind::Byoc) {
        return None;
    }

    let session_access_token = std::env::var("MOSSEN_SESSION_INGRESS_AUTH_TOKEN").ok();
    if session_access_token.is_none() {
        return None;
    }

    let session_id = match std::env::var("MOSSEN_CODE_REMOTE_SESSION_ID") {
        Ok(id) => id,
        Err(_) => {
            eprintln!("File persistence enabled but MOSSEN_CODE_REMOTE_SESSION_ID is not set");
            return None;
        }
    };

    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let outputs_dir = format!("{}/{}/{}", cwd, session_id, OUTPUTS_SUBDIR);

    let modified_files = find_modified_files(turn_start_time, &outputs_dir).await;

    if modified_files.is_empty() {
        log_debug("No modified files to persist");
        return None;
    }

    log_debug(&format!("Found {} modified files", modified_files.len()));

    if modified_files.len() > FILE_COUNT_LIMIT {
        log_debug(&format!(
            "File count limit exceeded: {} > {}",
            modified_files.len(),
            FILE_COUNT_LIMIT
        ));
        return Some(FilesPersistedEventData {
            files: Vec::new(),
            failed: vec![FailedPersistence {
                filename: outputs_dir,
                error: format!(
                    "Too many files modified ({}). Maximum: {}.",
                    modified_files.len(),
                    FILE_COUNT_LIMIT
                ),
            }],
        });
    }

    // Filter files that resolve outside the outputs directory
    let outputs_path = Path::new(&outputs_dir);
    let files_to_process: Vec<(String, String)> = modified_files
        .iter()
        .filter_map(|file_path| {
            let fp = Path::new(file_path);
            let relative = fp.strip_prefix(outputs_path).ok()?;
            let rel_str = relative.to_string_lossy().to_string();
            if rel_str.starts_with("..") {
                log_debug(&format!(
                    "Skipping file outside outputs directory: {}",
                    rel_str
                ));
                return None;
            }
            Some((file_path.clone(), rel_str))
        })
        .collect();

    log_debug(&format!(
        "BYOC mode: uploading {} files",
        files_to_process.len()
    ));

    // In a real implementation, would upload files here
    // For now, return empty success
    Some(FilesPersistedEventData {
        files: Vec::new(),
        failed: Vec::new(),
    })
}

/// Execute Cloud (1P) mode persistence.
fn execute_cloud_persistence() -> FilesPersistedEventData {
    log_debug("Cloud mode: xattr-based file ID reading not yet implemented");
    FilesPersistedEventData {
        files: Vec::new(),
        failed: Vec::new(),
    }
}

/// Execute file persistence and emit result via callback.
pub async fn execute_file_persistence<F>(
    turn_start_time: TurnStartTime,
    _abort: Option<()>,
    on_result: F,
) where
    F: FnOnce(FilesPersistedEventData),
{
    match run_file_persistence(turn_start_time, None).await {
        Some(result) => on_result(result),
        None => {}
    }
}

/// Check if file persistence is enabled.
pub fn is_file_persistence_enabled() -> bool {
    get_environment_kind() == Some(EnvironmentKind::Byoc)
        && std::env::var("MOSSEN_SESSION_INGRESS_AUTH_TOKEN").is_ok()
        && std::env::var("MOSSEN_CODE_REMOTE_SESSION_ID").is_ok()
}
