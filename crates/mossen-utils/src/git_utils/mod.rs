//! Git utilities — translated from utils/git/
//! Covers: git config parsing, filesystem-based git state reading, gitignore operations

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use regex::Regex;
use tokio::fs;
use tracing::error;

// --- Git Config Parser (from gitConfigParser.ts) ---

/// Parse a single value from .git/config.
pub async fn parse_git_config_value(
    git_dir: &Path,
    section: &str,
    subsection: Option<&str>,
    key: &str,
) -> Option<String> {
    let config_path = git_dir.join("config");
    let config = fs::read_to_string(&config_path).await.ok()?;
    parse_config_string(&config, section, subsection, key)
}

/// Parse a config value from an in-memory config string.
pub fn parse_config_string(
    config: &str,
    section: &str,
    subsection: Option<&str>,
    key: &str,
) -> Option<String> {
    let section_lower = section.to_lowercase();
    let key_lower = key.to_lowercase();

    let mut in_section = false;
    for line in config.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') {
            in_section = matches_section_header(trimmed, &section_lower, subsection);
            continue;
        }

        if !in_section {
            continue;
        }

        // Key-value line
        if let Some(kv) = parse_key_value(trimmed) {
            if kv.0.to_lowercase() == key_lower {
                return Some(kv.1);
            }
        }
    }

    None
}

fn parse_key_value(line: &str) -> Option<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    // Read key: alphanumeric + hyphen
    while i < chars.len() && is_key_char(chars[i]) {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let key: String = chars[..i].iter().collect();

    // Skip whitespace
    while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }

    // Must have '='
    if i >= chars.len() || chars[i] != '=' {
        return None;
    }
    i += 1; // skip '='

    // Skip whitespace after '='
    while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
        i += 1;
    }

    let value = parse_value(&chars, i);
    Some((key, value))
}

fn parse_value(chars: &[char], start: usize) -> String {
    let mut result = String::new();
    let mut in_quote = false;
    let mut i = start;

    while i < chars.len() {
        let ch = chars[i];

        // Inline comments outside quotes end the value
        if !in_quote && (ch == '#' || ch == ';') {
            break;
        }

        if ch == '"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }

        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if in_quote {
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    _ => result.push(next),
                }
                i += 2;
                continue;
            }
            if next == '\\' {
                result.push('\\');
                i += 2;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

    if !in_quote {
        result = result.trim_end().to_string();
    }

    result
}

fn matches_section_header(line: &str, section_lower: &str, subsection: Option<&str>) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 1; // skip '['

    // Read section name
    while i < chars.len() && chars[i] != ']' && chars[i] != ' ' && chars[i] != '\t' && chars[i] != '"' {
        i += 1;
    }
    let found_section: String = chars[1..i].iter().collect::<String>().to_lowercase();

    if found_section != section_lower {
        return false;
    }

    match subsection {
        None => {
            // Simple section: must end with ']'
            i < chars.len() && chars[i] == ']'
        }
        Some(expected_subsection) => {
            // Skip whitespace before subsection quote
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                i += 1;
            }

            // Must have opening quote
            if i >= chars.len() || chars[i] != '"' {
                return false;
            }
            i += 1; // skip opening quote

            // Read subsection — case-sensitive
            let mut found_subsection = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    let next = chars[i + 1];
                    if next == '\\' || next == '"' {
                        found_subsection.push(next);
                        i += 2;
                        continue;
                    }
                    found_subsection.push(next);
                    i += 2;
                    continue;
                }
                found_subsection.push(chars[i]);
                i += 1;
            }

            // Must have closing quote followed by ']'
            if i >= chars.len() || chars[i] != '"' {
                return false;
            }
            i += 1;
            if i >= chars.len() || chars[i] != ']' {
                return false;
            }

            found_subsection == expected_subsection
        }
    }
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-'
}

// --- Git Filesystem (from gitFilesystem.ts) ---

static SAFE_REF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9/._+@-]+$").unwrap()
});

static SHA1_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[0-9a-f]{40}$").unwrap()
});

static SHA256_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[0-9a-f]{64}$").unwrap()
});

