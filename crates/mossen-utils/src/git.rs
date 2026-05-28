//! Git integration utilities.
//!
//! Provides git root discovery, repository state inspection, remote URL
//! normalization, file status tracking, and git state preservation for
//! issue submission.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::Serialize;
use sha2::{Digest, Sha256};

// --------------------------------------------------------------------------
// LRU Cache for git root lookups
// --------------------------------------------------------------------------

/// Simple LRU cache implementation for git roots.
mod lru_cache {
    use std::collections::HashMap;

    pub struct LruCache<K, V> {
        map: HashMap<K, V>,
        order: Vec<K>,
        capacity: usize,
    }

    impl<K: Clone + Eq + std::hash::Hash, V: Clone> LruCache<K, V> {
        pub fn new(capacity: usize) -> Self {
            Self {
                map: HashMap::new(),
                order: Vec::new(),
                capacity,
            }
        }

        pub fn get(&self, key: &K) -> Option<&V> {
            self.map.get(key)
        }

        pub fn insert(&mut self, key: K, value: V) {
            if self.map.contains_key(&key) {
                self.order.retain(|k| k != &key);
            } else if self.order.len() >= self.capacity {
                if let Some(oldest) = self.order.first().cloned() {
                    self.map.remove(&oldest);
                    self.order.remove(0);
                }
            }
            self.order.push(key.clone());
            self.map.insert(key, value);
        }

        pub fn clear(&mut self) {
            self.map.clear();
            self.order.clear();
        }
    }
}

use lru_cache::LruCache;

static GIT_ROOT_CACHE: Lazy<Mutex<LruCache<String, Option<String>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(50)));

static CANONICAL_ROOT_CACHE: Lazy<Mutex<LruCache<String, String>>> =
    Lazy::new(|| Mutex::new(LruCache::new(50)));

/// Find the git root by walking up the directory tree.
/// Looks for a .git directory or file (worktrees/submodules use a file).
/// Returns the directory containing .git, or None if not found.
///
/// Memoized per startPath with an LRU cache (max 50 entries).
pub fn find_git_root(start_path: &str) -> Option<String> {
    {
        let cache = GIT_ROOT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&start_path.to_string()) {
            return cached.clone();
        }
    }

    let result = find_git_root_impl(start_path);

    {
        let mut cache = GIT_ROOT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(start_path.to_string(), result.clone());
    }

    result
}

fn find_git_root_impl(start_path: &str) -> Option<String> {
    let mut current = PathBuf::from(start_path);
    if !current.is_absolute() {
        current = std::env::current_dir().unwrap_or_default().join(&current);
    }
    current = current.canonicalize().unwrap_or(current);

    loop {
        let git_path = current.join(".git");
        if let Ok(meta) = std::fs::metadata(&git_path) {
            if meta.is_dir() || meta.is_file() {
                let result = current.to_string_lossy().to_string();
                // NFC normalize on macOS
                return Some(result);
            }
        }
        if !current.pop() {
            break;
        }
    }

    None
}

/// Resolve a git root to the canonical main repository root.
/// For a regular repo this is a no-op. For a worktree, follows the
/// `.git` file → `gitdir:` → `commondir` chain to find the main repo's
/// working directory.
pub fn find_canonical_git_root(start_path: &str) -> Option<String> {
    let root = find_git_root(start_path)?;

    {
        let cache = CANONICAL_ROOT_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&root) {
            return Some(cached.clone());
        }
    }

    let resolved = resolve_canonical_root(&root);

    {
        let mut cache = CANONICAL_ROOT_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.insert(root.clone(), resolved.clone());
    }

    Some(resolved)
}

