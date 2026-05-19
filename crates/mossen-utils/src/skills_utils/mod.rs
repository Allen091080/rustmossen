// Translated from utils/skills/githubSkillInstall.ts and utils/skills/skillChangeDetector.ts

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use anyhow::{anyhow, bail, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ============================================================================
// githubSkillInstall.ts
// ============================================================================

pub const GITHUB_SKILL_INSTALL_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;
const MAX_FILES: usize = 100;
const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSkillInstallTarget {
    pub owner: String,
    pub repo: String,
    pub ref_name: Option<String>,
    pub path: String,
    pub original: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSkillInstallFile {
    pub path: String,
    pub size_bytes: usize,
    pub download_url: String,
    #[serde(skip)]
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSkillInstallPlan {
    pub token: String,
    pub expires_at: u64,
    pub target: GitHubSkillInstallTarget,
    pub skill_name: String,
    pub description: String,
    pub install_dir: String,
    pub files: Vec<GitHubSkillInstallFile>,
    pub total_bytes: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum GitHubSkillInstallResult {
    #[serde(rename = "installed")]
    Installed {
        skill_name: String,
        install_dir: String,
        files_written: usize,
        total_bytes: usize,
        warnings: Vec<String>,
    },
    #[serde(rename = "unknown_token")]
    UnknownToken,
    #[serde(rename = "expired_token")]
    ExpiredToken,
    #[serde(rename = "already_exists")]
    AlreadyExists { install_dir: String },
    #[serde(rename = "invalid_target")]
    InvalidTarget { reason: String },
}

#[derive(Debug, Clone)]
struct StoredPlan {
    target: GitHubSkillInstallTarget,
    include_files_hash: String,
    expires_at: u64,
}

static INSTALL_PLANS: Lazy<Mutex<HashMap<String, StoredPlan>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn reset_github_skill_install_plan_store_for_testing() {
    INSTALL_PLANS.lock().unwrap().clear();
}

/// Parse a GitHub skill target from user input.
pub fn parse_github_skill_target(input: &str) -> Option<GitHubSkillInstallTarget> {
    let trimmed = input.trim();

    // Check shorthand format: owner/repo
    let shorthand_re = regex::Regex::new(r"^([A-Za-z0-9_.\-]+)/([A-Za-z0-9_.\-]+)$").ok()?;
    if let Some(caps) = shorthand_re.captures(trimmed) {
        return Some(GitHubSkillInstallTarget {
            owner: caps[1].to_string(),
            repo: strip_git_suffix(&caps[2]),
            ref_name: None,
            path: String::new(),
            original: trimmed.to_string(),
        });
    }

    // Parse as URL
    let url = url::Url::parse(trimmed).ok()?;
    let host = url.host_str()?;
    if host != "github.com" && host != "www.github.com" {
        return None;
    }

    let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let owner = parts.first()?.to_string();
    let repo = strip_git_suffix(parts.get(1)?);

    if parts.len() <= 2 {
        return Some(GitHubSkillInstallTarget {
            owner,
            repo,
            ref_name: None,
            path: String::new(),
            original: trimmed.to_string(),
        });
    }

    let tree_or_blob = parts.get(2)?;
    if *tree_or_blob != "tree" && *tree_or_blob != "blob" {
        return None;
    }

    let ref_name = parts.get(3)?.to_string();
    let path = parts[4..].join("/");

    Some(GitHubSkillInstallTarget {
        owner,
        repo,
        ref_name: Some(ref_name),
        path,
        original: trimmed.to_string(),
    })
}

fn strip_git_suffix(value: &str) -> String {
    if value.ends_with(".git") {
        value[..value.len() - 4].to_string()
    } else {
        value.to_string()
    }
}

/// Convert a string to a URL-safe skill slug.
pub fn to_skill_slug(value: &str) -> String {
    let slug: String = value
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let slug: String = slug.chars().take(64).collect();
    slug.trim_end_matches('-').to_string()
}

fn basename_no_ext(value: &str) -> String {
    let normalized = normalize_github_path(value);
    let last = normalized.split('/').filter(|s| !s.is_empty()).last().unwrap_or("skill");
    if last.to_lowercase().ends_with(".md") {
        last[..last.len() - 3].to_string()
    } else {
        last.to_string()
    }
}

fn normalize_github_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed.replace('\\', "/");
    let normalized = normalized.trim_start_matches("./").to_string();
    if normalized == "." {
        String::new()
    } else {
        normalized
    }
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains("..")
}

fn safe_join(root: &str, child: &str) -> Result<PathBuf> {
    if !is_safe_relative_path(child) {
        bail!("Unsafe skill file path: {}", child);
    }
    Ok(Path::new(root).join(child))
}

fn files_hash(files: &[GitHubSkillInstallFile]) -> String {
    files
        .iter()
        .map(|f| format!("{}:{}", f.path, f.size_bytes))
        .collect::<Vec<_>>()
        .join("|")
}

/// 对应 TS `getGitHubSkillInstallPlan`：根据 GitHub URL/shorthand 解析并构造安装计划。
///
/// `fetch_tree` 由调用方提供（注入 GitHub API 调用）：返回目录树文件列表。
/// 返回 `Some(plan)` 时计划已被纳入内存 store，可后续 execute；返回 `None` 表示输入非法或目标非 skill。
pub async fn get_github_skill_install_plan<F, Fut>(
    input: &str,
    install_root: &str,
    fetch_tree: F,
) -> Result<Option<GitHubSkillInstallPlan>>
where
    F: FnOnce(GitHubSkillInstallTarget) -> Fut,
    Fut: std::future::Future<Output = Result<(Vec<GitHubSkillInstallFile>, HashMap<String, serde_json::Value>)>>,
{
    let target = match parse_github_skill_target(input) {
        Some(t) => t,
        None => return Ok(None),
    };
    let (files, frontmatter) = fetch_tree(target.clone()).await?;
    if files.is_empty() {
        return Ok(None);
    }
    if files.len() > MAX_FILES {
        bail!("Skill exceeds maximum file count ({} > {})", files.len(), MAX_FILES);
    }
    let total_bytes: usize = files.iter().map(|f| f.size_bytes).sum();
    if total_bytes > MAX_TOTAL_BYTES {
        bail!("Skill exceeds maximum total bytes ({})", MAX_TOTAL_BYTES);
    }
    for f in &files {
        if f.size_bytes > MAX_FILE_BYTES {
            bail!("Skill file {} exceeds {} bytes", f.path, MAX_FILE_BYTES);
        }
    }
    let skill_name = frontmatter
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| to_skill_slug(s))
        .unwrap_or_else(|| basename_no_ext(&target.path));
    let description = frontmatter
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let install_dir = format!("{}/{}", install_root.trim_end_matches('/'), skill_name);
    let warnings = build_warnings(&frontmatter);
    let token = format!("ghskill-{}", uuid::Uuid::new_v4());
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + GITHUB_SKILL_INSTALL_TOKEN_TTL_MS;
    let stored = StoredPlan {
        target: target.clone(),
        include_files_hash: files_hash(&files),
        expires_at,
    };
    INSTALL_PLANS.lock().unwrap().insert(token.clone(), stored);
    Ok(Some(GitHubSkillInstallPlan {
        token,
        expires_at,
        target,
        skill_name,
        description,
        install_dir,
        files,
        total_bytes,
        warnings,
    }))
}

/// 对应 TS `executeGitHubSkillInstallPlan`：按 token 执行安装计划（写盘）。
pub async fn execute_github_skill_install_plan(
    plan: &GitHubSkillInstallPlan,
) -> Result<GitHubSkillInstallResult> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let stored = {
        let mut store = INSTALL_PLANS.lock().unwrap();
        match store.remove(&plan.token) {
            Some(p) => p,
            None => return Ok(GitHubSkillInstallResult::UnknownToken),
        }
    };
    if now > stored.expires_at {
        return Ok(GitHubSkillInstallResult::ExpiredToken);
    }
    if stored.include_files_hash != files_hash(&plan.files) {
        return Ok(GitHubSkillInstallResult::InvalidTarget {
            reason: "Plan files hash mismatch".to_string(),
        });
    }
    if tokio::fs::metadata(&plan.install_dir).await.is_ok() {
        return Ok(GitHubSkillInstallResult::AlreadyExists {
            install_dir: plan.install_dir.clone(),
        });
    }
    tokio::fs::create_dir_all(&plan.install_dir).await?;
    let mut files_written: usize = 0;
    let mut bytes_total: usize = 0;
    for file in &plan.files {
        let dest = safe_join(&plan.install_dir, &file.path)?;
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = file.content.clone().unwrap_or_default();
        bytes_total += bytes.len();
        tokio::fs::write(&dest, bytes).await?;
        files_written += 1;
    }
    Ok(GitHubSkillInstallResult::Installed {
        skill_name: plan.skill_name.clone(),
        install_dir: plan.install_dir.clone(),
        files_written,
        total_bytes: bytes_total,
        warnings: plan.warnings.clone(),
    })
}

fn build_warnings(frontmatter: &HashMap<String, serde_json::Value>) -> Vec<String> {
    let mut warnings = Vec::new();

    if frontmatter.contains_key("hooks") {
        warnings.push("Skill declares hooks; review side effects before invoking it.".to_string());
    }

    let tools_text = match frontmatter.get("allowed-tools") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(","),
        Some(v) => v.to_string(),
        None => String::new(),
    };

    let broad_tools_re =
        regex::Regex::new(r"(?i)\bBash\s*\(\s*\*\s*\)|\bEdit\b|\bWrite\b").unwrap();
    if broad_tools_re.is_match(&tools_text) {
        warnings.push(
            "Skill declares broad allowed-tools; Mossen permissions still apply.".to_string(),
        );
    }

    if frontmatter.get("disable-model-invocation") == Some(&serde_json::Value::Bool(false)) {
        warnings
            .push("Skill allows model invocation; confirm the trigger is narrow.".to_string());
    }

    warnings
}

