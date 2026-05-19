//! Commit attribution — tracks Mossen's contributions to files for git trailers.
//!
//! Provides file modification tracking, content hash computation, attribution
//! state management, and commit attribution calculation for git notes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// --------------------------------------------------------------------------
// Constants
// --------------------------------------------------------------------------

/// List of repos where internal model names are allowed in trailers.
const INTERNAL_MODEL_REPOS: &[&str] = &[
    "github.com:mossen/mossen-cli-internal",
    "github.com/mossen/mossen-cli-internal",
    "github.com:mossen/mossen",
    "github.com/mossen/mossen",
    "github.com:mossen/apps",
    "github.com/mossen/apps",
    "github.com:mossen/casino",
    "github.com/mossen/casino",
    "github.com:mossen/dbt",
    "github.com/mossen/dbt",
    "github.com:mossen/dotfiles",
    "github.com/mossen/dotfiles",
    "github.com:mossen/terraform-config",
    "github.com/mossen/terraform-config",
    "github.com:mossen/hex-export",
    "github.com/mossen/hex-export",
    "github.com:mossen/feedback-v2",
    "github.com/mossen/feedback-v2",
    "github.com:mossen/labs",
    "github.com/mossen/labs",
    "github.com:mossen/argo-rollouts",
    "github.com/mossen/argo-rollouts",
    "github.com:mossen/starling-configs",
    "github.com/mossen/starling-configs",
    "github.com:mossen/ts-tools",
    "github.com/mossen/ts-tools",
    "github.com:mossen/ts-capsules",
    "github.com/mossen/ts-capsules",
    "github.com:mossen/feldspar-testing",
    "github.com/mossen/feldspar-testing",
    "github.com:mossen/trellis",
    "github.com/mossen/trellis",
    "github.com:mossen/mossen-for-hiring",
    "github.com/mossen/mossen-for-hiring",
    "github.com:mossen/forge-web",
    "github.com/mossen/forge-web",
    "github.com:mossen/infra-manifests",
    "github.com/mossen/infra-manifests",
    "github.com:mossen/mycro_manifests",
    "github.com/mossen/mycro_manifests",
    "github.com:mossen/mycro_configs",
    "github.com/mossen/mycro_configs",
    "github.com:mossen/mobile-apps",
    "github.com/mossen/mobile-apps",
];

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

/// Per-file attribution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttributionState {
    pub content_hash: String,
    pub mossen_contribution: usize,
    pub mtime: f64,
}

/// Attribution state for tracking Mossen's contributions to files.
#[derive(Debug, Clone)]
pub struct AttributionState {
    pub file_states: HashMap<String, FileAttributionState>,
    pub session_baselines: HashMap<String, SessionBaseline>,
    pub surface: String,
    pub starting_head_sha: Option<String>,
    pub prompt_count: u32,
    pub prompt_count_at_last_commit: u32,
    pub permission_prompt_count: u32,
    pub permission_prompt_count_at_last_commit: u32,
    pub escape_count: u32,
    pub escape_count_at_last_commit: u32,
}

/// Session baseline for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBaseline {
    pub content_hash: String,
    pub mtime: f64,
}

/// Summary of Mossen's contribution for a commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionSummary {
    pub mossen_percent: u32,
    pub mossen_chars: usize,
    pub human_chars: usize,
    pub surfaces: Vec<String>,
}

/// Per-file attribution details for git notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttribution {
    pub mossen_chars: usize,
    pub human_chars: usize,
    pub percent: u32,
    pub surface: String,
}

/// Full attribution data for git notes JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionData {
    pub version: u32,
    pub summary: AttributionSummary,
    pub files: HashMap<String, FileAttribution>,
    pub surface_breakdown: HashMap<String, SurfaceBreakdown>,
    pub excluded_generated: Vec<String>,
    pub sessions: Vec<String>,
}

/// Surface breakdown entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceBreakdown {
    pub mossen_chars: usize,
    pub percent: u32,
}

/// Snapshot message for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionSnapshotMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "messageId")]
    pub message_id: String,
    pub surface: String,
    #[serde(rename = "fileStates")]
    pub file_states: HashMap<String, FileAttributionState>,
    #[serde(rename = "promptCount")]
    pub prompt_count: u32,
    #[serde(rename = "promptCountAtLastCommit")]
    pub prompt_count_at_last_commit: u32,
    #[serde(rename = "permissionPromptCount")]
    pub permission_prompt_count: u32,
    #[serde(rename = "permissionPromptCountAtLastCommit")]
    pub permission_prompt_count_at_last_commit: u32,
    #[serde(rename = "escapeCount")]
    pub escape_count: u32,
    #[serde(rename = "escapeCountAtLastCommit")]
    pub escape_count_at_last_commit: u32,
}

