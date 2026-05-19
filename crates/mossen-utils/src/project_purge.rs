//! Project purge — archive and delete project data with safety guardrails.
//!
//! Two-step flow: `get_project_purge_plan()` builds a dry-run plan,
//! `execute_project_purge_plan()` consumes the token and performs archive + delete.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::fs;
use tracing::debug;

/// TTL for project-purge plan tokens (10 minutes).
pub const PROJECT_PURGE_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

/// Memory location status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPurgeMemoryStatus {
    InProject,
    External,
    Absent,
}

/// A single entry within a project directory.
#[derive(Debug, Clone)]
pub struct ProjectPurgeEntry {
    pub name: String,
    pub abs_path: PathBuf,
    pub kind: EntryKind,
    pub size_bytes: i64,
    pub is_memory: bool,
}

/// Kind of file entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Other,
}

/// A dry-run purge plan.
#[derive(Debug, Clone)]
pub struct ProjectPurgePlan {
    pub token: String,
    pub created_at: u64,
    pub target_cwd: String,
    pub sanitized_target: String,
    pub original_project_dir: PathBuf,
    pub memory_status: ProjectPurgeMemoryStatus,
    pub memory_external_hint: Option<String>,
    pub memory_external_reason: Option<String>,
    pub include_memory: bool,
    pub to_archive: Vec<ProjectPurgeEntry>,
    pub to_skip: Vec<ProjectPurgeEntry>,
    pub total_archive_bytes: i64,
    pub archive_dir: PathBuf,
}

/// Errors that can occur during project purge.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProjectPurgeError {
    #[error("unknown token")]
    UnknownToken,
    #[error("expired token")]
    ExpiredToken,
    #[error("active project: {target_cwd}")]
    ActiveProject {
        target_cwd: String,
        sanitized_target: String,
    },
    #[error("invalid target: {reason}")]
    InvalidTarget { target_cwd: String, reason: String },
    #[error("unsupported flag: {flag}")]
    UnsupportedFlag { flag: String },
    #[error("external memory include rejected")]
    ExternalMemoryIncludeRejected {
        external_hint: Option<String>,
        reason: Option<String>,
    },
    #[error("token target mismatch: expected {expected}, got {got}")]
    TokenTargetMismatch { expected: String, got: String },
    #[error("project dir missing: {path}")]
    ProjectDirMissing { path: String },
}

/// Result of executing a purge plan.
#[derive(Debug, Clone)]
pub struct ProjectPurgeResult {
    pub archived_entries: Vec<ArchivedEntry>,
    pub skipped_entries: Vec<SkippedEntry>,
    pub errors: Vec<PurgePhaseError>,
    pub archive_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub total_archived_bytes: i64,
    pub project_dir_removed: bool,
    pub phase_a_halted: bool,
}

#[derive(Debug, Clone)]
pub struct ArchivedEntry {
    pub name: String,
    pub kind: EntryKind,
    pub bytes: i64,
}

#[derive(Debug, Clone)]
pub struct SkippedEntry {
    pub name: String,
    pub kind: EntryKind,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct PurgePhaseError {
    pub phase: PurgePhase,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgePhase {
    Copy,
    Delete,
    Manifest,
    Cleanup,
}

/// Module-level plan store.
static PLAN_STORE: once_cell::sync::Lazy<Mutex<HashMap<String, ProjectPurgePlan>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

/// Path separator for export.
pub const PROJECT_PURGE_PATH_SEPARATOR: char = std::path::MAIN_SEPARATOR;

/// Evict expired plans from the store.
fn evict_expired_plans(now: u64) {
    if let Ok(mut store) = PLAN_STORE.lock() {
        store.retain(|_, plan| now - plan.created_at <= PROJECT_PURGE_TOKEN_TTL_MS);
    }
}

/// Generate a random 8-hex-char purge token.
fn generate_purge_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 4] = rng.gen();
    hex::encode(bytes)
}

/// Create a timestamp suitable for archive dir names.
fn timestamp_for_archive_dir(now_ms: u64) -> String {
    let secs = now_ms / 1000;
    let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
        .unwrap_or_else(|| chrono::Utc::now());
    dt.format("%Y-%m-%dT%H-%M-%S")
        .to_string()
        .replace([':', '.'], "-")
}