fn resolve_canonical_root(git_root: &str) -> String {
    let git_file_path = Path::new(git_root).join(".git");

    // Try to read .git as a file (worktree/submodule)
    let git_content = match std::fs::read_to_string(&git_file_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => return git_root.to_string(), // Regular repo (.git is a directory)
    };

    if !git_content.starts_with("gitdir:") {
        return git_root.to_string();
    }

    let gitdir_path_str = git_content["gitdir:".len()..].trim();
    let worktree_git_dir = if Path::new(gitdir_path_str).is_absolute() {
        PathBuf::from(gitdir_path_str)
    } else {
        Path::new(git_root).join(gitdir_path_str)
    };

    // Read commondir
    let commondir_path = worktree_git_dir.join("commondir");
    let commondir_content = match std::fs::read_to_string(&commondir_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => return git_root.to_string(), // Submodule (no commondir)
    };

    let common_dir = if Path::new(&commondir_content).is_absolute() {
        PathBuf::from(&commondir_content)
    } else {
        worktree_git_dir.join(&commondir_content)
    };

    let common_dir = match common_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return git_root.to_string(),
    };

    // Security validation: worktreeGitDir must be a child of commonDir/worktrees/
    let worktrees_dir = common_dir.join("worktrees");
    let worktree_git_dir_canonical = worktree_git_dir
        .canonicalize()
        .unwrap_or(worktree_git_dir.clone());

    if worktree_git_dir_canonical.parent() != Some(&worktrees_dir) {
        return git_root.to_string();
    }

    // Security validation: gitdir file must point back to git_root/.git
    let gitdir_file = worktree_git_dir.join("gitdir");
    let backlink = match std::fs::read_to_string(&gitdir_file) {
        Ok(content) => content.trim().to_string(),
        Err(_) => return git_root.to_string(),
    };

    let backlink_resolved = match PathBuf::from(&backlink).canonicalize() {
        Ok(p) => p,
        Err(_) => return git_root.to_string(),
    };

    let git_root_resolved = match PathBuf::from(git_root).canonicalize() {
        Ok(p) => p.join(".git"),
        Err(_) => return git_root.to_string(),
    };

    if backlink_resolved != git_root_resolved {
        return git_root.to_string();
    }

    // Bare-repo worktrees: the common dir isn't inside a working directory
    if common_dir.file_name().map(|n| n.to_str()) != Some(Some(".git")) {
        return common_dir.to_string_lossy().to_string();
    }

    common_dir
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| git_root.to_string())
}

/// Find the git executable path.
pub fn git_exe() -> String {
    static GIT_EXE: Lazy<String> = Lazy::new(|| {
        which::which("git")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "git".to_string())
    });
    GIT_EXE.clone()
}

/// Check if current directory is a git repo.
pub fn get_is_git(cwd: &str) -> bool {
    find_git_root(cwd).is_some()
}

/// Check if the current directory IS the git root.
pub async fn is_at_git_root(cwd: &str) -> bool {
    let git_root = match find_git_root(cwd) {
        Some(root) => root,
        None => return false,
    };
    // Resolve symlinks for accurate comparison
    let resolved_cwd = tokio::fs::canonicalize(cwd).await;
    let resolved_root = tokio::fs::canonicalize(&git_root).await;

    match (resolved_cwd, resolved_root) {
        (Ok(cwd_path), Ok(root_path)) => cwd_path == root_path,
        _ => cwd == git_root,
    }
}

/// Check if a directory is inside a git repo.
pub fn dir_is_in_git_repo(cwd: &str) -> bool {
    find_git_root(cwd).is_some()
}

/// Normalizes a git remote URL to a canonical form for hashing.
/// Converts SSH and HTTPS URLs to the same format: host/owner/repo (lowercase, no .git)
pub fn normalize_git_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Handle SSH format: git@host:owner/repo.git
    let ssh_re = regex::Regex::new(r"^git@([^:]+):(.+?)(?:\.git)?$").ok()?;
    if let Some(caps) = ssh_re.captures(trimmed) {
        let host = caps.get(1)?.as_str();
        let path = caps.get(2)?.as_str();
        return Some(format!("{}/{}", host, path).to_lowercase());
    }

    // Handle HTTPS/SSH URL format
    let url_re =
        regex::Regex::new(r"^(?:https?|ssh)://(?:[^@]+@)?([^/]+)/(.+?)(?:\.git)?$").ok()?;
    if let Some(caps) = url_re.captures(trimmed) {
        let host = caps.get(1)?.as_str();
        let path = caps.get(2)?.as_str();

        // CCR git proxy URLs
        if is_local_host(host) && path.starts_with("git/") {
            let proxy_path = &path[4..];
            let segments: Vec<&str> = proxy_path.split('/').collect();
            if segments.len() >= 3 && segments[0].contains('.') {
                return Some(proxy_path.to_lowercase());
            }
            return Some(format!("github.com/{}", proxy_path).to_lowercase());
        }

        return Some(format!("{}/{}", host, path).to_lowercase());
    }

    None
}