/// Validate that a ref/branch name is safe to use
pub fn is_safe_ref_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.starts_with('/') {
        return false;
    }
    if name.contains("..") {
        return false;
    }
    // Reject single-dot and empty path components
    if name.split('/').any(|c| c == "." || c.is_empty()) {
        return false;
    }
    SAFE_REF_RE.is_match(name)
}

/// Validate that a string is a git SHA (40 or 64 hex chars)
pub fn is_valid_git_sha(s: &str) -> bool {
    SHA1_RE.is_match(s) || SHA256_RE.is_match(s)
}

/// Result of reading git HEAD
#[derive(Debug, Clone)]
pub enum GitHeadState {
    Branch { name: String },
    Detached { sha: String },
}

/// Resolve the actual .git directory for a repo (handles worktrees/submodules)
pub async fn resolve_git_dir(start_path: &Path) -> Option<PathBuf> {
    let root = find_git_root_path(start_path)?;
    let git_path = root.join(".git");

    let metadata = fs::metadata(&git_path).await.ok()?;
    if metadata.is_file() {
        // Worktree or submodule: .git is a file with `gitdir: <path>`
        let content = fs::read_to_string(&git_path).await.ok()?;
        let content = content.trim();
        if let Some(raw_dir) = content.strip_prefix("gitdir:") {
            let raw_dir = raw_dir.trim();
            let resolved = root.join(raw_dir);
            return Some(resolved);
        }
        return None;
    }

    // Regular repo: .git is a directory
    Some(git_path)
}