/// Compute total size of an entry (recursively for directories).
fn compute_entry_size_bytes(abs_path: &Path, kind: EntryKind) -> std::pin::Pin<Box<dyn std::future::Future<Output = i64> + Send + '_>> {
    Box::pin(async move {
    match kind {
        EntryKind::File => match fs::metadata(abs_path).await {
            Ok(meta) => meta.len() as i64,
            Err(_) => -1,
        },
        EntryKind::Directory => {
            let mut total: i64 = 0;
            let mut entries = match fs::read_dir(abs_path).await {
                Ok(e) => e,
                Err(_) => return -1,
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let ft = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => return -1,
                };
                let child_path = entry.path();
                if ft.is_dir() {
                    let sub = compute_entry_size_bytes(&child_path, EntryKind::Directory).await;
                    if sub < 0 {
                        return -1;
                    }
                    total += sub;
                } else if ft.is_file() {
                    match fs::metadata(&child_path).await {
                        Ok(meta) => total += meta.len() as i64,
                        Err(_) => return -1,
                    }
                }
            }
            total
        }
        EntryKind::Other => -1,
    }
    }) // end Box::pin
}

/// Sanitize a path for use as a directory name.
pub fn sanitize_path(path: &str) -> String {
    path.replace(['/', '\\', ':', ' '], "_")
        .trim_matches('_')
        .to_string()
}

/// Detect memory override from environment variables or settings.
fn detect_memory_override() -> (bool, Option<String>, Option<String>) {
    if let Ok(val) = std::env::var("MOSSEN_COWORK_MEMORY_PATH_OVERRIDE") {
        if !val.is_empty() {
            return (
                true,
                Some(val),
                Some("env.MOSSEN_COWORK_MEMORY_PATH_OVERRIDE".to_string()),
            );
        }
    }
    if let Ok(val) = std::env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR") {
        if !val.is_empty() {
            return (
                true,
                Some(val),
                Some("env.MOSSEN_CODE_REMOTE_MEMORY_DIR".to_string()),
            );
        }
    }
    (false, None, None)
}

/// Classify memory status for a project directory.
async fn classify_memory(
    project_dir: &Path,
) -> (ProjectPurgeMemoryStatus, Option<String>, Option<String>) {
    let (is_override, hint, reason) = detect_memory_override();
    if is_override {
        return (ProjectPurgeMemoryStatus::External, hint, reason);
    }
    let memory_path = project_dir.join("memory");
    match fs::metadata(&memory_path).await {
        Ok(meta) if meta.is_dir() => (ProjectPurgeMemoryStatus::InProject, None, None),
        _ => (ProjectPurgeMemoryStatus::Absent, None, None),
    }
}

/// Recursive copy that refuses to follow symlinks.
async fn copy_recursive_no_symlink(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).await?;

    if meta.file_type().is_symlink() {
        debug!("projectPurge: skipping symlink during archive: {:?}", src);
        return Ok(());
    }

    if meta.is_dir() {
        fs::create_dir_all(dest).await?;
        let mut entries = fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let child_src = entry.path();
            let child_dest = dest.join(entry.file_name());
            Box::pin(copy_recursive_no_symlink(&child_src, &child_dest)).await?;
        }
        return Ok(());
    }

    if meta.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::copy(src, dest).await?;
        return Ok(());
    }

    debug!("projectPurge: skipping non-regular entry: {:?}", src);
    Ok(())
}