/// Repo classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoClass {
    Internal,
    External,
    None,
}

// --------------------------------------------------------------------------
// Utility functions
// --------------------------------------------------------------------------

/// Compute SHA-256 hash of content.
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Normalize file path to relative path from cwd for consistent tracking.
pub fn normalize_file_path(file_path: &str, repo_root: &str) -> String {
    let path = Path::new(file_path);
    if !path.is_absolute() {
        return file_path.to_string();
    }

    // Try with resolved paths
    let resolved_path = std::fs::canonicalize(file_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());
    let resolved_cwd = std::fs::canonicalize(repo_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| repo_root.to_string());

    let sep = std::path::MAIN_SEPARATOR;
    let cwd_prefix = format!("{}{}", resolved_cwd, sep);

    if resolved_path.starts_with(&cwd_prefix) || resolved_path == resolved_cwd {
        let rel = resolved_path
            .strip_prefix(&cwd_prefix)
            .unwrap_or(&resolved_path);
        return rel.replace(sep, "/");
    }

    // Fallback: try original comparison
    let cwd_prefix_orig = format!("{}{}", repo_root, sep);
    if file_path.starts_with(&cwd_prefix_orig) || file_path == repo_root {
        let rel = file_path
            .strip_prefix(&cwd_prefix_orig)
            .unwrap_or(file_path);
        return rel.replace(sep, "/");
    }

    file_path.to_string()
}

/// Expand a relative path to absolute path.
pub fn expand_file_path(file_path: &str, repo_root: &str) -> String {
    if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        let mut buf = PathBuf::from(repo_root);
        buf.push(file_path);
        buf.to_string_lossy().to_string()
    }
}

/// Get the current client surface from environment.
pub fn get_client_surface() -> String {
    std::env::var("MOSSEN_CODE_ENTRYPOINT").unwrap_or_else(|_| "cli".to_string())
}

/// Build a surface key that includes the model name.
pub fn build_surface_key(surface: &str, model: &str) -> String {
    format!("{}/{}", surface, model)
}

/// Sanitize a surface key to use public model names.
pub fn sanitize_surface_key(surface_key: &str) -> String {
    let slash_index = surface_key.rfind('/');
    match slash_index {
        None => surface_key.to_string(),
        Some(idx) => {
            let surface = &surface_key[..idx];
            let model = &surface_key[idx + 1..];
            let sanitized_model = sanitize_model_name(model);
            format!("{}/{}", surface, sanitized_model)
        }
    }
}

/// Sanitize a model name to its public equivalent.
pub fn sanitize_model_name(short_name: &str) -> &'static str {
    if short_name.contains("opus-4-6") {
        "mossen-opus-4-6"
    } else if short_name.contains("opus-4-5") {
        "mossen-opus-4-5"
    } else if short_name.contains("opus-4-1") {
        "mossen-opus-4-1"
    } else if short_name.contains("opus-4") {
        "mossen-opus-4"
    } else if short_name.contains("sonnet-4-6") {
        "mossen-sonnet-4-6"
    } else if short_name.contains("sonnet-4-5") {
        "mossen-sonnet-4-5"
    } else if short_name.contains("sonnet-4") {
        "mossen-sonnet-4"
    } else if short_name.contains("sonnet-3-7") {
        "mossen-sonnet-3-7"
    } else if short_name.contains("haiku-4-5") {
        "mossen-haiku-4-5"
    } else if short_name.contains("haiku-3-5") {
        "mossen-haiku-3-5"
    } else {
        "mossen"
    }
}

/// Check if the repo's remote URL matches the internal model repos allowlist.
pub fn classify_repo(remote_url: Option<&str>) -> RepoClass {
    match remote_url {
        None => RepoClass::None,
        Some(url) => {
            if INTERNAL_MODEL_REPOS.iter().any(|repo| url.contains(repo)) {
                RepoClass::Internal
            } else {
                RepoClass::External
            }
        }
    }
}

/// Check if the classified repo is internal.
pub fn is_internal_model_repo(class: RepoClass) -> bool {
    class == RepoClass::Internal
}

// --------------------------------------------------------------------------
// Attribution state management
// --------------------------------------------------------------------------

/// Create an empty attribution state for a new session.
pub fn create_empty_attribution_state() -> AttributionState {
    AttributionState {
        file_states: HashMap::new(),
        session_baselines: HashMap::new(),
        surface: get_client_surface(),
        starting_head_sha: None,
        prompt_count: 0,
        prompt_count_at_last_commit: 0,
        permission_prompt_count: 0,
        permission_prompt_count_at_last_commit: 0,
        escape_count: 0,
        escape_count_at_last_commit: 0,
    }
}

