//! Project inventory — read-only inventory of ~/.mossen/projects/.
//!
//! Translated from utils/projectInventory.ts

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryLocationStatus {
    InProject,
    External,
    Absent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInventoryEntry {
    pub sanitized_id: String,
    pub project_dir: String,
    pub inferred_cwd: String,
    pub inferred_cwd_confidence: String, // "high" | "low"
    pub session_jsonl_count: usize,
    pub sub_session_dir_count: usize,
    pub has_memory_dir: bool,
    pub memory_file_count: usize,
    pub memory_bytes: i64,
    pub total_bytes: i64,
    pub modified_ms: u64,
    pub stale: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInventoryResult {
    pub projects_dir: String,
    pub entries: Vec<ProjectInventoryEntry>,
    pub aggregate_bytes: i64,
    pub missing_projects_dir: bool,
    pub active_markers: ActiveProjectMarkers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProjectMarkers {
    pub original_cwd: String,
    pub project_root: String,
    pub session_project_dir: Option<String>,
    #[serde(skip)]
    pub active_sanitized: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSizeSummary {
    pub path: String,
    pub exists: bool,
    pub total_bytes: i64,
    pub entry_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStateSummary {
    pub status: MemoryLocationStatus,
    pub path: Option<String>,
    pub reason: String,
    pub file_count: usize,
    pub total_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct ActiveProjectStatus {
    pub active_markers: ActiveProjectMarkers,
    pub inventory: Option<ProjectInventoryEntry>,
    pub memory: MemoryStateSummary,
    pub purge_eligibility: PurgeEligibility,
    pub session_count: usize,
    pub caches: Vec<CacheSizeSummary>,
}

#[derive(Debug, Clone)]
pub struct PurgeEligibility {
    pub eligible: bool,
    pub reason: String,
}

/// Compute active project markers.
pub fn compute_active_markers(
    original_cwd: &str,
    project_root: &str,
    session_project_dir: Option<&str>,
) -> ActiveProjectMarkers {
    let mut active_sanitized = HashSet::new();
    active_sanitized.insert(sanitize_path(original_cwd));
    active_sanitized.insert(sanitize_path(project_root));
    if let Some(dir) = session_project_dir {
        if let Some(basename) = Path::new(dir).file_name() {
            active_sanitized.insert(basename.to_string_lossy().to_string());
        }
    }
    ActiveProjectMarkers {
        original_cwd: original_cwd.to_string(),
        project_root: project_root.to_string(),
        session_project_dir: session_project_dir.map(|s| s.to_string()),
        active_sanitized,
    }
}

/// Detect memory override from environment variables and settings.
pub fn detect_memory_override() -> Option<(String, String)> {
    if let Ok(v) = std::env::var("MOSSEN_COWORK_MEMORY_PATH_OVERRIDE") {
        if !v.is_empty() {
            return Some((v, "env.MOSSEN_COWORK_MEMORY_PATH_OVERRIDE".to_string()));
        }
    }
    if let Ok(v) = std::env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR") {
        if !v.is_empty() {
            return Some((v, "env.MOSSEN_CODE_REMOTE_MEMORY_DIR".to_string()));
        }
    }
    None
}

/// Recursive size walker — counts regular files only.
pub async fn walk_size(dir: &Path) -> (i64, i64) {
    let mut bytes: i64 = 0;
    let mut file_count: i64 = 0;

    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return (-1, -1),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let ft = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => return (-1, -1),
        };

        if ft.is_symlink() {
            continue;
        }

        let child = entry.path();
        if ft.is_dir() {
            let (sub_bytes, sub_count) = Box::pin(walk_size(&child)).await;
            if sub_bytes < 0 {
                return (-1, -1);
            }
            bytes += sub_bytes;
            file_count += sub_count;
        } else if ft.is_file() {
            match fs::metadata(&child).await {
                Ok(meta) => {
                    bytes += meta.len() as i64;
                    file_count += 1;
                }
                Err(_) => return (-1, -1),
            }
        }
    }

    (bytes, file_count)
}

/// Infer CWD from sanitized ID (pure decoration).
pub fn infer_cwd(sanitized_id: &str) -> (String, &'static str) {
    if !sanitized_id.starts_with('-') {
        return (sanitized_id.to_string(), "low");
    }
    let inferred = sanitized_id.replace('-', "/");
    let tail = sanitized_id.split('-').next_back().unwrap_or("");
    let looks_hashed = tail.len() >= 7 && tail.chars().all(|c| c.is_ascii_alphanumeric());
    (inferred, if looks_hashed { "low" } else { "high" })
}

/// Inventory a single project directory.
pub async fn inventory_project_dir(
    projects_dir: &str,
    sanitized_id: &str,
    active_markers: &ActiveProjectMarkers,
    stale_threshold_days: u64,
) -> Option<ProjectInventoryEntry> {
    let project_dir = PathBuf::from(projects_dir).join(sanitized_id);
    let mut entries = match fs::read_dir(&project_dir).await {
        Ok(e) => e,
        Err(_) => return None,
    };

    let mut session_jsonl_count = 0usize;
    let mut sub_session_dir_count = 0usize;
    let mut has_memory_dir = false;

    let mut dir_entries = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        dir_entries.push(entry);
    }

    for entry in &dir_entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = match entry.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if name == "memory" && ft.is_dir() {
            has_memory_dir = true;
        } else if ft.is_file() && name.ends_with(".jsonl") {
            session_jsonl_count += 1;
        } else if ft.is_dir() {
            sub_session_dir_count += 1;
        }
    }

    let (memory_bytes, memory_file_count) = if has_memory_dir {
        let mem_path = project_dir.join("memory");
        let (bytes, count) = walk_size(&mem_path).await;
        (
            if bytes < 0 { 0i64 } else { bytes },
            if count < 0 { 0usize } else { count as usize },
        )
    } else {
        (0, 0)
    };

    let (total_bytes, _) = walk_size(&project_dir).await;

    let modified_ms = fs::metadata(&project_dir)
        .await
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let (inferred_cwd, confidence) = infer_cwd(sanitized_id);
    let stale = if modified_ms > 0 {
        let age_days = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            - modified_ms)
            / (24 * 60 * 60 * 1000);
        age_days >= stale_threshold_days
    } else {
        false
    };
    let active = active_markers.active_sanitized.contains(sanitized_id);

    Some(ProjectInventoryEntry {
        sanitized_id: sanitized_id.to_string(),
        project_dir: project_dir.to_string_lossy().to_string(),
        inferred_cwd,
        inferred_cwd_confidence: confidence.to_string(),
        session_jsonl_count,
        sub_session_dir_count,
        has_memory_dir,
        memory_file_count,
        memory_bytes,
        total_bytes,
        modified_ms,
        stale,
        active,
    })
}