fn find_git_root_path(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Parse .git/HEAD to determine current branch or detached SHA
pub async fn read_git_head(git_dir: &Path) -> Option<GitHeadState> {
    let content = fs::read_to_string(git_dir.join("HEAD")).await.ok()?;
    let content = content.trim();

    if let Some(ref_part) = content.strip_prefix("ref:") {
        let ref_path = ref_part.trim();
        if let Some(name) = ref_path.strip_prefix("refs/heads/") {
            if !is_safe_ref_name(name) {
                return None;
            }
            return Some(GitHeadState::Branch { name: name.to_string() });
        }
        // Unusual symref — resolve to SHA
        if !is_safe_ref_name(ref_path) {
            return None;
        }
        let sha = resolve_ref(git_dir, ref_path).await.unwrap_or_default();
        return Some(GitHeadState::Detached { sha });
    }

    // Raw SHA (detached HEAD)
    if !is_valid_git_sha(content) {
        return None;
    }
    Some(GitHeadState::Detached { sha: content.to_string() })
}

/// Resolve a git ref to a commit SHA
pub async fn resolve_ref(git_dir: &Path, ref_name: &str) -> Option<String> {
    if let Some(result) = resolve_ref_in_dir(git_dir, ref_name).await {
        return Some(result);
    }
    // For worktrees: try the common gitdir
    if let Some(common_dir) = get_common_dir(git_dir).await {
        if common_dir != git_dir {
            return resolve_ref_in_dir(&common_dir, ref_name).await;
        }
    }
    None
}

async fn resolve_ref_in_dir(dir: &Path, ref_name: &str) -> Option<String> {
    // Try loose ref file
    if let Ok(content) = fs::read_to_string(dir.join(ref_name)).await {
        let content = content.trim();
        if let Some(target) = content.strip_prefix("ref:") {
            let target = target.trim();
            if !is_safe_ref_name(target) {
                return None;
            }
            // Recursively resolve symref (use boxed future to avoid infinite size)
            return Box::pin(resolve_ref(dir, target)).await;
        }
        if is_valid_git_sha(content) {
            return Some(content.to_string());
        }
        return None;
    }

    // Try packed-refs
    if let Ok(packed) = fs::read_to_string(dir.join("packed-refs")).await {
        for line in packed.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            if let Some(space_idx) = line.find(' ') {
                if &line[space_idx + 1..] == ref_name {
                    let sha = &line[..space_idx];
                    if is_valid_git_sha(sha) {
                        return Some(sha.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Read the `commondir` file to find the shared git directory
pub async fn get_common_dir(git_dir: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(git_dir.join("commondir")).await.ok()?;
    let content = content.trim();
    Some(git_dir.join(content))
}

/// Read a raw symref file and extract the branch name after a known prefix
pub async fn read_raw_symref(
    git_dir: &Path,
    ref_path: &str,
    branch_prefix: &str,
) -> Option<String> {
    let content = fs::read_to_string(git_dir.join(ref_path)).await.ok()?;
    let content = content.trim();
    if let Some(target) = content.strip_prefix("ref:") {
        let target = target.trim();
        if let Some(name) = target.strip_prefix(branch_prefix) {
            if is_safe_ref_name(name) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Get HEAD SHA for an arbitrary directory
pub async fn get_head_for_dir(cwd: &Path) -> Option<String> {
    let git_dir = resolve_git_dir(cwd).await?;
    let head = read_git_head(&git_dir).await?;
    match head {
        GitHeadState::Branch { name } => {
            resolve_ref(&git_dir, &format!("refs/heads/{}", name)).await
        }
        GitHeadState::Detached { sha } => Some(sha),
    }
}

/// Read the remote origin URL for an arbitrary directory via .git/config
pub async fn get_remote_url_for_dir(cwd: &Path) -> Option<String> {
    let git_dir = resolve_git_dir(cwd).await?;
    if let Some(url) = parse_git_config_value(&git_dir, "remote", Some("origin"), "url").await {
        return Some(url);
    }
    // In worktrees, config with remote URLs is in common dir
    if let Some(common_dir) = get_common_dir(&git_dir).await {
        if common_dir != git_dir {
            return parse_git_config_value(&common_dir, "remote", Some("origin"), "url").await;
        }
    }
    None
}

/// Check if we're in a shallow clone
pub async fn is_shallow_clone(cwd: &Path) -> bool {
    let git_dir = match resolve_git_dir(cwd).await {
        Some(d) => d,
        None => return false,
    };
    let common_dir = get_common_dir(&git_dir).await.unwrap_or_else(|| git_dir.clone());
    fs::metadata(common_dir.join("shallow")).await.is_ok()
}

/// Count worktrees by reading <commonDir>/worktrees/ directory
pub async fn get_worktree_count_from_fs(cwd: &Path) -> usize {
    let git_dir = match resolve_git_dir(cwd).await {
        Some(d) => d,
        None => return 0,
    };
    let common_dir = get_common_dir(&git_dir).await.unwrap_or_else(|| git_dir.clone());
    match fs::read_dir(common_dir.join("worktrees")).await {
        Ok(mut entries) => {
            let mut count = 0;
            while entries.next_entry().await.ok().flatten().is_some() {
                count += 1;
            }
            count + 1 // main worktree not listed
        }
        Err(_) => 1, // No worktrees directory
    }
}

// --- Gitignore (from gitignore.ts) ---

/// Check if a path is ignored by git (via `git check-ignore`)
pub async fn is_path_gitignored(file_path: &str, cwd: &Path) -> bool {
    let output = tokio::process::Command::new("git")
        .args(["check-ignore", file_path])
        .current_dir(cwd)
        .output()
        .await;
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Gets the path to the global gitignore file
pub fn get_global_gitignore_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".config")
        .join("git")
        .join("ignore")
}

/// Adds a file pattern to the global gitignore file if not already ignored
pub async fn add_file_glob_rule_to_gitignore(filename: &str, cwd: &Path) -> Result<()> {
    // Check if in git repo
    if resolve_git_dir(cwd).await.is_none() {
        return Ok(());
    }

    let gitignore_entry = format!("**/{}", filename);

    // Check if already ignored
    let test_path = if filename.ends_with('/') {
        format!("{}sample-file.txt", filename)
    } else {
        filename.to_string()
    };

    if is_path_gitignored(&test_path, cwd).await {
        return Ok(());
    }

    let global_gitignore_path = get_global_gitignore_path();
    let config_git_dir = global_gitignore_path.parent().unwrap();

    // Create directory if needed
    fs::create_dir_all(config_git_dir).await?;

    // Add entry
    match fs::read_to_string(&global_gitignore_path).await {
        Ok(content) => {
            if content.contains(&gitignore_entry) {
                return Ok(());
            }
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&global_gitignore_path)
                .await?;
            use tokio::io::AsyncWriteExt;
            file.write_all(format!("\n{}\n", gitignore_entry).as_bytes()).await?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::write(&global_gitignore_path, format!("{}\n", gitignore_entry)).await?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}
