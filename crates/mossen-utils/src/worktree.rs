//! Git worktree management utilities.
//!
//! Translates `utils/worktree.ts` — provides worktree creation, cleanup, tmux
//! integration, and ephemeral agent worktree lifecycle management.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;
use tokio::process::Command;
use tracing::warn;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

static VALID_WORKTREE_SLUG_SEGMENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9._-]+$").unwrap());

const MAX_WORKTREE_SLUG_LENGTH: usize = 64;

/// Env vars to prevent git/SSH from prompting for credentials.
fn git_no_prompt_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_ASKPASS", ""),
    ]
}

/// Ephemeral worktree patterns for stale cleanup.
static EPHEMERAL_WORKTREE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"^agent-a[0-9a-f]{7}$").unwrap(),
        Regex::new(r"^wf_[0-9a-f]{8}-[0-9a-f]{3}-\d+$").unwrap(),
        Regex::new(r"^wf-\d+$").unwrap(),
        Regex::new(r"^bridge-[A-Za-z0-9_]+(-[A-Za-z0-9_]+)*$").unwrap(),
        Regex::new(r"^job-[a-zA-Z0-9._-]{1,55}-[0-9a-f]{8}$").unwrap(),
    ]
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WorktreeSession {
    pub original_cwd: String,
    pub worktree_path: String,
    pub worktree_name: String,
    pub worktree_branch: Option<String>,
    pub original_branch: Option<String>,
    pub original_head_commit: Option<String>,
    pub session_id: String,
    pub tmux_session_name: Option<String>,
    pub hook_based: Option<bool>,
    pub creation_duration_ms: Option<u64>,
    pub used_sparse_paths: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct WorktreeObservabilitySnapshot {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
    pub original_cwd: String,
    pub original_branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorktreeDevTargetSnapshot {
    pub kind: WorktreeDevTargetKind,
    pub display_name: String,
    pub path: String,
    pub branch: Option<String>,
    pub original_cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorktreeDevTargetKind {
    Project,
    Worktree,
}

pub type WorktreeIdeTargetSnapshot = WorktreeDevTargetSnapshot;

#[derive(Debug, Clone)]
pub struct WorktreeCreateResult {
    pub worktree_path: String,
    pub worktree_branch: String,
    pub head_commit: String,
    pub base_branch: Option<String>,
    pub existed: bool,
}

#[derive(Debug, Clone)]
pub struct TmuxCreateResult {
    pub created: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecIntoTmuxResult {
    pub handled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorktreeCreateOptions {
    pub pr_number: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AgentWorktreeResult {
    pub worktree_path: String,
    pub worktree_branch: Option<String>,
    pub head_commit: Option<String>,
    pub git_root: Option<String>,
    pub hook_based: Option<bool>,
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static CURRENT_WORKTREE_SESSION: Lazy<Mutex<Option<WorktreeSession>>> =
    Lazy::new(|| Mutex::new(None));

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validates a worktree slug to prevent path traversal and directory escape.
///
/// Forward slashes are allowed for nesting (e.g. `asm/feature-foo`); each
/// segment is validated independently against the allowlist.
pub fn validate_worktree_slug(slug: &str) -> Result<()> {
    if slug.len() > MAX_WORKTREE_SLUG_LENGTH {
        return Err(anyhow!(
            "Invalid worktree name: must be {} characters or fewer (got {})",
            MAX_WORKTREE_SLUG_LENGTH,
            slug.len()
        ));
    }

    for segment in slug.split('/') {
        if segment == "." || segment == ".." {
            return Err(anyhow!(
                "Invalid worktree name \"{}\": must not contain \".\" or \"..\" path segments",
                slug
            ));
        }
        if !VALID_WORKTREE_SLUG_SEGMENT.is_match(segment) {
            return Err(anyhow!(
                "Invalid worktree name \"{}\": each \"/\"-separated segment must be non-empty and contain only letters, digits, dots, underscores, and dashes",
                slug
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

async fn mkdir_recursive(dir_path: &Path) -> Result<()> {
    fs::create_dir_all(dir_path).await?;
    Ok(())
}

/// Symlinks directories from the main repository to avoid duplication.
async fn symlink_directories(
    repo_root_path: &str,
    worktree_path: &str,
    dirs_to_symlink: &[String],
) {
    for dir in dirs_to_symlink {
        if contains_path_traversal(dir) {
            tracing::warn!("Skipping symlink for \"{}\": path traversal detected", dir);
            continue;
        }

        let source_path = PathBuf::from(repo_root_path).join(dir);
        let dest_path = PathBuf::from(worktree_path).join(dir);

        match tokio_symlink(&source_path, &dest_path).await {
            Ok(_) => {
                tracing::debug!(
                    "Symlinked {} from main repository to worktree to avoid disk bloat",
                    dir
                );
            }
            Err(e) => {
                let err_str = e.to_string();
                // ENOENT / EEXIST are expected; skip silently
                if !err_str.contains("No such file") && !err_str.contains("File exists") {
                    tracing::warn!(
                        "Failed to symlink {} ({}): {}",
                        dir,
                        "unknown",
                        err_str
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
async fn tokio_symlink(src: &Path, dst: &Path) -> Result<()> {
    tokio::fs::symlink(src, dst).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn tokio_symlink(src: &Path, dst: &Path) -> Result<()> {
    // Windows: try dir symlink
    #[cfg(windows)]
    {
        tokio::fs::symlink_dir(src, dst).await?;
    }
    Ok(())
}

fn contains_path_traversal(path: &str) -> bool {
    path.contains("..") || path.starts_with('/')
}

async fn canonicalize_comparable_path(path: &str) -> String {
    match fs::canonicalize(path).await {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => {
            let p = PathBuf::from(path);
            match p.canonicalize() {
                Ok(r) => r.to_string_lossy().to_string(),
                Err(_) => path.to_string(),
            }
        }
    }
}

fn worktrees_dir(repo_root: &str) -> PathBuf {
    PathBuf::from(repo_root).join(".mossen").join("worktrees")
}

/// Flatten nested slugs (`user/feature` → `user+feature`).
fn flatten_slug(slug: &str) -> String {
    slug.replace('/', "+")
}

/// Generate the worktree branch name from a slug.
pub fn worktree_branch_name(slug: &str) -> String {
    format!("worktree-{}", flatten_slug(slug))
}

fn worktree_path_for(repo_root: &str, slug: &str) -> PathBuf {
    worktrees_dir(repo_root).join(flatten_slug(slug))
}

// ---------------------------------------------------------------------------
// Session state accessors
// ---------------------------------------------------------------------------

pub fn get_current_worktree_session() -> Option<WorktreeSession> {
    CURRENT_WORKTREE_SESSION.lock().unwrap().clone()
}

pub fn get_current_worktree_observability_snapshot() -> Option<WorktreeObservabilitySnapshot> {
    let session = CURRENT_WORKTREE_SESSION.lock().unwrap();
    session.as_ref().map(|s| WorktreeObservabilitySnapshot {
        name: s.worktree_name.clone(),
        path: s.worktree_path.clone(),
        branch: s.worktree_branch.clone(),
        original_cwd: s.original_cwd.clone(),
        original_branch: s.original_branch.clone(),
    })
}

pub fn get_current_worktree_dev_target_snapshot(
    project_cwd: &str,
) -> WorktreeDevTargetSnapshot {
    let session = CURRENT_WORKTREE_SESSION.lock().unwrap();
    match session.as_ref() {
        None => WorktreeDevTargetSnapshot {
            kind: WorktreeDevTargetKind::Project,
            display_name: Path::new(project_cwd)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: project_cwd.to_string(),
            branch: None,
            original_cwd: None,
        },
        Some(s) => WorktreeDevTargetSnapshot {
            kind: WorktreeDevTargetKind::Worktree,
            display_name: s.worktree_name.clone(),
            path: s.worktree_path.clone(),
            branch: s.worktree_branch.clone(),
            original_cwd: Some(s.original_cwd.clone()),
        },
    }
}

pub fn get_current_worktree_ide_target_snapshot(
    project_cwd: &str,
) -> WorktreeIdeTargetSnapshot {
    get_current_worktree_dev_target_snapshot(project_cwd)
}

/// Restore the worktree session on --resume.
pub fn restore_worktree_session(session: Option<WorktreeSession>) {
    *CURRENT_WORKTREE_SESSION.lock().unwrap() = session;
}

pub fn generate_tmux_session_name(repo_path: &str, branch: &str) -> String {
    let repo_name = Path::new(repo_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let combined = format!("{}_{}", repo_name, branch);
    let re = Regex::new(r"[/.]").unwrap();
    re.replace_all(&combined, "_").to_string()
}

// ---------------------------------------------------------------------------
// exec_file_no_throw helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

async fn exec_file_no_throw(program: &str, args: &[&str]) -> ExecResult {
    exec_file_no_throw_with_cwd(program, args, None, None).await
}

async fn exec_file_no_throw_with_cwd(
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
    env_vars: Option<&[(&str, &str)]>,
) -> ExecResult {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    if let Some(vars) = env_vars {
        for (k, v) in vars {
            cmd.env(k, v);
        }
    }
    // Prevent stdin prompting
    cmd.stdin(std::process::Stdio::null());

    match cmd.output().await {
        Ok(output) => ExecResult {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(_) => ExecResult {
            code: -1,
            stdout: String::new(),
            stderr: "Failed to execute command".to_string(),
        },
    }
}

fn git_exe() -> &'static str {
    "git"
}

// ---------------------------------------------------------------------------
// Worktree creation
// ---------------------------------------------------------------------------

/// Creates a new git worktree or resumes it if it already exists.
async fn get_or_create_worktree(
    repo_root: &str,
    slug: &str,
    options: Option<&WorktreeCreateOptions>,
) -> Result<WorktreeCreateResult> {
    let worktree_path = worktree_path_for(repo_root, slug);
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    let worktree_branch = worktree_branch_name(slug);

    // Fast resume: check if worktree already exists by reading HEAD
    if let Ok(head) = read_worktree_head_sha(&worktree_path_str).await {
        if !head.is_empty() {
            return Ok(WorktreeCreateResult {
                worktree_path: worktree_path_str,
                worktree_branch,
                head_commit: head,
                base_branch: None,
                existed: true,
            });
        }
    }

    // New worktree: create directories
    mkdir_recursive(&worktrees_dir(repo_root)).await?;

    let no_prompt_env = git_no_prompt_env();

    let (base_branch, mut base_sha): (String, Option<String>) =
        if let Some(opts) = options.and_then(|o| o.pr_number.map(|_| o)) {
            let pr_num = opts.pr_number.unwrap();
            let fetch_ref = format!("pull/{}/head", pr_num);
            let result = exec_file_no_throw_with_cwd(
                git_exe(),
                &["fetch", "origin", &fetch_ref],
                Some(repo_root),
                Some(&no_prompt_env),
            )
            .await;
            if result.code != 0 {
                return Err(anyhow!(
                    "Failed to fetch PR #{}: {}",
                    pr_num,
                    result.stderr.trim()
                ));
            }
            ("FETCH_HEAD".to_string(), None)
        } else {
            let default_branch = get_default_branch(repo_root).await;
            let origin_ref = format!("origin/{}", default_branch);

            // Try to resolve ref locally first
            let resolve_result = exec_file_no_throw_with_cwd(
                git_exe(),
                &["rev-parse", &format!("refs/remotes/origin/{}", default_branch)],
                Some(repo_root),
                None,
            )
            .await;

            if resolve_result.code == 0 && !resolve_result.stdout.trim().is_empty() {
                (origin_ref, Some(resolve_result.stdout.trim().to_string()))
            } else {
                let fetch_result = exec_file_no_throw_with_cwd(
                    git_exe(),
                    &["fetch", "origin", &default_branch],
                    Some(repo_root),
                    Some(&no_prompt_env),
                )
                .await;
                let branch = if fetch_result.code == 0 {
                    origin_ref
                } else {
                    "HEAD".to_string()
                };
                (branch, None)
            }
        };

    // Resolve SHA if not already known
    if base_sha.is_none() {
        let result = exec_file_no_throw_with_cwd(
            git_exe(),
            &["rev-parse", &base_branch],
            Some(repo_root),
            None,
        )
        .await;
        if result.code != 0 {
            return Err(anyhow!(
                "Failed to resolve base branch \"{}\": git rev-parse failed",
                base_branch
            ));
        }
        base_sha = Some(result.stdout.trim().to_string());
    }

    // Build worktree add args
    let mut add_args: Vec<String> = vec!["worktree".to_string(), "add".to_string()];

    // Check for sparse paths (simplified - would need settings access)
    let sparse_paths: Vec<String> = Vec::new();
    if !sparse_paths.is_empty() {
        add_args.push("--no-checkout".to_string());
    }

    add_args.push("-B".to_string());
    add_args.push(worktree_branch.clone());
    add_args.push(worktree_path_str.clone());
    add_args.push(base_branch.clone());

    let add_args_ref: Vec<&str> = add_args.iter().map(|s| s.as_str()).collect();
    let result = exec_file_no_throw_with_cwd(git_exe(), &add_args_ref, Some(repo_root), None).await;
    if result.code != 0 {
        return Err(anyhow!("Failed to create worktree: {}", result.stderr));
    }

    // Handle sparse-checkout if needed
    if !sparse_paths.is_empty() {
        let mut sparse_args = vec!["sparse-checkout", "set", "--cone", "--"];
        let sparse_refs: Vec<&str> = sparse_paths.iter().map(|s| s.as_str()).collect();
        sparse_args.extend(sparse_refs);

        let sparse_result = exec_file_no_throw_with_cwd(
            git_exe(),
            &sparse_args,
            Some(&worktree_path_str),
            None,
        )
        .await;
        if sparse_result.code != 0 {
            // Tear down on failure
            exec_file_no_throw_with_cwd(
                git_exe(),
                &["worktree", "remove", "--force", &worktree_path_str],
                Some(repo_root),
                None,
            )
            .await;
            return Err(anyhow!(
                "Failed to configure sparse-checkout: {}",
                sparse_result.stderr
            ));
        }

        let co_result = exec_file_no_throw_with_cwd(
            git_exe(),
            &["checkout", "HEAD"],
            Some(&worktree_path_str),
            None,
        )
        .await;
        if co_result.code != 0 {
            exec_file_no_throw_with_cwd(
                git_exe(),
                &["worktree", "remove", "--force", &worktree_path_str],
                Some(repo_root),
                None,
            )
            .await;
            return Err(anyhow!(
                "Failed to checkout sparse worktree: {}",
                co_result.stderr
            ));
        }
    }

    Ok(WorktreeCreateResult {
        worktree_path: worktree_path_str,
        worktree_branch,
        head_commit: base_sha.unwrap_or_default(),
        base_branch: Some(base_branch),
        existed: false,
    })
}

/// Read the HEAD sha from a worktree path (no subprocess).
async fn read_worktree_head_sha(worktree_path: &str) -> Result<String> {
    let head_path = PathBuf::from(worktree_path).join(".git");
    // .git in a worktree is a file containing "gitdir: <path>"
    let content = fs::read_to_string(&head_path).await?;
    let git_dir = content
        .strip_prefix("gitdir: ")
        .unwrap_or(&content)
        .trim();
    let head_file = PathBuf::from(git_dir).join("HEAD");
    let head_content = fs::read_to_string(&head_file).await?;
    let trimmed = head_content.trim();
    if let Some(ref_path) = trimmed.strip_prefix("ref: ") {
        let ref_file = PathBuf::from(git_dir).join(ref_path);
        match fs::read_to_string(&ref_file).await {
            Ok(sha) => Ok(sha.trim().to_string()),
            Err(_) => Ok(String::new()),
        }
    } else {
        Ok(trimmed.to_string())
    }
}

async fn get_default_branch(repo_root: &str) -> String {
    let result = exec_file_no_throw_with_cwd(
        git_exe(),
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        Some(repo_root),
        None,
    )
    .await;
    if result.code == 0 {
        result
            .stdout
            .trim()
            .strip_prefix("origin/")
            .unwrap_or(result.stdout.trim())
            .to_string()
    } else {
        "main".to_string()
    }
}

// ---------------------------------------------------------------------------
// Copy .worktreeinclude files
// ---------------------------------------------------------------------------

/// Copy gitignored files specified in .worktreeinclude from base repo to worktree.
pub async fn copy_worktree_include_files(
    repo_root: &str,
    worktree_path: &str,
) -> Vec<String> {
    let include_path = PathBuf::from(repo_root).join(".worktreeinclude");
    let include_content = match fs::read_to_string(&include_path).await {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let patterns: Vec<&str> = include_content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    if patterns.is_empty() {
        return Vec::new();
    }

    // Get gitignored files
    let gitignored = exec_file_no_throw_with_cwd(
        git_exe(),
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
        Some(repo_root),
        None,
    )
    .await;

    if gitignored.code != 0 || gitignored.stdout.trim().is_empty() {
        return Vec::new();
    }

    let entries: Vec<&str> = gitignored
        .stdout
        .trim()
        .lines()
        .filter(|l| !l.is_empty())
        .collect();

    // Simple pattern matching (simplified from `ignore` library)
    let matcher = build_gitignore_matcher(&patterns);

    let files: Vec<String> = entries
        .iter()
        .filter(|e| !e.ends_with('/'))
        .filter(|e| matcher(e))
        .map(|e| e.to_string())
        .collect();

    // Expand collapsed directories that have matching patterns
    let collapsed_dirs: Vec<&str> = entries.iter().filter(|e| e.ends_with('/')).copied().collect();
    let mut all_files = files;

    let dirs_to_expand: Vec<&str> = collapsed_dirs
        .iter()
        .filter(|dir| {
            patterns.iter().any(|p| {
                let normalized = p.strip_prefix('/').unwrap_or(p);
                if normalized.starts_with(**dir) {
                    return true;
                }
                // Check anchored glob prefix
                if let Some(glob_idx) = normalized.find(&['*', '?', '['][..]) {
                    if glob_idx > 0 {
                        let literal_prefix = &normalized[..glob_idx];
                        if dir.starts_with(literal_prefix) {
                            return true;
                        }
                    }
                }
                false
            }) || matcher(&dir[..dir.len() - 1])
        })
        .copied()
        .collect();

    if !dirs_to_expand.is_empty() {
        let mut expand_args = vec![
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--",
        ];
        let dir_strs: Vec<&str> = dirs_to_expand.iter().copied().collect();
        expand_args.extend(dir_strs);
        let expanded = exec_file_no_throw_with_cwd(
            git_exe(),
            &expand_args,
            Some(repo_root),
            None,
        )
        .await;
        if expanded.code == 0 && !expanded.stdout.trim().is_empty() {
            for f in expanded.stdout.trim().lines().filter(|l| !l.is_empty()) {
                if matcher(f) {
                    all_files.push(f.to_string());
                }
            }
        }
    }

    let mut copied: Vec<String> = Vec::new();
    for relative_path in &all_files {
        let src = PathBuf::from(repo_root).join(relative_path);
        let dest = PathBuf::from(worktree_path).join(relative_path);
        if let Some(parent) = dest.parent() {
            if fs::create_dir_all(parent).await.is_err() {
                continue;
            }
        }
        match fs::copy(&src, &dest).await {
            Ok(_) => copied.push(relative_path.clone()),
            Err(e) => {
                tracing::warn!("Failed to copy {} to worktree: {}", relative_path, e);
            }
        }
    }

    if !copied.is_empty() {
        tracing::debug!(
            "Copied {} files from .worktreeinclude: {}",
            copied.len(),
            copied.join(", ")
        );
    }

    copied
}

/// Simplified gitignore-style matcher.
fn build_gitignore_matcher<'a>(patterns: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
    move |path: &str| {
        for pattern in patterns {
            if simple_glob_match(pattern, path) {
                return true;
            }
        }
        false
    }
}

fn simple_glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    if pattern == "**" {
        return true;
    }
    if pattern.contains('*') {
        // Simple wildcard matching
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return path.starts_with(prefix) && path.ends_with(suffix);
        }
        // For ** patterns
        if pattern.starts_with("**/") {
            let rest = &pattern[3..];
            return path.ends_with(rest) || path.contains(&format!("/{}", rest));
        }
        return false;
    }
    path == pattern || path.starts_with(&format!("{}/", pattern))
}

// ---------------------------------------------------------------------------
// Post-creation setup
// ---------------------------------------------------------------------------

/// Post-creation setup for a newly created worktree.
async fn perform_post_creation_setup(repo_root: &str, worktree_path: &str) {
    // Copy settings.local.json
    let local_settings_relative = ".mossen/settings.local.json";
    let source = PathBuf::from(repo_root).join(local_settings_relative);
    let dest = PathBuf::from(worktree_path).join(local_settings_relative);
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    match fs::copy(&source, &dest).await {
        Ok(_) => {
            tracing::debug!("Copied settings.local.json to worktree: {:?}", dest);
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Failed to copy settings.local.json: {}", e);
            }
        }
    }

    // Configure git hooks path
    let husky_path = PathBuf::from(repo_root).join(".husky");
    let git_hooks_path = PathBuf::from(repo_root).join(".git").join("hooks");
    let mut hooks_path: Option<PathBuf> = None;

    for candidate in &[&husky_path, &git_hooks_path] {
        if let Ok(meta) = fs::metadata(candidate).await {
            if meta.is_dir() {
                hooks_path = Some(candidate.to_path_buf());
                break;
            }
        }
    }

    if let Some(hp) = &hooks_path {
        let hp_str = hp.to_string_lossy().to_string();
        let result = exec_file_no_throw_with_cwd(
            git_exe(),
            &["config", "core.hooksPath", &hp_str],
            Some(worktree_path),
            None,
        )
        .await;
        if result.code == 0 {
            tracing::debug!(
                "Configured worktree to use hooks from main repository: {}",
                hp_str
            );
        } else {
            tracing::error!("Failed to configure hooks path: {}", result.stderr);
        }
    }

    // Symlink directories (would require settings access)
    // let dirs_to_symlink = settings.worktree.symlink_directories;
    // symlink_directories(repo_root, worktree_path, &dirs_to_symlink).await;

    // Copy .worktreeinclude files
    copy_worktree_include_files(repo_root, worktree_path).await;
}

// ---------------------------------------------------------------------------
// PR Reference parsing
// ---------------------------------------------------------------------------

/// Parse a PR reference from a string (URL or #N format).
pub fn parse_pr_reference(input: &str) -> Option<u64> {
    // GitHub-style PR URL
    let url_re = Regex::new(
        r"(?i)^https?://[^/]+/[^/]+/[^/]+/pull/(\d+)/?(?:[?#].*)?$",
    )
    .unwrap();
    if let Some(caps) = url_re.captures(input) {
        if let Some(num_str) = caps.get(1) {
            return num_str.as_str().parse().ok();
        }
    }

    // #N format
    let hash_re = Regex::new(r"^#(\d+)$").unwrap();
    if let Some(caps) = hash_re.captures(input) {
        if let Some(num_str) = caps.get(1) {
            return num_str.as_str().parse().ok();
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tmux operations
// ---------------------------------------------------------------------------

pub async fn is_tmux_available() -> bool {
    let result = exec_file_no_throw("tmux", &["-V"]).await;
    result.code == 0
}

pub fn get_tmux_install_instructions() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Install tmux with: brew install tmux"
    }
    #[cfg(target_os = "linux")]
    {
        "Install tmux with: sudo apt install tmux (Debian/Ubuntu) or sudo dnf install tmux (Fedora/RHEL)"
    }
    #[cfg(target_os = "windows")]
    {
        "tmux is not natively available on Windows. Consider using WSL or Cygwin."
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "Install tmux using your system package manager."
    }
}

pub async fn create_tmux_session_for_worktree(
    session_name: &str,
    worktree_path: &str,
) -> TmuxCreateResult {
    let result = exec_file_no_throw(
        "tmux",
        &["new-session", "-d", "-s", session_name, "-c", worktree_path],
    )
    .await;

    if result.code != 0 {
        TmuxCreateResult {
            created: false,
            error: Some(result.stderr),
        }
    } else {
        TmuxCreateResult {
            created: true,
            error: None,
        }
    }
}

pub async fn kill_tmux_session(session_name: &str) -> bool {
    let result = exec_file_no_throw("tmux", &["kill-session", "-t", session_name]).await;
    result.code == 0
}

// ---------------------------------------------------------------------------
// Session worktree lifecycle
// ---------------------------------------------------------------------------

/// Create a worktree for a session.
pub async fn create_worktree_for_session(
    session_id: &str,
    slug: &str,
    tmux_session_name: Option<&str>,
    options: Option<&WorktreeCreateOptions>,
    original_cwd: &str,
    git_root: Option<&str>,
) -> Result<WorktreeSession> {
    validate_worktree_slug(slug)?;

    let git_root = git_root.ok_or_else(|| {
        anyhow!(
            "Cannot create a worktree: not in a git repository and no WorktreeCreate hooks are configured."
        )
    })?;

    let original_branch = get_current_branch(git_root).await;

    let create_start = std::time::Instant::now();
    let result = get_or_create_worktree(git_root, slug, options).await?;

    let creation_duration_ms = if result.existed {
        tracing::debug!("Resuming existing worktree at: {}", result.worktree_path);
        None
    } else {
        tracing::debug!(
            "Created worktree at: {} on branch: {}",
            result.worktree_path,
            result.worktree_branch
        );
        perform_post_creation_setup(git_root, &result.worktree_path).await;
        Some(create_start.elapsed().as_millis() as u64)
    };

    let session = WorktreeSession {
        original_cwd: original_cwd.to_string(),
        worktree_path: result.worktree_path,
        worktree_name: slug.to_string(),
        worktree_branch: Some(result.worktree_branch),
        original_branch,
        original_head_commit: Some(result.head_commit),
        session_id: session_id.to_string(),
        tmux_session_name: tmux_session_name.map(|s| s.to_string()),
        hook_based: None,
        creation_duration_ms,
        used_sparse_paths: None,
    };

    *CURRENT_WORKTREE_SESSION.lock().unwrap() = Some(session.clone());
    Ok(session)
}

async fn get_current_branch(cwd: &str) -> Option<String> {
    let result = exec_file_no_throw_with_cwd(
        git_exe(),
        &["symbolic-ref", "--short", "HEAD"],
        Some(cwd),
        None,
    )
    .await;
    if result.code == 0 {
        Some(result.stdout.trim().to_string())
    } else {
        None
    }
}

/// Keep worktree intact but clear session.
pub async fn keep_worktree() {
    let session = {
        let guard = CURRENT_WORKTREE_SESSION.lock().unwrap();
        guard.clone()
    };

    if let Some(session) = session {
        let worktree_path = session.worktree_path.clone();
        let worktree_branch = session.worktree_branch.clone();

        // Clear session
        *CURRENT_WORKTREE_SESSION.lock().unwrap() = None;

        tracing::debug!(
            "Linked worktree preserved at: {}{}",
            worktree_path,
            worktree_branch
                .map(|b| format!(" on branch: {}", b))
                .unwrap_or_default()
        );
    }
}

/// Clean up the worktree completely.
pub async fn cleanup_worktree() {
    let session = {
        let guard = CURRENT_WORKTREE_SESSION.lock().unwrap();
        guard.clone()
    };

    let Some(session) = session else {
        return;
    };

    let worktree_path = &session.worktree_path;
    let original_cwd = &session.original_cwd;
    let hook_based = session.hook_based.unwrap_or(false);

    if !hook_based {
        let result = exec_file_no_throw_with_cwd(
            git_exe(),
            &["worktree", "remove", "--force", worktree_path],
            Some(original_cwd),
            None,
        )
        .await;

        if result.code != 0 {
            tracing::error!("Failed to remove linked worktree: {}", result.stderr);
        } else {
            tracing::debug!("Removed linked worktree at: {}", worktree_path);
        }
    }

    // Clear session
    *CURRENT_WORKTREE_SESSION.lock().unwrap() = None;

    // Delete temporary branch
    if !hook_based {
        if let Some(ref branch) = session.worktree_branch {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let result = exec_file_no_throw_with_cwd(
                git_exe(),
                &["branch", "-D", branch],
                Some(original_cwd),
                None,
            )
            .await;

            if result.code != 0 {
                tracing::error!("Could not delete worktree branch: {}", result.stderr);
            } else {
                tracing::debug!("Deleted worktree branch: {}", branch);
            }
        }
    }

    tracing::debug!("Linked worktree cleaned up completely");
}

// ---------------------------------------------------------------------------
// Agent worktree
// ---------------------------------------------------------------------------

/// Create a lightweight worktree for a subagent.
pub async fn create_agent_worktree(
    slug: &str,
    git_root: &str,
) -> Result<AgentWorktreeResult> {
    validate_worktree_slug(slug)?;

    let result = get_or_create_worktree(git_root, slug, None).await?;

    if !result.existed {
        tracing::debug!(
            "Created agent worktree at: {} on branch: {}",
            result.worktree_path,
            result.worktree_branch
        );
        perform_post_creation_setup(git_root, &result.worktree_path).await;
    } else {
        // Bump mtime
        let now = SystemTime::now();
        let _ = filetime::set_file_mtime(
            &result.worktree_path,
            filetime::FileTime::from_system_time(now),
        );
        tracing::debug!("Resuming existing agent worktree at: {}", result.worktree_path);
    }

    Ok(AgentWorktreeResult {
        worktree_path: result.worktree_path,
        worktree_branch: Some(result.worktree_branch),
        head_commit: Some(result.head_commit),
        git_root: Some(git_root.to_string()),
        hook_based: None,
    })
}

/// Remove a worktree created by create_agent_worktree.
pub async fn remove_agent_worktree(
    worktree_path: &str,
    worktree_branch: Option<&str>,
    git_root: Option<&str>,
    hook_based: bool,
) -> bool {
    if hook_based {
        tracing::warn!("No WorktreeRemove hook configured, hook-based agent worktree left at: {}", worktree_path);
        return false;
    }

    let Some(root) = git_root else {
        tracing::error!("Cannot remove agent worktree: no git root provided");
        return false;
    };

    let result = exec_file_no_throw_with_cwd(
        git_exe(),
        &["worktree", "remove", "--force", worktree_path],
        Some(root),
        None,
    )
    .await;

    if result.code != 0 {
        tracing::error!("Failed to remove agent worktree: {}", result.stderr);
        return false;
    }
    tracing::debug!("Removed agent worktree at: {}", worktree_path);

    if let Some(branch) = worktree_branch {
        let del_result = exec_file_no_throw_with_cwd(
            git_exe(),
            &["branch", "-D", branch],
            Some(root),
            None,
        )
        .await;

        if del_result.code != 0 {
            tracing::error!(
                "Could not delete agent worktree branch: {}",
                del_result.stderr
            );
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Stale worktree cleanup
// ---------------------------------------------------------------------------

/// Remove stale agent/workflow worktrees older than cutoff_date.
pub async fn cleanup_stale_agent_worktrees(
    cutoff_date: SystemTime,
    cwd: &str,
    git_root: Option<&str>,
) -> u64 {
    let Some(root) = git_root else {
        return 0;
    };

    let dir = worktrees_dir(root);
    let entries = match fs::read_dir(&dir).await {
        Ok(mut rd) => {
            let mut entries = Vec::new();
            while let Ok(Some(entry)) = rd.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    entries.push(name.to_string());
                }
            }
            entries
        }
        Err(_) => return 0,
    };

    let cutoff_ms = cutoff_date
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let current_path = get_current_worktree_session().map(|s| s.worktree_path);
    let resolved_current = if let Some(ref p) = current_path {
        canonicalize_comparable_path(p).await
    } else {
        String::new()
    };
    let resolved_cwd = canonicalize_comparable_path(cwd).await;

    let mut removed: u64 = 0;

    for slug in &entries {
        if !EPHEMERAL_WORKTREE_PATTERNS.iter().any(|p| p.is_match(slug)) {
            continue;
        }

        let wt_path = dir.join(slug);
        let wt_path_str = wt_path.to_string_lossy().to_string();
        let resolved_wt = canonicalize_comparable_path(&wt_path_str).await;

        // Skip current session or cwd
        if resolved_wt == resolved_current
            || resolved_cwd == resolved_wt
            || resolved_cwd.starts_with(&format!("{}/", resolved_wt))
        {
            continue;
        }

        // Check mtime
        let mtime_ms = match fs::metadata(&wt_path).await {
            Ok(meta) => {
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }
            Err(_) => continue,
        };
        if mtime_ms >= cutoff_ms {
            continue;
        }

        // Check for uncommitted changes
        let status = exec_file_no_throw_with_cwd(
            git_exe(),
            &["--no-optional-locks", "status", "--porcelain", "-uno"],
            Some(&wt_path_str),
            None,
        )
        .await;
        if status.code != 0 || !status.stdout.trim().is_empty() {
            continue;
        }

        // Check for unpushed commits
        let unpushed = exec_file_no_throw_with_cwd(
            git_exe(),
            &["rev-list", "--max-count=1", "HEAD", "--not", "--remotes"],
            Some(&wt_path_str),
            None,
        )
        .await;
        if unpushed.code != 0 || !unpushed.stdout.trim().is_empty() {
            continue;
        }

        let branch = worktree_branch_name(slug);
        if remove_agent_worktree(&wt_path_str, Some(&branch), Some(root), false).await {
            removed += 1;
        }
    }

    if removed > 0 {
        exec_file_no_throw_with_cwd(git_exe(), &["worktree", "prune"], Some(root), None).await;
        tracing::debug!(
            "cleanupStaleAgentWorktrees: removed {} stale worktree(s)",
            removed
        );
    }

    removed
}

// ---------------------------------------------------------------------------
// Worktree changes detection
// ---------------------------------------------------------------------------

/// Check whether a worktree has uncommitted changes or new commits.
pub async fn has_worktree_changes(worktree_path: &str, head_commit: &str) -> bool {
    let status = exec_file_no_throw_with_cwd(
        git_exe(),
        &["status", "--porcelain"],
        Some(worktree_path),
        None,
    )
    .await;
    if status.code != 0 {
        return true;
    }
    if !status.stdout.trim().is_empty() {
        return true;
    }

    let rev_list = exec_file_no_throw_with_cwd(
        git_exe(),
        &["rev-list", "--count", &format!("{}..HEAD", head_commit)],
        Some(worktree_path),
        None,
    )
    .await;
    if rev_list.code != 0 {
        return true;
    }
    if let Ok(count) = rev_list.stdout.trim().parse::<u64>() {
        if count > 0 {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Exec into tmux worktree (fast-path)
// ---------------------------------------------------------------------------

/// Fast-path handler for --worktree --tmux.
pub async fn exec_into_tmux_worktree(args: &[String]) -> ExecIntoTmuxResult {
    #[cfg(target_os = "windows")]
    {
        return ExecIntoTmuxResult {
            handled: false,
            error: Some("Error: --tmux is not supported on Windows".to_string()),
        };
    }

    // Check tmux availability
    let tmux_check = exec_file_no_throw("tmux", &["-V"]).await;
    if tmux_check.code != 0 {
        let install_hint = get_tmux_install_instructions();
        return ExecIntoTmuxResult {
            handled: false,
            error: Some(format!("Error: tmux is not installed. {}", install_hint)),
        };
    }

    // Parse worktree name and tmux mode from args
    let mut worktree_name: Option<String> = None;
    let mut _force_classic_tmux = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-w" || arg == "--worktree" {
            if let Some(next) = args.get(i + 1) {
                if !next.starts_with('-') {
                    worktree_name = Some(next.clone());
                }
            }
        } else if let Some(val) = arg.strip_prefix("--worktree=") {
            worktree_name = Some(val.to_string());
        } else if arg == "--tmux=classic" {
            _force_classic_tmux = true;
        }
        i += 1;
    }

    // Check for PR reference
    let mut pr_number: Option<u64> = None;
    if let Some(ref name) = worktree_name {
        pr_number = parse_pr_reference(name);
        if pr_number.is_some() {
            worktree_name = Some(format!("pr-{}", pr_number.unwrap()));
        }
    }

    // Generate slug if none provided
    if worktree_name.is_none() {
        let adjectives = ["swift", "bright", "calm", "keen", "bold"];
        let nouns = ["fox", "owl", "elm", "oak", "ray"];
        let adj = adjectives[rand::random::<usize>() % adjectives.len()];
        let noun = nouns[rand::random::<usize>() % nouns.len()];
        let suffix: String = (0..4)
            .map(|_| {
                let idx = rand::random::<usize>() % 36;
                if idx < 10 {
                    (b'0' + idx as u8) as char
                } else {
                    (b'a' + (idx - 10) as u8) as char
                }
            })
            .collect();
        worktree_name = Some(format!("{}-{}-{}", adj, noun, suffix));
    }

    let wt_name = worktree_name.unwrap();
    if let Err(e) = validate_worktree_slug(&wt_name) {
        return ExecIntoTmuxResult {
            handled: false,
            error: Some(format!("Error: {}", e)),
        };
    }

    // 1. 解析仓库根目录并准备 worktree 路径。
    //    Hook 分支需要 HooksContext，pre-main 阶段还没构建好；按 TS 同样
    //    顺序——先尝试 git 路径，这覆盖 90% 的真实用法。Hook 集成属于后续
    //    迭代。
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| ".".to_string());
    let repo_root = match crate::git::find_canonical_git_root(&cwd) {
        Some(r) => r,
        None => {
            return ExecIntoTmuxResult {
                handled: false,
                error: Some("Error: --worktree requires a git repository".to_string()),
            };
        }
    };
    let repo_name = std::path::Path::new(&repo_root)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo")
        .to_string();
    let worktree_path_buf = worktree_path_for(&repo_root, &wt_name);
    let worktree_dir = worktree_path_buf.to_string_lossy().to_string();

    let create_opts = pr_number.map(|n| WorktreeCreateOptions { pr_number: Some(n) });
    let create_result =
        match get_or_create_worktree(&repo_root, &wt_name, create_opts.as_ref()).await {
            Ok(r) => r,
            Err(e) => {
                return ExecIntoTmuxResult {
                    handled: false,
                    error: Some(format!("Error: {}", e)),
                };
            }
        };
    if !create_result.existed {
        eprintln!(
            "Created worktree: {} (based on {})",
            create_result.worktree_path,
            create_result.base_branch.as_deref().unwrap_or("HEAD"),
        );
        perform_post_creation_setup(&repo_root, &create_result.worktree_path).await;
    }

    // 2. tmux session 名 = `<repo>_<branch>`，把 `/`、`.` 替换成 `_` 满足
    //    tmux 命名约束。
    let raw_session_name = format!("{}_{}", repo_name, worktree_branch_name(&wt_name));
    let tmux_session_name: String = raw_session_name
        .chars()
        .map(|c| if c == '/' || c == '.' { '_' } else { c })
        .collect();

    // 3. 重新构造转发给内层 mossen 的 argv：去掉 `--tmux*` / `-w` / `--worktree`
    //    本身（以及它们的值），其它保留。
    let mut new_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--tmux" || arg == "--tmux=classic" || arg.starts_with("--tmux=") {
            i += 1;
            continue;
        }
        if arg == "-w" || arg == "--worktree" {
            // Skip flag + its value
            if args.get(i + 1).map_or(false, |n| !n.starts_with('-')) {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if arg.starts_with("--worktree=") {
            i += 1;
            continue;
        }
        new_args.push(arg.clone());
        i += 1;
    }

    // 4. 拉 tmux prefix（用于 conflict 提示）。
    let prefix_result = exec_file_no_throw("tmux", &["show-options", "-g", "prefix"]).await;
    let tmux_prefix = if prefix_result.code == 0 {
        // stdout looks like: `prefix C-b`
        prefix_result
            .stdout
            .split_whitespace()
            .nth(1)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "C-b".to_string())
    } else {
        "C-b".to_string()
    };
    let mossen_bindings = ["C-b", "C-c", "C-d", "C-t", "C-o", "C-r", "C-s", "C-g", "C-e"];
    let prefix_conflicts = mossen_bindings.contains(&tmux_prefix.as_str());

    // 5. 内层 mossen 需要的环境变量。
    let env_overrides = [
        ("MOSSEN_CODE_TMUX_SESSION", tmux_session_name.as_str()),
        ("MOSSEN_CODE_TMUX_PREFIX", tmux_prefix.as_str()),
        (
            "MOSSEN_CODE_TMUX_PREFIX_CONFLICTS",
            if prefix_conflicts { "1" } else { "" },
        ),
    ];

    // 6. 已经在 tmux session 内？已存在 session？iTerm2？
    let has_session_result =
        exec_file_no_throw("tmux", &["has-session", "-t", &tmux_session_name]).await;
    let session_exists = has_session_result.code == 0;
    let is_already_in_tmux = std::env::var("TMUX")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let use_control_mode =
        crate::swarm::is_in_iterm2() && !_force_classic_tmux && !is_already_in_tmux;
    let tmux_global_args: &[&str] = if use_control_mode { &["-CC"] } else { &[] };

    if use_control_mode && !session_exists {
        eprintln!(
            "\n╭─ iTerm2 Tip ────────────────────────────────────────────────────────╮\n\
             │ To open as a tab instead of a new window:                           │\n\
             │ iTerm2 > Settings > General > tmux > \"Tabs in attaching window\"     │\n\
             ╰─────────────────────────────────────────────────────────────────────╯\n"
        );
    }

    // 7. 当前可执行文件 path——内层 mossen 用同一个二进制。
    let mossen_exec = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return ExecIntoTmuxResult {
                handled: false,
                error: Some(format!("Error: failed to resolve current exe: {}", e)),
            };
        }
    };
    let mossen_exec_str = mossen_exec.to_string_lossy().to_string();

    // 8. 真正起 tmux。三条路径：
    //    a) 已经在 tmux 里 + session 存在  → switch-client
    //    b) 已经在 tmux 里 + session 不在  → new-session -d + switch-client
    //    c) 不在 tmux 里                   → new-session -A（attach 或创建）
    use std::process::Command as StdCommand;

    let spawn_inherit = |args: &[&str], cwd: Option<&str>, extra_env: &[(&str, &str)]| -> bool {
        let mut cmd = StdCommand::new("tmux");
        cmd.args(args);
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        match cmd.status() {
            Ok(s) => s.success(),
            Err(_) => false,
        }
    };

    if is_already_in_tmux {
        if session_exists {
            spawn_inherit(&["switch-client", "-t", &tmux_session_name], None, &[]);
        } else {
            // 1) detached new-session
            let mut new_session_args: Vec<&str> = vec![
                "new-session",
                "-d",
                "-s",
                &tmux_session_name,
                "-c",
                &worktree_dir,
                "--",
                &mossen_exec_str,
            ];
            let new_arg_refs: Vec<&str> = new_args.iter().map(String::as_str).collect();
            new_session_args.extend(new_arg_refs.iter().copied());
            spawn_inherit(&new_session_args, Some(&worktree_dir), &env_overrides);
            // 2) switch-client (sibling session)
            spawn_inherit(&["switch-client", "-t", &tmux_session_name], None, &[]);
        }
    } else {
        // Not in tmux — new-session -A (attach if exists, create if not).
        let mut full_args: Vec<&str> = Vec::new();
        full_args.extend(tmux_global_args.iter().copied());
        full_args.extend(
            [
                "new-session",
                "-A",
                "-s",
                tmux_session_name.as_str(),
                "-c",
                worktree_dir.as_str(),
                "--",
                mossen_exec_str.as_str(),
            ]
            .iter()
            .copied(),
        );
        let new_arg_refs: Vec<&str> = new_args.iter().map(String::as_str).collect();
        full_args.extend(new_arg_refs.iter().copied());
        spawn_inherit(&full_args, Some(&worktree_dir), &env_overrides);
    }

    ExecIntoTmuxResult {
        handled: true,
        error: None,
    }
}