/// Build the full project inventory.
pub async fn build_project_inventory(
    projects_dir: &str,
    active_markers: &ActiveProjectMarkers,
    stale_threshold_days: u64,
) -> ProjectInventoryResult {
    let mut entries_list = Vec::new();
    let dir_path = Path::new(projects_dir);

    let dir_entries = match fs::read_dir(dir_path).await {
        Ok(mut e) => {
            let mut dirs = Vec::new();
            while let Ok(Some(entry)) = e.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        dirs.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
            dirs
        }
        Err(_) => {
            return ProjectInventoryResult {
                projects_dir: projects_dir.to_string(),
                entries: Vec::new(),
                aggregate_bytes: 0,
                missing_projects_dir: true,
                active_markers: active_markers.clone(),
            };
        }
    };

    for name in &dir_entries {
        if let Some(inv) =
            inventory_project_dir(projects_dir, name, active_markers, stale_threshold_days).await
        {
            entries_list.push(inv);
        }
    }

    entries_list.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));

    let mut aggregate_bytes: i64 = 0;
    let mut any_unknown = false;
    for e in &entries_list {
        if e.total_bytes < 0 {
            any_unknown = true;
        } else {
            aggregate_bytes += e.total_bytes;
        }
    }

    ProjectInventoryResult {
        projects_dir: projects_dir.to_string(),
        entries: entries_list,
        aggregate_bytes: if any_unknown { -1 } else { aggregate_bytes },
        missing_projects_dir: false,
        active_markers: active_markers.clone(),
    }
}