/// Build a dry-run purge plan.
pub async fn get_project_purge_plan(
    target_cwd: Option<&str>,
    include_memory: bool,
    original_cwd: &str,
    project_root: &str,
    session_project_dir: Option<&str>,
    config_home: &Path,
    projects_dir: &Path,
) -> std::result::Result<ProjectPurgePlan, ProjectPurgeError> {
    let now = current_time_ms();
    evict_expired_plans(now);

    let raw_target = target_cwd.unwrap_or(original_cwd).trim();
    let raw_target = if raw_target.is_empty() {
        original_cwd
    } else {
        raw_target
    };

    let canonical = match tokio::fs::canonicalize(raw_target).await {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            return Err(ProjectPurgeError::InvalidTarget {
                target_cwd: raw_target.to_string(),
                reason: format!("realpath failed: {}", e),
            });
        }
    };

    // Active project guard
    let target_sanitized = sanitize_path(&canonical);
    let orig_sanitized = sanitize_path(original_cwd);
    let root_sanitized = sanitize_path(project_root);

    if target_sanitized == orig_sanitized || target_sanitized == root_sanitized {
        return Err(ProjectPurgeError::ActiveProject {
            target_cwd: canonical,
            sanitized_target: target_sanitized,
        });
    }

    if let Some(session_dir) = session_project_dir {
        let session_base = Path::new(session_dir)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if session_base == target_sanitized {
            return Err(ProjectPurgeError::ActiveProject {
                target_cwd: canonical,
                sanitized_target: target_sanitized,
            });
        }
    }

    let sanitized = sanitize_path(&canonical);
    let project_dir = projects_dir.join(&sanitized);

    let (memory_status, memory_hint, memory_reason) = classify_memory(&project_dir).await;

    if include_memory && memory_status == ProjectPurgeMemoryStatus::External {
        return Err(ProjectPurgeError::ExternalMemoryIncludeRejected {
            external_hint: memory_hint,
            reason: memory_reason,
        });
    }

    let dirents = match fs::read_dir(&project_dir).await {
        Ok(d) => d,
        Err(_) => {
            return Err(ProjectPurgeError::ProjectDirMissing {
                path: project_dir.to_string_lossy().to_string(),
            });
        }
    };

    let should_include_memory = include_memory && memory_status == ProjectPurgeMemoryStatus::InProject;

    let mut to_archive = Vec::new();
    let mut to_skip = Vec::new();
    let mut total_known: i64 = 0;
    let mut any_unknown = false;

    let mut entries = dirents;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = entry.file_type().await.unwrap_or_else(|_| {
            // Fallback - will be classified as Other
            std::fs::FileType::from(std::fs::metadata(entry.path()).unwrap().file_type())
        });
        let kind = if ft.is_dir() {
            EntryKind::Directory
        } else if ft.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };
        let abs_path = entry.path();
        let is_memory = name == "memory";
        let size_bytes = compute_entry_size_bytes(&abs_path, kind).await;
        if size_bytes < 0 {
            any_unknown = true;
        } else {
            total_known += size_bytes;
        }
        let pe = ProjectPurgeEntry {
            name,
            abs_path,
            kind,
            size_bytes,
            is_memory,
        };
        if is_memory && !should_include_memory {
            to_skip.push(pe);
        } else {
            to_archive.push(pe);
        }
    }

    let stamp = timestamp_for_archive_dir(now);
    let archive_dir = config_home
        .join("backups")
        .join(format!("purge-{}-{}", stamp, generate_purge_token()))
        .join(&sanitized);

    let token = generate_purge_token();
    let plan = ProjectPurgePlan {
        token: token.clone(),
        created_at: now,
        target_cwd: canonical,
        sanitized_target: sanitized,
        original_project_dir: project_dir,
        memory_status,
        memory_external_hint: memory_hint,
        memory_external_reason: memory_reason,
        include_memory: should_include_memory,
        to_archive,
        to_skip,
        total_archive_bytes: if any_unknown { -1 } else { total_known },
        archive_dir,
    };

    if let Ok(mut store) = PLAN_STORE.lock() {
        store.insert(token, plan.clone());
    }
    Ok(plan)
}