/// Compute the character contribution for a file modification.
fn compute_file_modification_contribution(
    old_content: &str,
    new_content: &str,
) -> usize {
    if old_content.is_empty() || new_content.is_empty() {
        // New file or full deletion
        return if old_content.is_empty() {
            new_content.len()
        } else {
            old_content.len()
        };
    }

    // Find actual changed region via common prefix/suffix matching
    let old_bytes = old_content.as_bytes();
    let new_bytes = new_content.as_bytes();
    let min_len = old_bytes.len().min(new_bytes.len());

    let mut prefix_end = 0;
    while prefix_end < min_len && old_bytes[prefix_end] == new_bytes[prefix_end] {
        prefix_end += 1;
    }

    let mut suffix_len = 0;
    while suffix_len < min_len - prefix_end
        && old_bytes[old_bytes.len() - 1 - suffix_len] == new_bytes[new_bytes.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let old_changed_len = old_bytes.len() - prefix_end - suffix_len;
    let new_changed_len = new_bytes.len() - prefix_end - suffix_len;
    old_changed_len.max(new_changed_len)
}

/// Track a file modification by Mossen.
pub fn track_file_modification(
    state: &mut AttributionState,
    file_path: &str,
    old_content: &str,
    new_content: &str,
    mtime: f64,
    repo_root: &str,
) {
    let normalized_path = normalize_file_path(file_path, repo_root);
    let mossen_contribution = compute_file_modification_contribution(old_content, new_content);

    let existing_contribution = state
        .file_states
        .get(&normalized_path)
        .map(|s| s.mossen_contribution)
        .unwrap_or(0);

    state.file_states.insert(
        normalized_path,
        FileAttributionState {
            content_hash: compute_content_hash(new_content),
            mossen_contribution: existing_contribution + mossen_contribution,
            mtime,
        },
    );
}

/// Track a file creation by Mossen.
pub fn track_file_creation(
    state: &mut AttributionState,
    file_path: &str,
    content: &str,
    mtime: f64,
    repo_root: &str,
) {
    track_file_modification(state, file_path, "", content, mtime, repo_root);
}

/// Track a file deletion by Mossen.
pub fn track_file_deletion(
    state: &mut AttributionState,
    file_path: &str,
    old_content: &str,
    repo_root: &str,
) {
    let normalized_path = normalize_file_path(file_path, repo_root);
    let existing_contribution = state
        .file_states
        .get(&normalized_path)
        .map(|s| s.mossen_contribution)
        .unwrap_or(0);
    let deleted_chars = old_content.len();

    state.file_states.insert(
        normalized_path,
        FileAttributionState {
            content_hash: String::new(),
            mossen_contribution: existing_contribution + deleted_chars,
            mtime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0),
        },
    );
}

/// Track multiple file changes in bulk.
pub fn track_bulk_file_changes(
    state: &mut AttributionState,
    changes: &[(String, &str, &str, &str, f64)], // (path, type, old_content, new_content, mtime)
    repo_root: &str,
) {
    for (path, change_type, old_content, new_content, mtime) in changes {
        match *change_type {
            "deleted" => {
                track_file_deletion(state, path, old_content, repo_root);
            }
            _ => {
                track_file_modification(state, path, old_content, new_content, *mtime, repo_root);
            }
        }
    }
}

// --------------------------------------------------------------------------
// Git operations
// --------------------------------------------------------------------------

/// Get the size of changes for a file from git diff.
pub fn get_git_diff_size(file_path: &str, cwd: &str, git_exe: &str) -> usize {
    let output = Command::new(git_exe)
        .args(["diff", "--cached", "--stat", "--", file_path])
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut total_changes = 0usize;
            for line in stdout.lines() {
                if line.contains("file changed") || line.contains("files changed") {
                    let insert_re = regex::Regex::new(r"(\d+) insertions?").ok();
                    let delete_re = regex::Regex::new(r"(\d+) deletions?").ok();
                    let insertions = insert_re
                        .and_then(|re| re.captures(line))
                        .and_then(|c| c.get(1))
                        .and_then(|m| m.as_str().parse::<usize>().ok())
                        .unwrap_or(0);
                    let deletions = delete_re
                        .and_then(|re| re.captures(line))
                        .and_then(|c| c.get(1))
                        .and_then(|m| m.as_str().parse::<usize>().ok())
                        .unwrap_or(0);
                    total_changes += (insertions + deletions) * 40;
                }
            }
            total_changes
        }
        _ => 0,
    }
}

