//! Filesystem-based git state reading — translated from utils/git/gitFilesystem.ts
//!
//! This module provides functions to read git state from the filesystem
//! without spawning git subprocesses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use tokio::fs;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;

/// Clear cached git dir resolutions
pub fn clear_resolve_git_dir_cache() {
    RESOLVE_GIT_DIR_CACHE.lock().unwrap().clear();
}

/// Resolve the actual .git directory for a repo.
/// Handles worktrees/submodules where .git is a file containing `gitdir: <path>`.
/// Memoized per start_path.
pub async fn resolve_git_dir(start_path: Option<&str>) -> Result<Option<String>> {
    let cwd = start_path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let cwd_str = cwd.to_string_lossy().to_string();

    let cache = RESOLVE_GIT_DIR_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(&cwd_str) {
        return Ok(Some(cached.clone()));
    }
    drop(cache);

    let root = crate::git::find_git_root(&cwd_str).await?;
    if let Some(root) = root {
        let git_path = root.join(".git");
        let metadata = fs::metadata(&git_path).await;

        match metadata {
            Ok(meta) if meta.is_file() => {
                // Worktree or submodule: .git is a file with `gitdir: <path>`
                let content = tokio::fs::read_to_string(&git_path).await?;
                let content = content.trim();
                if content.starts_with("gitdir:") {
                    let raw_dir = content["gitdir:".len()..].trim();
                    let resolved = root.join(raw_dir);
                    let resolved_str = resolved.to_string_lossy().to_string();
                    RESOLVE_GIT_DIR_CACHE.lock().unwrap().insert(cwd_str, resolved_str.clone());
                    return Ok(Some(resolved_str));
                }
            }
            Ok(meta) if meta.is_dir() => {
                // Regular repo: .git is a directory
                let git_path_str = git_path.to_string_lossy().to_string();
                RESOLVE_GIT_DIR_CACHE.lock().unwrap().insert(cwd_str, git_path_str.clone());
                return Ok(Some(git_path_str));
            }
            _ => {}
        }
    }

    RESOLVE_GIT_DIR_CACHE.lock().unwrap().insert(cwd_str, String::new());
    Ok(None)
}

/// Cache for git dir resolutions
static RESOLVE_GIT_DIR_CACHE: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Validate that a ref/branch name read from .git/ is safe to use in path joins
pub fn is_safe_ref_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.starts_with('/') {
        return false;
    }
    if name.contains("..") {
        return false;
    }
    // Reject single-dot and empty path components
    if name.split('/').any(|c| c.is_empty() || c == ".") {
        return false;
    }
    // Allowlist-only: alphanumerics, /, ., _, +, -, @
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '.' || c == '_' || c == '+' || c == '-' || c == '@')
}

/// Validate that a string is a git SHA: 40 hex chars (SHA-1) or 64 hex chars (SHA-256)
pub fn is_valid_git_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// HEAD type
#[derive(Debug, Clone)]
pub enum HeadType {
    Branch { name: String },
    Detached { sha: String },
}

/// Parse .git/HEAD to determine current branch or detached SHA
pub async fn read_git_head(git_dir: &str) -> Result<Option<HeadType>> {
    let head_path = Path::new(git_dir).join("HEAD");
    let content = tokio::fs::read_to_string(&head_path).await?;
    let content = content.trim();

    if content.starts_with("ref:") {
        let ref_path = content["ref:".len()..].trim();
        if ref_path.starts_with("refs/heads/") {
            let name = ref_path["refs/heads/".len()..].to_string();
            if !is_safe_ref_name(&name) {
                return Ok(None);
            }
            return Ok(Some(HeadType::Branch { name }));
        }
        // Unusual symref — try to resolve
        if !is_safe_ref_name(ref_path) {
            return Ok(None);
        }
        if let Some(sha) = resolve_ref(git_dir, ref_path).await? {
            return Ok(Some(HeadType::Detached { sha }));
        }
        return Ok(Some(HeadType::Detached { sha: String::new() }));
    }

    // Raw SHA (detached HEAD)
    if !is_valid_git_sha(content) {
        return Ok(None);
    }
    Ok(Some(HeadType::Detached { sha: content.to_string() }))
}

/// Resolve a git ref (e.g. `refs/heads/main`) to a commit SHA
pub async fn resolve_ref(git_dir: &str, reference: &str) -> Result<Option<String>> {
    // Try loose ref file
    if let Some(sha) = resolve_ref_in_dir(git_dir, reference).await? {
        return Ok(Some(sha));
    }

    // For worktrees: try the common gitdir
    if let Some(common_dir) = get_common_dir(git_dir).await? {
        if common_dir != git_dir {
            return resolve_ref_in_dir(&common_dir, reference).await;
        }
    }

    Ok(None)
}