/// Describe memory state for a project directory.
pub async fn describe_memory_state(candidate_in_project_dir: &str) -> MemoryStateSummary {
    if let Some((hint, reason)) = detect_memory_override() {
        return MemoryStateSummary {
            status: MemoryLocationStatus::External,
            path: Some(hint),
            reason,
            file_count: 0,
            total_bytes: 0,
        };
    }

    let mem_dir = PathBuf::from(candidate_in_project_dir).join("memory");
    match fs::metadata(&mem_dir).await {
        Ok(meta) if meta.is_dir() => {}
        _ => {
            return MemoryStateSummary {
                status: MemoryLocationStatus::Absent,
                path: None,
                reason: "absent".to_string(),
                file_count: 0,
                total_bytes: 0,
            };
        }
    }

    let (bytes, file_count) = walk_size(&mem_dir).await;
    MemoryStateSummary {
        status: MemoryLocationStatus::InProject,
        path: Some(mem_dir.to_string_lossy().to_string()),
        reason: "default-in-project".to_string(),
        file_count: if file_count < 0 {
            0
        } else {
            file_count as usize
        },
        total_bytes: if bytes < 0 { -1 } else { bytes },
    }
}

/// Summarize a cache directory.
pub async fn summarize_cache_dir(path: &str) -> CacheSizeSummary {
    let dir_path = Path::new(path);
    if fs::metadata(dir_path).await.is_err() {
        return CacheSizeSummary {
            path: path.to_string(),
            exists: false,
            total_bytes: 0,
            entry_count: 0,
        };
    }

    let entry_count = match fs::read_dir(dir_path).await {
        Ok(mut e) => {
            let mut count: i64 = 0;
            while e.next_entry().await.ok().flatten().is_some() {
                count += 1;
            }
            count
        }
        Err(_) => -1,
    };

    let (bytes, _) = walk_size(dir_path).await;
    CacheSizeSummary {
        path: path.to_string(),
        exists: true,
        total_bytes: bytes,
        entry_count,
    }
}

/// Sanitize a path for use as directory name.
fn sanitize_path(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

/// 对应 TS `describeActiveProjectStatus`：扫描 ~/.mossen/projects 目录，
/// 描述当前项目的 inventory 状态。
///
/// 复用模块顶部已有的 [`ActiveProjectStatus`] 类型。
pub async fn describe_active_project_status(cwd: &str) -> ActiveProjectStatus {
    let mut session_count = 0usize;
    if let Some(home) = dirs::home_dir() {
        let project_dir = home
            .join(".mossen")
            .join("projects")
            .join(sanitize_path(cwd));
        if let Ok(mut entries) = fs::read_dir(&project_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    session_count += 1;
                }
            }
        }
    }
    ActiveProjectStatus {
        active_markers: ActiveProjectMarkers {
            original_cwd: cwd.to_string(),
            project_root: cwd.to_string(),
            session_project_dir: None,
            active_sanitized: HashSet::new(),
        },
        inventory: None,
        memory: MemoryStateSummary {
            status: MemoryLocationStatus::Absent,
            path: None,
            reason: String::new(),
            file_count: 0,
            total_bytes: 0,
        },
        purge_eligibility: PurgeEligibility {
            eligible: false,
            reason: String::new(),
        },
        session_count,
        caches: Vec::new(),
    }
}