/// Returns a SHA256 hash (first 16 chars) of the normalized git remote URL.
pub fn get_repo_remote_hash(remote_url: &str) -> Option<String> {
    let normalized = normalize_git_remote_url(remote_url)?;
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let hash = hex::encode(hasher.finalize());
    Some(hash[..16].to_string())
}

fn is_local_host(host: &str) -> bool {
    let host_without_port = host.split(':').next().unwrap_or("");
    host_without_port == "localhost"
        || regex::Regex::new(r"^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$")
            .map(|re| re.is_match(host_without_port))
            .unwrap_or(false)
}

/// Git file status.
#[derive(Debug, Clone, Default)]
pub struct GitFileStatus {
    pub tracked: Vec<String>,
    pub untracked: Vec<String>,
}

/// Git repo state.
#[derive(Debug, Clone, Serialize)]
pub struct GitRepoState {
    pub commit_hash: String,
    pub branch_name: String,
    pub remote_url: Option<String>,
    pub is_head_on_remote: bool,
    pub is_clean: bool,
    pub worktree_count: u32,
}

/// Execute a git command with no-throw semantics.
pub async fn exec_git_no_throw(args: &[&str]) -> (String, i32) {
    let output = tokio::process::Command::new(git_exe())
        .args(args)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let code = out.status.code().unwrap_or(-1);
            (stdout, code)
        }
        Err(_) => (String::new(), -1),
    }
}

/// Execute a git command with a specific working directory.
pub async fn exec_git_no_throw_cwd(args: &[&str], cwd: &str) -> (String, i32) {
    let output = tokio::process::Command::new(git_exe())
        .args(args)
        .current_dir(cwd)
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let code = out.status.code().unwrap_or(-1);
            (stdout, code)
        }
        Err(_) => (String::new(), -1),
    }
}

/// Check if HEAD is on remote.
pub async fn get_is_head_on_remote() -> bool {
    let (_, code) = exec_git_no_throw(&["rev-parse", "@{u}"]).await;
    code == 0
}

/// Check if there are unpushed commits.
pub async fn has_unpushed_commits() -> bool {
    let (stdout, code) = exec_git_no_throw(&["rev-list", "--count", "@{u}..HEAD"]).await;
    code == 0 && stdout.trim().parse::<u32>().unwrap_or(0) > 0
}

/// Check if the working tree is clean.
pub async fn get_is_clean(ignore_untracked: bool) -> bool {
    let mut args = vec!["--no-optional-locks", "status", "--porcelain"];
    if ignore_untracked {
        args.push("-uno");
    }
    let (stdout, _) = exec_git_no_throw(&args).await;
    stdout.trim().is_empty()
}