async fn resolve_ref_in_dir(dir: &str, reference: &str) -> Result<Option<String>> {
    let ref_path = Path::new(dir).join(reference);

    // Try loose ref file
    if let Ok(content) = tokio::fs::read_to_string(&ref_path).await {
        let content = content.trim();
        if content.starts_with("ref:") {
            let target = content["ref:".len()..].trim();
            if !is_safe_ref_name(target) {
                return Ok(None);
            }
            return resolve_ref(dir, target).await;
        }
        if is_valid_git_sha(content) {
            return Ok(Some(content.to_string()));
        }
        return Ok(None);
    }

    // Try packed-refs
    let packed_refs_path = Path::new(dir).join("packed-refs");
    if let Ok(packed) = tokio::fs::read_to_string(&packed_refs_path).await {
        for line in packed.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            if let Some(space_idx) = line.find(' ') {
                let sha = &line[..space_idx];
                let refname = &line[space_idx + 1..];
                if refname == reference && is_valid_git_sha(sha) {
                    return Ok(Some(sha.to_string()));
                }
            }
        }
    }

    Ok(None)
}

/// Read the `commondir` file to find the shared git directory
pub async fn get_common_dir(git_dir: &str) -> Result<Option<String>> {
    let commondir_path = Path::new(git_dir).join("commondir");
    match tokio::fs::read_to_string(&commondir_path).await {
        Ok(content) => {
            let content = content.trim();
            let resolved = Path::new(git_dir).join(content);
            Ok(Some(resolved.to_string_lossy().to_string()))
        }
        Err(_) => Ok(None),
    }
}

/// Read the HEAD SHA for an arbitrary directory
pub async fn get_head_for_dir(cwd: &str) -> Result<Option<String>> {
    let git_dir = resolve_git_dir(Some(cwd)).await?;
    if let Some(git_dir) = git_dir {
        if let Some(head) = read_git_head(&git_dir).await? {
            return match head {
                HeadType::Branch { name } => {
                    resolve_ref(&git_dir, &format!("refs/heads/{}", name)).await
                }
                HeadType::Detached { sha } => Ok(Some(sha)),
            };
        }
    }
    Ok(None)
}

/// Check if we're in a shallow clone
pub async fn is_shallow_clone() -> Result<bool> {
    let git_dir = resolve_git_dir(None).await?;
    if let Some(git_dir) = git_dir {
        let common_dir = get_common_dir(&git_dir).await?.unwrap_or(git_dir);
        let shallow_path = Path::new(&common_dir).join("shallow");
        return Ok(tokio::fs::metadata(&shallow_path).await.is_ok());
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_ref_name() {
        assert!(is_safe_ref_name("main"));
        assert!(is_safe_ref_name("feature/foo"));
        assert!(is_safe_ref_name("release-1.2.3+build"));
        assert!(!is_safe_ref_name(""));
        assert!(!is_safe_ref_name("-feature"));
        assert!(!is_safe_ref_name("foo/bar/.."));
    }

    #[test]
    fn test_is_valid_git_sha() {
        assert!(is_valid_git_sha("a".repeat(40).as_str()));
        assert!(is_valid_git_sha("a".repeat(64).as_str()));
        assert!(!is_valid_git_sha("short"));
        assert!(!is_valid_git_sha("g".repeat(40).as_str())); // 'g' is not hex
    }
}

// =============================================================================
// 与 TS `git/gitFilesystem.ts` 对齐的缓存查询入口。
// =============================================================================

#[derive(Debug, Clone, Default)]
struct GitCacheEntry {
    pub branch: Option<String>,
    pub head: Option<String>,
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
}

static GIT_CACHE: once_cell::sync::Lazy<std::sync::Mutex<GitCacheEntry>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(GitCacheEntry::default()));

/// 对应 TS `getCachedBranch`：返回上次解析的 branch（如有）。
pub fn get_cached_branch() -> Option<String> {
    GIT_CACHE.lock().unwrap().branch.clone()
}

/// 对应 TS `getCachedHead`：返回上次解析的 HEAD SHA。
pub fn get_cached_head() -> Option<String> {
    GIT_CACHE.lock().unwrap().head.clone()
}

/// 对应 TS `getCachedRemoteUrl`：返回上次解析的 remote URL。
pub fn get_cached_remote_url() -> Option<String> {
    GIT_CACHE.lock().unwrap().remote_url.clone()
}

/// 对应 TS `getCachedDefaultBranch`：返回上次解析的默认分支。
pub fn get_cached_default_branch() -> Option<String> {
    GIT_CACHE.lock().unwrap().default_branch.clone()
}

/// 对应 TS `resetGitFileWatcher`：清空缓存并停止 file watcher。
pub fn reset_git_file_watcher() {
    *GIT_CACHE.lock().unwrap() = GitCacheEntry::default();
}