/// Execute a purge plan by token.
pub async fn execute_project_purge_plan(
    token: &str,
    target_cwd_check: Option<&str>,
    original_cwd: &str,
    project_root: &str,
    session_project_dir: Option<&str>,
) -> std::result::Result<ProjectPurgeResult, ProjectPurgeError> {
    let now = current_time_ms();
    evict_expired_plans(now);

    let plan = {
        let mut store = PLAN_STORE.lock().map_err(|_| ProjectPurgeError::UnknownToken)?;
        match store.remove(token) {
            Some(p) => p,
            None => return Err(ProjectPurgeError::UnknownToken),
        }
    };

    if now - plan.created_at > PROJECT_PURGE_TOKEN_TTL_MS {
        return Err(ProjectPurgeError::ExpiredToken);
    }

    // Target double-check
    if let Some(check) = target_cwd_check {
        let canon = tokio::fs::canonicalize(check)
            .await
            .map_err(|e| ProjectPurgeError::InvalidTarget {
                target_cwd: check.to_string(),
                reason: format!("realpath failed: {}", e),
            })?
            .to_string_lossy()
            .to_string();
        if canon != plan.target_cwd {
            return Err(ProjectPurgeError::TokenTargetMismatch {
                expected: plan.target_cwd.clone(),
                got: canon,
            });
        }
    }

    // Re-run active project guard
    let target_sanitized = sanitize_path(&plan.target_cwd);
    let orig_sanitized = sanitize_path(original_cwd);
    let root_sanitized = sanitize_path(project_root);
    if target_sanitized == orig_sanitized || target_sanitized == root_sanitized {
        return Err(ProjectPurgeError::ActiveProject {
            target_cwd: plan.target_cwd.clone(),
            sanitized_target: target_sanitized,
        });
    }
    if let Some(sd) = session_project_dir {
        let sb = Path::new(sd)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if *sb == target_sanitized {
            return Err(ProjectPurgeError::ActiveProject {
                target_cwd: plan.target_cwd.clone(),
                sanitized_target: target_sanitized,
            });
        }
    }

    // Re-enumerate project dir
    let mut entries_reader = match fs::read_dir(&plan.original_project_dir).await {
        Ok(d) => d,
        Err(_) => {
            return Err(ProjectPurgeError::ProjectDirMissing {
                path: plan.original_project_dir.to_string_lossy().to_string(),
            });
        }
    };

    let mut result = ProjectPurgeResult {
        archived_entries: Vec::new(),
        skipped_entries: Vec::new(),
        errors: Vec::new(),
        archive_dir: plan.archive_dir.clone(),
        manifest_path: plan.archive_dir.join("purge-manifest.json"),
        total_archived_bytes: 0,
        project_dir_removed: false,
        phase_a_halted: false,
    };

    let mut archive_set: Vec<(String, PathBuf, EntryKind, i64)> = Vec::new();
    while let Ok(Some(entry)) = entries_reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = entry.file_type().await.unwrap_or_else(|_| {
            std::fs::metadata(entry.path()).unwrap().file_type()
        });
        let kind = if ft.is_dir() {
            EntryKind::Directory
        } else if ft.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        };
        if name == "memory" && !plan.include_memory {
            result.skipped_entries.push(SkippedEntry {
                name,
                kind: EntryKind::Directory,
                reason: "preserved-by-default".to_string(),
            });
            continue;
        }
        let abs_path = entry.path();
        let size_bytes = compute_entry_size_bytes(&abs_path, kind).await;
        archive_set.push((name, abs_path, kind, size_bytes));
    }

    // Phase A — copy entries into archive
    fs::create_dir_all(&plan.archive_dir).await.ok();
    let mut successfully_archived: Vec<(String, PathBuf, EntryKind, i64)> = Vec::new();

    for (name, abs_path, kind, size_bytes) in &archive_set {
        let dest = plan.archive_dir.join(name);
        match copy_recursive_no_symlink(abs_path, &dest).await {
            Ok(()) => {
                successfully_archived.push((name.clone(), abs_path.clone(), *kind, *size_bytes));
                if *size_bytes >= 0 {
                    result.total_archived_bytes += size_bytes;
                }
                result.archived_entries.push(ArchivedEntry {
                    name: name.clone(),
                    kind: *kind,
                    bytes: *size_bytes,
                });
            }
            Err(e) => {
                result.errors.push(PurgePhaseError {
                    phase: PurgePhase::Copy,
                    name: name.clone(),
                    message: e.to_string(),
                });
                result.phase_a_halted = true;
                // Cleanup partial
                fs::remove_dir_all(&dest).await.ok();
                break;
            }
        }
    }

    // Phase B — delete originals
    if !result.phase_a_halted {
        for (name, abs_path, _, _) in &successfully_archived {
            if let Err(e) = fs::remove_dir_all(abs_path).await {
                if let Err(e2) = fs::remove_file(abs_path).await {
                    result.errors.push(PurgePhaseError {
                        phase: PurgePhase::Delete,
                        name: name.clone(),
                        message: format!("{} / {}", e, e2),
                    });
                }
            }
        }
    }

    // Phase C — write manifest
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "purgedAt": chrono::Utc::now().to_rfc3339(),
        "targetCwd": plan.target_cwd,
        "sanitizedTarget": plan.sanitized_target,
        "originalProjectDir": plan.original_project_dir.to_string_lossy(),
        "includeMemory": plan.include_memory,
        "archivedEntries": result.archived_entries.iter().map(|e| {
            serde_json::json!({"name": e.name, "kind": format!("{:?}", e.kind), "bytes": e.bytes})
        }).collect::<Vec<_>>(),
        "skippedEntries": result.skipped_entries.iter().map(|e| {
            serde_json::json!({"name": e.name, "kind": format!("{:?}", e.kind), "reason": e.reason})
        }).collect::<Vec<_>>(),
        "errors": result.errors.iter().map(|e| {
            serde_json::json!({"phase": format!("{:?}", e.phase), "name": e.name, "message": e.message})
        }).collect::<Vec<_>>(),
        "totalArchivedBytes": result.total_archived_bytes,
        "phaseAHalted": result.phase_a_halted,
    });

    if let Err(e) = fs::write(
        &result.manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n",
    )
    .await
    {
        result.errors.push(PurgePhaseError {
            phase: PurgePhase::Manifest,
            name: "purge-manifest.json".to_string(),
            message: e.to_string(),
        });
    }

    // Phase D — remove empty project dir
    if !result.phase_a_halted {
        if let Ok(mut remaining) = fs::read_dir(&plan.original_project_dir).await {
            let has_entries = remaining.next_entry().await.ok().flatten().is_some();
            if !has_entries {
                if let Err(e) = fs::remove_dir_all(&plan.original_project_dir).await {
                    result.errors.push(PurgePhaseError {
                        phase: PurgePhase::Cleanup,
                        name: plan
                            .original_project_dir
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        message: e.to_string(),
                    });
                } else {
                    result.project_dir_removed = true;
                }
            }
        }
    }

    Ok(result)
}

/// Reset the plan store (for testing).
pub fn reset_project_purge_plan_store_for_testing() {
    if let Ok(mut store) = PLAN_STORE.lock() {
        store.clear();
    }
}

/// Get current time in milliseconds since epoch.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}