/// Get changed files from git status.
pub async fn get_changed_files() -> Vec<String> {
    let (stdout, _) = exec_git_no_throw(&["--no-optional-locks", "status", "--porcelain"]).await;
    stdout
        .trim()
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // Remove status prefix (first 3 chars: "M  ", "A  ", "?? ")
            if trimmed.len() > 3 {
                Some(trimmed[3..].trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Get file status (tracked vs untracked).
pub async fn get_file_status() -> GitFileStatus {
    let (stdout, _) = exec_git_no_throw(&["--no-optional-locks", "status", "--porcelain"]).await;

    let mut result = GitFileStatus::default();

    for line in stdout.trim().lines() {
        if line.is_empty() {
            continue;
        }
        let status = &line[..2.min(line.len())];
        let filename = line[2..].trim().to_string();

        if status == "??" {
            result.untracked.push(filename);
        } else if !filename.is_empty() {
            result.tracked.push(filename);
        }
    }

    result
}

/// Stashes all changes to return git to a clean state.
pub async fn stash_to_clean_state(message: Option<&str>) -> bool {
    let stash_message = message
        .map(|m| m.to_string())
        .unwrap_or_else(|| format!("Mossen auto-stash - {}", chrono::Utc::now().to_rfc3339()));

    // First, check for untracked files
    let status = get_file_status().await;

    if !status.untracked.is_empty() {
        let mut args: Vec<&str> = vec!["add"];
        let untracked_refs: Vec<&str> = status.untracked.iter().map(|s| s.as_str()).collect();
        args.extend(&untracked_refs);
        let (_, code) = exec_git_no_throw(&args).await;
        if code != 0 {
            return false;
        }
    }

    let (_, code) = exec_git_no_throw(&["stash", "push", "--message", &stash_message]).await;
    code == 0
}

/// Get the full git state.
pub async fn get_git_state() -> Option<GitRepoState> {
    let head_fut = exec_git_no_throw(&["rev-parse", "HEAD"]);
    let branch_fut = exec_git_no_throw(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let remote_fut = exec_git_no_throw(&["remote", "get-url", "origin"]);
    let is_head_on_remote_fut = get_is_head_on_remote();
    let is_clean_fut = get_is_clean(false);

    let (head_result, branch_result, remote_result, is_head_on_remote, is_clean) = tokio::join!(
        head_fut,
        branch_fut,
        remote_fut,
        is_head_on_remote_fut,
        is_clean_fut
    );

    let commit_hash = head_result.0.trim().to_string();
    let branch_name = branch_result.0.trim().to_string();
    let remote_url = if remote_result.1 == 0 {
        Some(remote_result.0.trim().to_string())
    } else {
        None
    };

    Some(GitRepoState {
        commit_hash,
        branch_name,
        remote_url,
        is_head_on_remote,
        is_clean,
        worktree_count: 1, // Default; actual count requires filesystem inspection
    })
}

/// Find the best remote branch to use as a base.
pub async fn find_remote_base() -> Option<String> {
    // First try: tracking branch
    let (stdout, code) =
        exec_git_no_throw(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).await;
    if code == 0 && !stdout.trim().is_empty() {
        return Some(stdout.trim().to_string());
    }

    // Second try: remote show origin
    let (stdout, code) = exec_git_no_throw(&["remote", "show", "origin", "--", "HEAD"]).await;
    if code == 0 {
        let re = regex::Regex::new(r"HEAD branch: (\S+)").ok()?;
        if let Some(caps) = re.captures(&stdout) {
            if let Some(branch) = caps.get(1) {
                return Some(format!("origin/{}", branch.as_str()));
            }
        }
    }

    // Third try: common branch names
    let candidates = ["origin/main", "origin/staging", "origin/master"];
    for candidate in &candidates {
        let (_, code) = exec_git_no_throw(&["rev-parse", "--verify", candidate]).await;
        if code == 0 {
            return Some(candidate.to_string());
        }
    }

    None
}

/// Preserved git state for issue submission.
#[derive(Debug, Clone, Serialize)]
pub struct PreservedGitState {
    pub remote_base_sha: Option<String>,
    pub remote_base: Option<String>,
    pub patch: String,
    pub untracked_files: Vec<UntrackedFile>,
    pub format_patch: Option<String>,
    pub head_sha: Option<String>,
    pub branch_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UntrackedFile {
    pub path: String,
    pub content: String,
}

/// Size limits for untracked file capture.
const MAX_FILE_SIZE_BYTES: u64 = 500 * 1024 * 1024;
const MAX_TOTAL_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const MAX_FILE_COUNT: usize = 20000;

/// Capture untracked files (git diff doesn't include them).
pub async fn capture_untracked_files() -> Vec<UntrackedFile> {
    let (stdout, code) = exec_git_no_throw(&["ls-files", "--others", "--exclude-standard"]).await;
    if code != 0 || stdout.trim().is_empty() {
        return Vec::new();
    }

    let files: Vec<&str> = stdout.trim().lines().filter(|l| !l.is_empty()).collect();
    let mut result = Vec::new();
    let mut total_size: u64 = 0;

    for file_path in files {
        if result.len() >= MAX_FILE_COUNT {
            break;
        }

        // Skip known binary extensions
        if has_binary_extension(file_path) {
            continue;
        }

        let meta = match tokio::fs::metadata(file_path).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_size = meta.len();
        if file_size > MAX_FILE_SIZE_BYTES {
            continue;
        }
        if total_size + file_size > MAX_TOTAL_SIZE_BYTES {
            break;
        }

        if file_size == 0 {
            result.push(UntrackedFile {
                path: file_path.to_string(),
                content: String::new(),
            });
            continue;
        }

        // Read and check for binary content
        match tokio::fs::read(file_path).await {
            Ok(bytes) => {
                if is_binary_content(&bytes[..bytes.len().min(8192)]) {
                    continue;
                }
                let content = String::from_utf8_lossy(&bytes).to_string();
                total_size += file_size;
                result.push(UntrackedFile {
                    path: file_path.to_string(),
                    content,
                });
            }
            Err(_) => continue,
        }
    }

    result
}

/// Check if a file has a known binary extension.
fn has_binary_extension(path: &str) -> bool {
    const BINARY_EXTS: &[&str] = &[
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp", ".svg", ".mp3", ".mp4", ".wav",
        ".avi", ".mov", ".mkv", ".flac", ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar",
        ".exe", ".dll", ".so", ".dylib", ".bin", ".obj", ".o", ".a", ".woff", ".woff2", ".ttf",
        ".otf", ".eot", ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".pyc", ".pyo",
        ".class", ".jar",
    ];

    let lower = path.to_lowercase();
    BINARY_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// Simple heuristic to detect binary content.
fn is_binary_content(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    let null_count = data[..check_len].iter().filter(|&&b| b == 0).count();
    null_count > 0
}

/// Checks if the current working directory appears to be a bare git repository.
pub fn is_current_directory_bare_git_repo(cwd: &str) -> bool {
    let cwd_path = Path::new(cwd);
    let git_path = cwd_path.join(".git");

    // Check if .git exists and is valid
    match std::fs::metadata(&git_path) {
        Ok(meta) => {
            if meta.is_file() {
                // worktree/submodule
                return false;
            }
            if meta.is_dir() {
                let git_head = git_path.join("HEAD");
                match std::fs::metadata(&git_head) {
                    Ok(head_meta) if head_meta.is_file() => return false,
                    _ => {} // fall through
                }
            }
        }
        Err(_) => {} // no .git
    }

    // Check for bare git repo indicators
    if std::fs::metadata(cwd_path.join("HEAD"))
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        return true;
    }
    if std::fs::metadata(cwd_path.join("objects"))
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        return true;
    }
    if std::fs::metadata(cwd_path.join("refs"))
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        return true;
    }

    false
}

/// Preserve git state for issue submission.
pub async fn preserve_git_state_for_issue(cwd: &str) -> Option<PreservedGitState> {
    if !get_is_git(cwd) {
        return None;
    }

    let remote_base = find_remote_base().await;

    if remote_base.is_none() {
        let (patch, _) = exec_git_no_throw(&["diff", "HEAD"]).await;
        let untracked_files = capture_untracked_files().await;
        return Some(PreservedGitState {
            remote_base_sha: None,
            remote_base: None,
            patch,
            untracked_files,
            format_patch: None,
            head_sha: None,
            branch_name: None,
        });
    }

    let remote_base_ref = remote_base.as_ref().unwrap();

    // Get merge-base
    let (merge_base_stdout, merge_base_code) =
        exec_git_no_throw(&["merge-base", "HEAD", remote_base_ref]).await;

    if merge_base_code != 0 || merge_base_stdout.trim().is_empty() {
        let (patch, _) = exec_git_no_throw(&["diff", "HEAD"]).await;
        let untracked_files = capture_untracked_files().await;
        return Some(PreservedGitState {
            remote_base_sha: None,
            remote_base: None,
            patch,
            untracked_files,
            format_patch: None,
            head_sha: None,
            branch_name: None,
        });
    }

    let remote_base_sha = merge_base_stdout.trim().to_string();

    // Run commands in parallel
    let diff_args = vec!["diff", remote_base_sha.as_str()];
    let patch_fut = exec_git_no_throw(&diff_args);
    let untracked_fut = capture_untracked_files();
    let range_str = format!("{}..HEAD", remote_base_sha);
    let format_patch_args = vec!["format-patch", range_str.as_str(), "--stdout"];
    let format_patch_fut = exec_git_no_throw(&format_patch_args);
    let head_sha_fut = exec_git_no_throw(&["rev-parse", "HEAD"]);
    let branch_name_fut = exec_git_no_throw(&["rev-parse", "--abbrev-ref", "HEAD"]);

    let (patch_result, untracked_files, format_patch_result, head_sha_result, branch_result) = tokio::join!(
        patch_fut,
        untracked_fut,
        format_patch_fut,
        head_sha_fut,
        branch_name_fut
    );

    let format_patch = if format_patch_result.1 == 0 && !format_patch_result.0.trim().is_empty() {
        Some(format_patch_result.0)
    } else {
        None
    };

    let branch_name = {
        let name = branch_result.0.trim();
        if !name.is_empty() && name != "HEAD" {
            Some(name.to_string())
        } else {
            None
        }
    };

    Some(PreservedGitState {
        remote_base_sha: Some(remote_base_sha),
        remote_base,
        patch: patch_result.0,
        untracked_files,
        format_patch,
        head_sha: Some(head_sha_result.0.trim().to_string()),
        branch_name,
    })
}

// =============================================================================
// 与 TS `git.ts` 对齐的便捷异步入口（const-arrow 等价物）。
// =============================================================================

/// 获取当前仓库的 `.git` 目录（对应 TS `getGitDir`）。
pub async fn get_git_dir(cwd: &str) -> Option<String> {
    let (out, code) = exec_git_no_throw_cwd(&["rev-parse", "--git-dir"], cwd).await;
    if code == 0 {
        let path = out.trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    } else {
        None
    }
}

/// 获取 GitHub 仓库 `owner/repo` 标识（对应 TS `getGithubRepo`）。
pub async fn get_github_repo() -> Option<String> {
    let (out, code) = exec_git_no_throw(&["config", "--get", "remote.origin.url"]).await;
    if code != 0 {
        return None;
    }
    let normalized = normalize_git_remote_url(out.trim())?;
    let trimmed = normalized.trim_end_matches(".git");
    let parts: Vec<&str> = trimmed.rsplit('/').take(2).collect();
    if parts.len() == 2 {
        Some(format!("{}/{}", parts[1], parts[0]))
    } else {
        None
    }
}

/// 获取当前 HEAD 的 SHA（对应 TS `getHead`）。
pub async fn get_head() -> String {
    exec_git_no_throw(&["rev-parse", "HEAD"])
        .await
        .0
        .trim()
        .to_string()
}

/// 获取当前分支名（对应 TS `getBranch`）。
pub async fn get_branch() -> String {
    exec_git_no_throw(&["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .0
        .trim()
        .to_string()
}

/// 获取 worktree 数量（对应 TS `getWorktreeCount`）。
pub async fn get_worktree_count() -> usize {
    let (out, code) = exec_git_no_throw(&["worktree", "list"]).await;
    if code != 0 {
        return 0;
    }
    out.lines().filter(|l| !l.trim().is_empty()).count()
}