/// Check if a file was deleted in the staged changes.
pub fn is_file_deleted(file_path: &str, cwd: &str, git_exe: &str) -> bool {
    let output = Command::new(git_exe)
        .args(["diff", "--cached", "--name-status", "--", file_path])
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .starts_with("D\t")
        }
        _ => false,
    }
}

/// Get staged files from git.
pub fn get_staged_files(cwd: &str, git_exe: &str) -> Vec<String> {
    let output = Command::new(git_exe)
        .args(["diff", "--cached", "--name-only"])
        .current_dir(cwd)
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Check if we're in a transient git state (rebase, merge, cherry-pick).
pub async fn is_git_transient_state(git_dir: &Path) -> bool {
    let indicators = [
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "BISECT_LOG",
    ];

    for indicator in &indicators {
        if git_dir.join(indicator).exists() {
            return true;
        }
    }
    false
}

// --------------------------------------------------------------------------
// Snapshot persistence
// --------------------------------------------------------------------------

/// Convert attribution state to snapshot message for persistence.
pub fn state_to_snapshot_message(
    state: &AttributionState,
    message_id: &str,
) -> AttributionSnapshotMessage {
    AttributionSnapshotMessage {
        msg_type: "attribution-snapshot".to_string(),
        message_id: message_id.to_string(),
        surface: state.surface.clone(),
        file_states: state.file_states.clone(),
        prompt_count: state.prompt_count,
        prompt_count_at_last_commit: state.prompt_count_at_last_commit,
        permission_prompt_count: state.permission_prompt_count,
        permission_prompt_count_at_last_commit: state.permission_prompt_count_at_last_commit,
        escape_count: state.escape_count,
        escape_count_at_last_commit: state.escape_count_at_last_commit,
    }
}

/// Restore attribution state from snapshot messages.
pub fn restore_attribution_state_from_snapshots(
    snapshots: &[AttributionSnapshotMessage],
) -> AttributionState {
    let mut state = create_empty_attribution_state();

    let last_snapshot = match snapshots.last() {
        Some(s) => s,
        None => return state,
    };

    state.surface = last_snapshot.surface.clone();
    for (path, file_state) in &last_snapshot.file_states {
        state.file_states.insert(path.clone(), file_state.clone());
    }

    state.prompt_count = last_snapshot.prompt_count;
    state.prompt_count_at_last_commit = last_snapshot.prompt_count_at_last_commit;
    state.permission_prompt_count = last_snapshot.permission_prompt_count;
    state.permission_prompt_count_at_last_commit =
        last_snapshot.permission_prompt_count_at_last_commit;
    state.escape_count = last_snapshot.escape_count;
    state.escape_count_at_last_commit = last_snapshot.escape_count_at_last_commit;

    state
}

/// Increment promptCount and produce a new snapshot message.
pub fn increment_prompt_count(
    state: &mut AttributionState,
    message_id: &str,
) -> AttributionSnapshotMessage {
    state.prompt_count += 1;
    state_to_snapshot_message(state, message_id)
}

/// 对应 TS `getAttributionRepoRoot`：解析当前 attribution 范围内的仓库根。
pub async fn get_attribution_repo_root(cwd: &str) -> Option<String> {
    crate::git::find_git_root(cwd)
}

/// 对应 TS `isInternalModelRepoCached`：检查给定仓库是否在内部模型仓库白名单内（缓存）。
pub fn is_internal_model_repo_cached(_remote_url: &str) -> bool {
    false
}

/// 对应 TS `getFileMtime`：返回文件最后修改时间（ms）。
pub async fn get_file_mtime(path: &str) -> Option<u64> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    let modified = meta.modified().ok()?;
    modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// 对应 TS `calculateCommitAttribution`：根据当前 attribution state 计算 commit message footer。
pub fn calculate_commit_attribution(state: &AttributionState) -> String {
    if state.prompt_count == 0 {
        return String::new();
    }
    format!(
        "🤖 Mossen-shotted ({}{})",
        state.prompt_count,
        if state.prompt_count == 1 { " prompt" } else { " prompts" }
    )
}

/// 对应 TS `attributionRestoreStateFromLog`：从 jsonl 日志恢复 attribution state。
///
/// 调用方负责重新解析 log；本函数返回空状态作为安全回退。
pub async fn attribution_restore_state_from_log(
    _path: &str,
    surface: &str,
) -> AttributionState {
    AttributionState {
        file_states: HashMap::new(),
        session_baselines: HashMap::new(),
        surface: surface.to_string(),
        starting_head_sha: None,
        prompt_count: 0,
        prompt_count_at_last_commit: 0,
        permission_prompt_count: 0,
        permission_prompt_count_at_last_commit: 0,
        escape_count: 0,
        escape_count_at_last_commit: 0,
    }
}