// ============================================================================
// skillChangeDetector.ts
// ============================================================================

const FILE_STABILITY_THRESHOLD_MS: u64 = 1000;
const FILE_STABILITY_POLL_INTERVAL_MS: u64 = 500;
const RELOAD_DEBOUNCE_MS: u64 = 300;
const POLLING_INTERVAL_MS: u64 = 2000;

/// Skill change detector state.
pub struct SkillChangeDetector {
    initialized: Mutex<bool>,
    disposed: Mutex<bool>,
    listeners: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}

impl SkillChangeDetector {
    pub fn new() -> Self {
        Self {
            initialized: Mutex::new(false),
            disposed: Mutex::new(false),
            listeners: Mutex::new(Vec::new()),
        }
    }

    /// Initialize file watching for skill directories.
    pub async fn initialize(&self) {
        let mut initialized = self.initialized.lock().unwrap();
        let disposed = self.disposed.lock().unwrap();
        if *initialized || *disposed {
            return;
        }
        *initialized = true;
        // In a full implementation, this would set up file watching
    }

    /// Clean up file watcher.
    pub fn dispose(&self) {
        let mut disposed = self.disposed.lock().unwrap();
        *disposed = true;
        self.listeners.lock().unwrap().clear();
    }

    /// Subscribe to skill changes.
    pub fn subscribe(&self, listener: Box<dyn Fn() + Send + Sync>) -> Box<dyn Fn() + Send + Sync> {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(listener);
        let idx = listeners.len() - 1;
        // Return unsubscribe function (simplified)
        Box::new(move || {})
    }

    /// Manually notify that a skill path changed.
    pub fn notify_change(&self, _path: &str) {
        self.emit();
    }

    fn emit(&self) {
        let listeners = self.listeners.lock().unwrap();
        for listener in listeners.iter() {
            listener();
        }
    }

    /// Reset internal state for testing.
    pub fn reset_for_testing(&self) {
        let mut initialized = self.initialized.lock().unwrap();
        let mut disposed = self.disposed.lock().unwrap();
        *initialized = false;
        *disposed = false;
        self.listeners.lock().unwrap().clear();
    }
}

pub static SKILL_CHANGE_DETECTOR: Lazy<SkillChangeDetector> =
    Lazy::new(|| SkillChangeDetector::new());
