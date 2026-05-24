//! Filesystem permission checks.
//!
//! Translates `utils/permissions/filesystem.ts` — dangerous files/directories,
//! path safety checks, working-directory membership, rule matching (gitignore),
//! and internal-path carve-outs for plans/scratchpad/agent-memory.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use once_cell::sync::Lazy;
use regex::Regex;

use super::permission_result::{
    ExternalPermissionMode, PermissionAllowDecision, PermissionAskDecision, PermissionBehavior,
    PermissionDecision, PermissionDecisionReason, PermissionDenyDecision, PermissionMode,
    PermissionResult, PermissionRule, PermissionRuleSource, PermissionRuleValue, PermissionUpdate,
    PermissionUpdateDestination, ToolPermissionContext,
};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Dangerous files that should be protected from auto-editing.
pub const DANGEROUS_FILES: &[&str] = &[
    ".gitconfig",
    ".gitmodules",
    ".bashrc",
    ".bash_profile",
    ".zshrc",
    ".zprofile",
    ".profile",
    ".ripgreprc",
    ".mcp.json",
    ".mossen.json",
    ".npmrc",
    ".pypirc",
    ".netrc",
    ".env",
    "authorized_keys",
    "id_rsa",
    "id_ed25519",
    "credentials",
];

/// Dangerous file-name prefixes (e.g. `.env.local`, `.env.production`).
pub const DANGEROUS_FILE_PREFIXES: &[&str] = &[".env."];

/// Dangerous directories protected from auto-editing.
pub const DANGEROUS_DIRECTORIES: &[&str] = &[
    ".git", ".vscode", ".idea", ".mossen", ".ssh", ".aws", ".kube", ".docker",
];

/// File edit tool name constant.
pub const FILE_EDIT_TOOL_NAME: &str = "file_edit";

/// File read tool name constant.
pub const FILE_READ_TOOL_NAME: &str = "file_read";

/// Mossen folder permission pattern.
pub const MOSSEN_FOLDER_PERMISSION_PATTERN: &str = "/.mossen/**";

/// Global mossen folder permission pattern.
pub const GLOBAL_MOSSEN_FOLDER_PERMISSION_PATTERN: &str = "~/.mossen/**";

// Always use '/' as the path separator per gitignore spec
const DIR_SEP: char = '/';

// ─── Path Helpers ────────────────────────────────────────────────────────────

/// Normalizes a path for case-insensitive comparison.
pub fn normalize_case_for_comparison(path: &str) -> String {
    path.to_lowercase()
}

/// Cross-platform relative path calculation returning POSIX-style paths.
pub fn relative_path(from: &str, to: &str) -> String {
    let from_path = if cfg!(windows) {
        windows_path_to_posix(from)
    } else {
        from.to_string()
    };
    let to_path = if cfg!(windows) {
        windows_path_to_posix(to)
    } else {
        to.to_string()
    };
    compute_posix_relative(&from_path, &to_path)
}

/// Converts a path to POSIX format for pattern matching.
pub fn to_posix_path(path: &str) -> String {
    if cfg!(windows) {
        windows_path_to_posix(path)
    } else {
        path.to_string()
    }
}

fn windows_path_to_posix(path: &str) -> String {
    path.replace('\\', "/")
}

/// Compute POSIX-style relative path from `base` to `target`.
fn compute_posix_relative(base: &str, target: &str) -> String {
    let base_parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    let target_parts: Vec<&str> = target.split('/').filter(|s| !s.is_empty()).collect();

    let mut common = 0;
    for (a, b) in base_parts.iter().zip(target_parts.iter()) {
        if *a == *b {
            common += 1;
        } else {
            break;
        }
    }

    let ups = base_parts.len() - common;
    let mut parts: Vec<&str> = Vec::new();
    for _ in 0..ups {
        parts.push("..");
    }
    for part in &target_parts[common..] {
        parts.push(part);
    }
    parts.join("/")
}

// ─── Settings / CWD Helpers ──────────────────────────────────────────────────

/// Context passed to filesystem permission functions (replaces global state).
pub struct FsPermissionContext {
    pub original_cwd: String,
    pub session_id: String,
    pub home_dir: String,
    pub platform: Platform,
    pub mossen_config_home_dir: String,
    pub plans_directory: String,
    pub plan_slug: String,
    pub project_dir: String,
    pub tool_results_dir: String,
    pub scratchpad_enabled: bool,
    pub templates_enabled: bool,
    pub mossen_job_dir: Option<String>,
    pub mossen_temp_dir: String,
    pub bundled_skills_root: String,
    pub settings_paths: Vec<String>,
    pub settings_root_path_for_source: Box<dyn Fn(PermissionRuleSource) -> String + Send + Sync>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Unix,
    Windows,
    Wsl,
}

// ─── Mossen Skill Scope ─────────────────────────────────────────────────────

pub struct MossenSkillScope {
    pub skill_name: String,
    pub pattern: String,
}

/// If filePath is inside a .mossen/skills/{name}/ directory, returns the skill
/// name and a session-allow pattern scoped to just that skill.
pub fn get_mossen_skill_scope(
    file_path: &str,
    ctx: &FsPermissionContext,
) -> Option<MossenSkillScope> {
    let absolute_path = expand_path_with_ctx(file_path, ctx);
    let absolute_path_lower = normalize_case_for_comparison(&absolute_path);

    let bases = [
        (
            format!("{}/{}/{}", ctx.original_cwd, ".mossen", "skills"),
            "/.mossen/skills/",
        ),
        (
            format!("{}/{}/{}", ctx.home_dir, ".mossen", "skills"),
            "~/.mossen/skills/",
        ),
    ];

    for (dir, prefix) in &bases {
        let dir_expanded = expand_path_with_ctx(dir, ctx);
        let dir_lower = normalize_case_for_comparison(&dir_expanded);

        for sep_char in &[MAIN_SEPARATOR, '/'] {
            let sep_str = sep_char.to_string();
            let dir_with_sep = format!("{}{}", dir_lower, sep_str.to_lowercase());
            if absolute_path_lower.starts_with(&dir_with_sep) {
                let rest = &absolute_path[dir_expanded.len() + sep_str.len()..];
                let slash = rest.find('/');
                let bslash = if MAIN_SEPARATOR == '\\' {
                    rest.find('\\')
                } else {
                    None
                };
                let cut = match (slash, bslash) {
                    (None, None) => return None,
                    (Some(s), None) => s,
                    (None, Some(b)) => b,
                    (Some(s), Some(b)) => s.min(b),
                };
                if cut == 0 {
                    return None;
                }
                let skill_name = &rest[..cut];
                if skill_name.is_empty() || skill_name == "." || skill_name.contains("..") {
                    return None;
                }
                // Reject glob metacharacters
                static GLOB_META: Lazy<Regex> = Lazy::new(|| Regex::new(r"[*?\[\]]").unwrap());
                if GLOB_META.is_match(skill_name) {
                    return None;
                }
                return Some(MossenSkillScope {
                    skill_name: skill_name.to_string(),
                    pattern: format!("{}{}/**", prefix, skill_name),
                });
            }
        }
    }
    None
}

// ─── Settings Path Checks ────────────────────────────────────────────────────

pub fn is_mossen_settings_path(file_path: &str, ctx: &FsPermissionContext) -> bool {
    let expanded_path = expand_path_with_ctx(file_path, ctx);
    let normalized_path = normalize_case_for_comparison(&expanded_path);
    let sep = std::path::MAIN_SEPARATOR;

    let suffix1 = format!("{}.mossen{}settings.json", sep, sep).to_lowercase();
    let suffix2 = format!("{}.mossen{}settings.local.json", sep, sep).to_lowercase();

    if normalized_path.ends_with(&suffix1) || normalized_path.ends_with(&suffix2) {
        return true;
    }
    ctx.settings_paths
        .iter()
        .any(|sp| normalize_case_for_comparison(sp) == normalized_path)
}

fn is_mossen_config_file_path(file_path: &str, ctx: &FsPermissionContext) -> bool {
    if is_mossen_settings_path(file_path, ctx) {
        return true;
    }
    let commands_dir = format!("{}/.mossen/commands", ctx.original_cwd);
    let agents_dir = format!("{}/.mossen/agents", ctx.original_cwd);
    let skills_dir = format!("{}/.mossen/skills", ctx.original_cwd);

    path_in_working_path(file_path, &commands_dir, ctx)
        || path_in_working_path(file_path, &agents_dir, ctx)
        || path_in_working_path(file_path, &skills_dir, ctx)
}

fn is_session_plan_file(absolute_path: &str, ctx: &FsPermissionContext) -> bool {
    let expected_prefix = format!("{}/{}", ctx.plans_directory, ctx.plan_slug);
    let normalized = normalize_path(absolute_path);
    normalized.starts_with(&expected_prefix) && normalized.ends_with(".md")
}

/// Returns the session memory directory path for the current session.
pub fn get_session_memory_dir(ctx: &FsPermissionContext) -> String {
    format!("{}/{}/session-memory/", ctx.project_dir, ctx.session_id)
}

/// Returns the session memory file path.
pub fn get_session_memory_path(ctx: &FsPermissionContext) -> String {
    format!("{}summary.md", get_session_memory_dir(ctx))
}

fn is_session_memory_path(absolute_path: &str, ctx: &FsPermissionContext) -> bool {
    let normalized = normalize_path(absolute_path);
    normalized.starts_with(&get_session_memory_dir(ctx))
}

fn is_project_dir_path(absolute_path: &str, ctx: &FsPermissionContext) -> bool {
    let project_dir = &ctx.project_dir;
    let normalized = normalize_path(absolute_path);
    let sep = std::path::MAIN_SEPARATOR;
    normalized == *project_dir || normalized.starts_with(&format!("{}{}", project_dir, sep))
}

/// Returns the Mossen temp directory name.
pub fn get_mossen_temp_dir_name(ctx: &FsPermissionContext) -> String {
    if ctx.platform == Platform::Windows {
        "mossen".to_string()
    } else {
        let uid = unsafe { libc::getuid() };
        format!("mossen-{}", uid)
    }
}

/// Returns the project temp directory path.
pub fn get_project_temp_dir(ctx: &FsPermissionContext) -> String {
    let sanitized = sanitize_path(&ctx.original_cwd);
    format!("{}{}/", ctx.mossen_temp_dir, sanitized)
}

/// Returns the scratchpad directory path.
pub fn get_scratchpad_dir(ctx: &FsPermissionContext) -> String {
    format!("{}{}/scratchpad", get_project_temp_dir(ctx), ctx.session_id)
}

fn is_scratchpad_path(absolute_path: &str, ctx: &FsPermissionContext) -> bool {
    if !ctx.scratchpad_enabled {
        return false;
    }
    let scratchpad_dir = get_scratchpad_dir(ctx);
    let normalized = normalize_path(absolute_path);
    let sep = std::path::MAIN_SEPARATOR;
    normalized == scratchpad_dir || normalized.starts_with(&format!("{}{}", scratchpad_dir, sep))
}

// ─── Path Safety ─────────────────────────────────────────────────────────────

/// Result of a path safety check.
pub enum PathSafetyResult {
    Safe,
    Unsafe {
        message: String,
        classifier_approvable: bool,
    },
}

/// Check if a file path is dangerous to auto-edit.
pub fn is_dangerous_file_path_to_auto_edit(path: &str, ctx: &FsPermissionContext) -> bool {
    let absolute_path = expand_path_with_ctx(path, ctx);
    let sep = std::path::MAIN_SEPARATOR;
    let path_segments: Vec<&str> = absolute_path.split(sep).collect();
    let file_name = path_segments.last().copied();

    // Check UNC paths
    if path.starts_with("\\\\") || path.starts_with("//") {
        return true;
    }

    // Check dangerous directories (case-insensitive)
    for (i, segment) in path_segments.iter().enumerate() {
        let normalized_segment = normalize_case_for_comparison(segment);
        for dir in DANGEROUS_DIRECTORIES {
            if normalized_segment != normalize_case_for_comparison(dir) {
                continue;
            }
            // Special case: .mossen/worktrees/ is structural
            if *dir == ".mossen" {
                if let Some(next) = path_segments.get(i + 1) {
                    if normalize_case_for_comparison(next) == "worktrees" {
                        break;
                    }
                }
            }
            return true;
        }
    }

    // Check dangerous files (case-insensitive)
    if let Some(fname) = file_name {
        let normalized_fname = normalize_case_for_comparison(fname);
        if DANGEROUS_FILES
            .iter()
            .any(|df| normalize_case_for_comparison(df) == normalized_fname)
        {
            return true;
        }
        if DANGEROUS_FILE_PREFIXES
            .iter()
            .any(|pfx| normalized_fname.starts_with(&normalize_case_for_comparison(pfx)))
        {
            return true;
        }
    }

    false
}

/// Detects suspicious Windows path patterns.
pub fn has_suspicious_windows_path_pattern(path: &str, platform: Platform) -> bool {
    // NTFS Alternate Data Streams (Windows/WSL only)
    if platform == Platform::Windows || platform == Platform::Wsl {
        if path.len() > 2 {
            if let Some(_idx) = path[2..].find(':') {
                return true;
            }
        }
    }

    // 8.3 short names
    static SHORT_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"~\d").unwrap());
    if SHORT_NAME.is_match(path) {
        return true;
    }

    // Long path prefixes
    if path.starts_with("\\\\?\\")
        || path.starts_with("\\\\.\\")
        || path.starts_with("//?/")
        || path.starts_with("//./")
    {
        return true;
    }

    // Trailing dots and spaces
    static TRAILING: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.\s]+$").unwrap());
    if TRAILING.is_match(path) {
        return true;
    }

    // DOS device names
    static DOS_DEVICE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\.(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$").unwrap());
    if DOS_DEVICE.is_match(path) {
        return true;
    }

    // Three or more consecutive dots as path component
    static TRIPLE_DOTS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(^|/|\\)\.{3,}(/|\\|$)").unwrap());
    if TRIPLE_DOTS.is_match(path) {
        return true;
    }

    // UNC paths
    if contains_vulnerable_unc_path(path) {
        return true;
    }

    false
}

/// Check for vulnerable UNC path patterns.
pub fn contains_vulnerable_unc_path(path: &str) -> bool {
    path.starts_with("\\\\") || path.starts_with("//")
}

/// Checks if a path is safe for auto-editing.
pub fn check_path_safety_for_auto_edit(
    path: &str,
    precomputed_paths: Option<&[String]>,
    ctx: &FsPermissionContext,
) -> PathSafetyResult {
    let default_paths;
    let paths_to_check = match precomputed_paths {
        Some(p) => p,
        None => {
            default_paths = get_paths_for_permission_check(path);
            &default_paths
        }
    };

    for p in paths_to_check {
        if has_suspicious_windows_path_pattern(p, ctx.platform) {
            return PathSafetyResult::Unsafe {
                message: format!(
                    "Mossen requested permissions to write to {}, which contains a suspicious Windows path pattern that requires manual approval.",
                    path
                ),
                classifier_approvable: false,
            };
        }
    }

    for p in paths_to_check {
        if is_mossen_config_file_path(p, ctx) {
            return PathSafetyResult::Unsafe {
                message: format!(
                    "Mossen requested permissions to write to {}, but you haven't granted it yet.",
                    path
                ),
                classifier_approvable: true,
            };
        }
    }

    for p in paths_to_check {
        if is_dangerous_file_path_to_auto_edit(p, ctx) {
            return PathSafetyResult::Unsafe {
                message: format!(
                    "Mossen requested permissions to edit {} which is a sensitive file.",
                    path
                ),
                classifier_approvable: true,
            };
        }
    }

    PathSafetyResult::Safe
}

// ─── Working Directory Checks ────────────────────────────────────────────────

/// Returns all working directories from context.
pub fn all_working_directories(
    context: &ToolPermissionContext,
    original_cwd: &str,
) -> HashSet<String> {
    let mut dirs = HashSet::new();
    dirs.insert(original_cwd.to_string());
    for key in context.additional_working_directories.keys() {
        dirs.insert(key.clone());
    }
    dirs
}

/// Check if path is within any allowed working path.
pub fn path_in_allowed_working_path(
    path: &str,
    tool_permission_context: &ToolPermissionContext,
    precomputed_paths: Option<&[String]>,
    ctx: &FsPermissionContext,
) -> bool {
    let default_paths;
    let paths_to_check = match precomputed_paths {
        Some(p) => p,
        None => {
            default_paths = get_paths_for_permission_check(path);
            &default_paths
        }
    };

    let working_dirs = all_working_directories(tool_permission_context, &ctx.original_cwd);
    let working_paths: Vec<String> = working_dirs
        .iter()
        .flat_map(|wp| get_paths_for_permission_check(wp))
        .collect();

    paths_to_check.iter().all(|ptc| {
        working_paths
            .iter()
            .any(|wp| path_in_working_path(ptc, wp, ctx))
    })
}

/// Check if path is inside working_path directory.
pub fn path_in_working_path(path: &str, working_path: &str, ctx: &FsPermissionContext) -> bool {
    let absolute_path = expand_path_with_ctx(path, ctx);
    let absolute_working_path = expand_path_with_ctx(working_path, ctx);

    // Handle macOS symlink normalization
    let normalized_path = absolute_path
        .replace("/private/var/", "/var/")
        .replace("/private/tmp/", "/tmp/")
        .replace("/private/tmp", "/tmp");
    let normalized_working_path = absolute_working_path
        .replace("/private/var/", "/var/")
        .replace("/private/tmp/", "/tmp/")
        .replace("/private/tmp", "/tmp");

    let case_path = normalize_case_for_comparison(&normalized_path);
    let case_working = normalize_case_for_comparison(&normalized_working_path);

    let rel = relative_path(&case_working, &case_path);

    if rel.is_empty() {
        return true;
    }
    if contains_path_traversal(&rel) {
        return false;
    }
    !rel.starts_with('/')
}

fn contains_path_traversal(path: &str) -> bool {
    path == ".." || path.starts_with("../") || path.contains("/../") || path.ends_with("/..")
}

// ─── Rule Matching (gitignore-style) ─────────────────────────────────────────

fn root_path_for_source(source: PermissionRuleSource, ctx: &FsPermissionContext) -> String {
    match source {
        PermissionRuleSource::CliArg
        | PermissionRuleSource::Command
        | PermissionRuleSource::Session => expand_path_with_ctx(&ctx.original_cwd, ctx),
        _ => (ctx.settings_root_path_for_source)(source),
    }
}

fn prepend_dir_sep(path: &str) -> String {
    format!("/{}", path.trim_start_matches('/'))
}

struct PatternWithRoot {
    relative_pattern: String,
    root: Option<String>,
}

fn pattern_with_root(
    pattern: &str,
    source: PermissionRuleSource,
    ctx: &FsPermissionContext,
) -> PatternWithRoot {
    if pattern.starts_with("//") {
        let without_double = &pattern[1..];
        if ctx.platform == Platform::Windows {
            // Check POSIX-style drive path like /c/Users/...
            static DRIVE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^/[a-zA-Z]/").unwrap());
            if DRIVE_RE.is_match(without_double) {
                let drive_letter = without_double.chars().nth(1).unwrap_or('C');
                let path_after_drive = &without_double[2..];
                let drive_root = format!("{}:\\", drive_letter.to_uppercase().next().unwrap());
                let relative_from_drive = path_after_drive
                    .strip_prefix('/')
                    .unwrap_or(path_after_drive);
                return PatternWithRoot {
                    relative_pattern: relative_from_drive.to_string(),
                    root: Some(drive_root),
                };
            }
        }
        return PatternWithRoot {
            relative_pattern: without_double.to_string(),
            root: Some("/".to_string()),
        };
    } else if pattern.starts_with("~/") {
        return PatternWithRoot {
            relative_pattern: pattern[1..].to_string(),
            root: Some(ctx.home_dir.clone()),
        };
    } else if pattern.starts_with('/') {
        return PatternWithRoot {
            relative_pattern: pattern.to_string(),
            root: Some(root_path_for_source(source, ctx)),
        };
    }
    // No root specified
    let normalized = if pattern.starts_with("./") {
        pattern[2..].to_string()
    } else {
        pattern.to_string()
    };
    PatternWithRoot {
        relative_pattern: normalized,
        root: None,
    }
}

/// Get patterns grouped by root for a given tool type and behavior.
fn get_patterns_by_root(
    tool_permission_context: &ToolPermissionContext,
    tool_type: &str,
    behavior: &str,
    ctx: &FsPermissionContext,
) -> HashMap<Option<String>, HashMap<String, PermissionRule>> {
    let tool_name = match tool_type {
        "edit" => FILE_EDIT_TOOL_NAME,
        "read" => FILE_READ_TOOL_NAME,
        _ => FILE_EDIT_TOOL_NAME,
    };

    let rules = get_rule_by_contents_for_tool_name(tool_permission_context, tool_name, behavior);
    let mut patterns_by_root: HashMap<Option<String>, HashMap<String, PermissionRule>> =
        HashMap::new();

    for (pattern, rule) in rules {
        let pwr = pattern_with_root(&pattern, rule.source, ctx);
        patterns_by_root
            .entry(pwr.root)
            .or_default()
            .insert(pwr.relative_pattern, rule);
    }
    patterns_by_root
}

/// Get rule contents for a given tool name and behavior from the context.
fn get_rule_by_contents_for_tool_name(
    context: &ToolPermissionContext,
    tool_name: &str,
    behavior: &str,
) -> HashMap<String, PermissionRule> {
    let rules_by_source = match behavior {
        "allow" => &context.always_allow_rules,
        "deny" => &context.always_deny_rules,
        "ask" => &context.always_ask_rules,
        _ => return HashMap::new(),
    };

    let perm_behavior = match behavior {
        "allow" => PermissionBehavior::Allow,
        "deny" => PermissionBehavior::Deny,
        "ask" => PermissionBehavior::Ask,
        _ => PermissionBehavior::Ask,
    };

    let mut result = HashMap::new();
    for (source, rules_vec) in rules_by_source {
        for rule_str in rules_vec {
            // Rules are stored as "toolName:ruleContent" or just "toolName"
            let (name, content) = if let Some(colon_idx) = rule_str.find(':') {
                (&rule_str[..colon_idx], Some(&rule_str[colon_idx + 1..]))
            } else {
                (rule_str.as_str(), None)
            };
            if name == tool_name {
                if let Some(c) = content {
                    result.insert(
                        c.to_string(),
                        PermissionRule {
                            source: *source,
                            rule_behavior: perm_behavior,
                            rule_value: PermissionRuleValue {
                                tool_name: tool_name.to_string(),
                                rule_content: Some(c.to_string()),
                            },
                        },
                    );
                }
            }
        }
    }
    result
}

/// Match a path against permission rules using gitignore-style pattern matching.
pub fn matching_rule_for_input(
    path: &str,
    tool_permission_context: &ToolPermissionContext,
    tool_type: &str,
    behavior: &str,
    ctx: &FsPermissionContext,
) -> Option<PermissionRule> {
    let mut file_absolute_path = expand_path_with_ctx(path, ctx);
    if cfg!(windows) && file_absolute_path.contains('\\') {
        file_absolute_path = windows_path_to_posix(&file_absolute_path);
    }

    let patterns_by_root = get_patterns_by_root(tool_permission_context, tool_type, behavior, ctx);

    for (root, pattern_map) in &patterns_by_root {
        let patterns: Vec<String> = pattern_map
            .keys()
            .map(|p| {
                if p.ends_with("/**") {
                    p[..p.len() - 3].to_string()
                } else {
                    p.clone()
                }
            })
            .collect();

        let base = root.as_deref().unwrap_or(&ctx.original_cwd);
        let rel = relative_path(base, &file_absolute_path);

        if rel.starts_with("../") {
            continue;
        }
        if rel.is_empty() {
            continue;
        }

        // Use gitignore-style matching
        if let Some(matched_pattern) = match_gitignore_patterns(&rel, &patterns) {
            // Check for /** variant first
            let with_wildcard = format!("{}/**", matched_pattern);
            if let Some(rule) = pattern_map.get(&with_wildcard) {
                return Some(rule.clone());
            }
            if let Some(rule) = pattern_map.get(&matched_pattern) {
                return Some(rule.clone());
            }
        }
    }
    None
}

/// Simple gitignore-style pattern matching.
fn match_gitignore_patterns(rel_path: &str, patterns: &[String]) -> Option<String> {
    for pattern in patterns {
        if gitignore_match(rel_path, pattern) {
            return Some(pattern.clone());
        }
    }
    None
}

/// Match a relative path against a single gitignore pattern.
fn gitignore_match(rel_path: &str, pattern: &str) -> bool {
    let pat = pattern.trim_start_matches('/');
    let path = rel_path.trim_start_matches('/');

    if pat.is_empty() {
        return false;
    }

    // Simple glob matching
    if pat.contains('*') {
        glob_match(path, pat)
    } else {
        // Exact match or directory prefix match
        path == pat || path.starts_with(&format!("{}/", pat))
    }
}

/// Simple glob pattern matching supporting * and **.
fn glob_match(path: &str, pattern: &str) -> bool {
    if pattern == "**" {
        return true;
    }
    if let Some(rest) = pattern.strip_prefix("**/") {
        // Match any directory prefix
        return glob_match(path, rest) || {
            if let Some(slash_idx) = path.find('/') {
                glob_match(&path[slash_idx + 1..], pattern)
            } else {
                glob_match(path, rest)
            }
        };
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix) || path.starts_with(&format!("{}/", prefix));
    }
    if let Some(idx) = pattern.find('*') {
        let before = &pattern[..idx];
        let after = &pattern[idx + 1..];
        if path.starts_with(before) {
            let remaining = &path[before.len()..];
            if after.is_empty() {
                return !remaining.contains('/');
            }
            for i in 0..=remaining.len() {
                if glob_match(&remaining[i..], after) {
                    return true;
                }
            }
        }
        return false;
    }
    path == pattern
}

// ─── Normalize Patterns ──────────────────────────────────────────────────────

fn normalize_pattern_to_path(pattern_root: &str, pattern: &str, root_path: &str) -> Option<String> {
    let full_pattern = format!(
        "{}/{}",
        pattern_root.trim_end_matches('/'),
        pattern.trim_start_matches('/')
    );
    if pattern_root == root_path {
        return Some(prepend_dir_sep(pattern));
    } else if full_pattern.starts_with(&format!("{}/", root_path)) {
        let relative_part = &full_pattern[root_path.len()..];
        return Some(prepend_dir_sep(relative_part));
    } else {
        let rel = compute_posix_relative(root_path, pattern_root);
        if rel.is_empty() || rel.starts_with("../") || rel == ".." {
            return None;
        }
        let relative_pattern = format!("{}/{}", rel, pattern.trim_start_matches('/'));
        return Some(prepend_dir_sep(&relative_pattern));
    }
}

/// Normalize patterns from multiple roots to a single reference root.
pub fn normalize_patterns_to_path(
    patterns_by_root: &HashMap<Option<String>, Vec<String>>,
    root: &str,
) -> Vec<String> {
    let mut result: HashSet<String> = HashSet::new();

    if let Some(null_patterns) = patterns_by_root.get(&None) {
        for p in null_patterns {
            result.insert(p.clone());
        }
    }

    for (pattern_root, patterns) in patterns_by_root {
        if pattern_root.is_none() {
            continue;
        }
        let pr = pattern_root.as_deref().unwrap();
        for pattern in patterns {
            if let Some(normalized) = normalize_pattern_to_path(pr, pattern, root) {
                result.insert(normalized);
            }
        }
    }
    result.into_iter().collect()
}

/// Get file read ignore patterns.
pub fn get_file_read_ignore_patterns(
    tool_permission_context: &ToolPermissionContext,
    ctx: &FsPermissionContext,
) -> HashMap<Option<String>, Vec<String>> {
    let patterns_by_root = get_patterns_by_root(tool_permission_context, "read", "deny", ctx);
    patterns_by_root
        .into_iter()
        .map(|(root, map)| (root, map.into_keys().collect()))
        .collect()
}

// ─── Read/Write Permission Checks ────────────────────────────────────────────

/// Check read permission for a tool given path and context.
pub fn check_read_permission_for_tool(
    tool_name: &str,
    path: &str,
    input: &HashMap<String, serde_json::Value>,
    tool_permission_context: &ToolPermissionContext,
    ctx: &FsPermissionContext,
) -> PermissionDecision {
    let paths_to_check = get_paths_for_permission_check(path);

    // 1. Block UNC paths
    for p in &paths_to_check {
        if p.starts_with("\\\\") || p.starts_with("//") {
            return PermissionDecision::Ask(PermissionAskDecision {
                message: format!(
                    "Mossen requested permissions to read from {}, which appears to be a UNC path that could access network resources.",
                    path
                ),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::Other {
                    reason: "UNC path detected (defense-in-depth check)".to_string(),
                }),
                suggestions: None,
                blocked_path: None,
                metadata: None,
            });
        }
    }

    // 2. Check suspicious Windows patterns
    for p in &paths_to_check {
        if has_suspicious_windows_path_pattern(p, ctx.platform) {
            return PermissionDecision::Ask(PermissionAskDecision {
                message: format!(
                    "Mossen requested permissions to read from {}, which contains a suspicious Windows path pattern that requires manual approval.",
                    path
                ),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::Other {
                    reason: "Path contains suspicious Windows-specific patterns".to_string(),
                }),
                suggestions: None,
                blocked_path: None,
                metadata: None,
            });
        }
    }

    // 3. Check READ deny rules
    for p in &paths_to_check {
        if let Some(deny_rule) =
            matching_rule_for_input(p, tool_permission_context, "read", "deny", ctx)
        {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!("Permission to read {} has been denied.", path),
                decision_reason: PermissionDecisionReason::Rule { rule: deny_rule },
                tool_use_id: None,
            });
        }
    }

    // 4. Check READ ask rules
    for p in &paths_to_check {
        if let Some(ask_rule) =
            matching_rule_for_input(p, tool_permission_context, "read", "ask", ctx)
        {
            return PermissionDecision::Ask(PermissionAskDecision {
                message: format!(
                    "Mossen requested permissions to read from {}, but you haven't granted it yet.",
                    path
                ),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::Rule { rule: ask_rule }),
                suggestions: None,
                blocked_path: None,
                metadata: None,
            });
        }
    }

    // 5. Edit access implies read access
    let edit_result = check_write_permission_for_tool(
        tool_name,
        path,
        input,
        tool_permission_context,
        Some(&paths_to_check),
        ctx,
    );
    if matches!(&edit_result, PermissionDecision::Allow(_)) {
        return edit_result;
    }

    // 6. Allow reads in working directories
    if path_in_allowed_working_path(path, tool_permission_context, Some(&paths_to_check), ctx) {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Mode {
                mode: PermissionMode::Default,
            }),
            tool_use_id: None,
        });
    }

    // 7. Allow reads from internal harness paths
    let absolute_path = expand_path_with_ctx(path, ctx);
    let internal_result = check_readable_internal_path(&absolute_path, input, ctx);
    if !matches!(&internal_result, PermissionResult::Passthrough { .. }) {
        return permission_result_to_decision(internal_result);
    }

    // 8. Check allow rules
    if let Some(allow_rule) =
        matching_rule_for_input(path, tool_permission_context, "read", "allow", ctx)
    {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Rule { rule: allow_rule }),
            tool_use_id: None,
        });
    }

    // Default: ask
    let suggestions = generate_suggestions(
        path,
        "read",
        tool_permission_context,
        Some(&paths_to_check),
        ctx,
    );
    PermissionDecision::Ask(PermissionAskDecision {
        message: format!(
            "Mossen requested permissions to read from {}, but you haven't granted it yet.",
            path
        ),
        updated_input: None,
        decision_reason: Some(PermissionDecisionReason::WorkingDir {
            reason: "Path is outside allowed working directories".to_string(),
        }),
        suggestions: Some(suggestions),
        blocked_path: None,
        metadata: None,
    })
}

/// Check write permission for a tool given path and context.
pub fn check_write_permission_for_tool(
    _tool_name: &str,
    path: &str,
    input: &HashMap<String, serde_json::Value>,
    tool_permission_context: &ToolPermissionContext,
    precomputed_paths: Option<&[String]>,
    ctx: &FsPermissionContext,
) -> PermissionDecision {
    let default_paths;
    let paths_to_check = match precomputed_paths {
        Some(p) => p,
        None => {
            default_paths = get_paths_for_permission_check(path);
            &default_paths
        }
    };

    // 1. Check deny rules
    for p in paths_to_check {
        if let Some(deny_rule) =
            matching_rule_for_input(p, tool_permission_context, "edit", "deny", ctx)
        {
            return PermissionDecision::Deny(PermissionDenyDecision {
                message: format!("Permission to edit {} has been denied.", path),
                decision_reason: PermissionDecisionReason::Rule { rule: deny_rule },
                tool_use_id: None,
            });
        }
    }

    // 1.5. Allow writes to internal editable paths
    let absolute_path = expand_path_with_ctx(path, ctx);
    let internal_result = check_editable_internal_path(&absolute_path, input, ctx);
    if !matches!(&internal_result, PermissionResult::Passthrough { .. }) {
        return permission_result_to_decision(internal_result);
    }

    // 1.6. Check .mossen/** session allow rules BEFORE safety checks
    let session_only_context = ToolPermissionContext {
        always_allow_rules: {
            let mut m = HashMap::new();
            if let Some(session_rules) = tool_permission_context
                .always_allow_rules
                .get(&PermissionRuleSource::Session)
            {
                m.insert(PermissionRuleSource::Session, session_rules.clone());
            }
            m
        },
        ..tool_permission_context.clone()
    };
    if let Some(mossen_rule) =
        matching_rule_for_input(path, &session_only_context, "edit", "allow", ctx)
    {
        if let Some(ref content) = mossen_rule.rule_value.rule_content {
            let mossen_prefix =
                &MOSSEN_FOLDER_PERMISSION_PATTERN[..MOSSEN_FOLDER_PERMISSION_PATTERN.len() - 2];
            let global_prefix = &GLOBAL_MOSSEN_FOLDER_PERMISSION_PATTERN
                [..GLOBAL_MOSSEN_FOLDER_PERMISSION_PATTERN.len() - 2];
            if (content.starts_with(mossen_prefix) || content.starts_with(global_prefix))
                && !content.contains("..")
                && content.ends_with("/**")
            {
                return PermissionDecision::Allow(PermissionAllowDecision {
                    updated_input: Some(input.clone()),
                    decision_reason: Some(PermissionDecisionReason::Rule { rule: mossen_rule }),
                    tool_use_id: None,
                });
            }
        }
    }

    // 1.7. Safety checks
    let safety = check_path_safety_for_auto_edit(path, Some(paths_to_check), ctx);
    if let PathSafetyResult::Unsafe {
        message,
        classifier_approvable,
    } = safety
    {
        let skill_scope = get_mossen_skill_scope(path, ctx);
        let suggestions = if let Some(scope) = skill_scope {
            vec![PermissionUpdate::AddRules {
                destination: PermissionUpdateDestination::Session,
                rules: vec![PermissionRuleValue {
                    tool_name: FILE_EDIT_TOOL_NAME.to_string(),
                    rule_content: Some(scope.pattern),
                }],
                behavior: PermissionBehavior::Allow,
            }]
        } else {
            generate_suggestions(
                path,
                "write",
                tool_permission_context,
                Some(paths_to_check),
                ctx,
            )
        };
        return PermissionDecision::Ask(PermissionAskDecision {
            message: message.clone(),
            updated_input: None,
            decision_reason: Some(PermissionDecisionReason::SafetyCheck {
                reason: message,
                classifier_approvable,
            }),
            suggestions: Some(suggestions),
            blocked_path: None,
            metadata: None,
        });
    }

    // 2. Check ask rules
    for p in paths_to_check {
        if let Some(ask_rule) =
            matching_rule_for_input(p, tool_permission_context, "edit", "ask", ctx)
        {
            return PermissionDecision::Ask(PermissionAskDecision {
                message: format!(
                    "Mossen requested permissions to write to {}, but you haven't granted it yet.",
                    path
                ),
                updated_input: None,
                decision_reason: Some(PermissionDecisionReason::Rule { rule: ask_rule }),
                suggestions: None,
                blocked_path: None,
                metadata: None,
            });
        }
    }

    // 3. AcceptEdits mode + working dir
    let is_in_working_dir =
        path_in_allowed_working_path(path, tool_permission_context, Some(paths_to_check), ctx);
    if tool_permission_context.mode == PermissionMode::AcceptEdits && is_in_working_dir {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Mode {
                mode: tool_permission_context.mode,
            }),
            tool_use_id: None,
        });
    }

    // 4. Check allow rules
    if let Some(allow_rule) =
        matching_rule_for_input(path, tool_permission_context, "edit", "allow", ctx)
    {
        return PermissionDecision::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Rule { rule: allow_rule }),
            tool_use_id: None,
        });
    }

    // 5. Default: ask
    let suggestions = generate_suggestions(
        path,
        "write",
        tool_permission_context,
        Some(paths_to_check),
        ctx,
    );
    PermissionDecision::Ask(PermissionAskDecision {
        message: format!(
            "Mossen requested permissions to write to {}, but you haven't granted it yet.",
            path
        ),
        updated_input: None,
        decision_reason: if !is_in_working_dir {
            Some(PermissionDecisionReason::WorkingDir {
                reason: "Path is outside allowed working directories".to_string(),
            })
        } else {
            None
        },
        suggestions: Some(suggestions),
        blocked_path: None,
        metadata: None,
    })
}

// ─── Suggestion Generation ───────────────────────────────────────────────────

/// Generate permission update suggestions for a path.
pub fn generate_suggestions(
    file_path: &str,
    operation_type: &str,
    tool_permission_context: &ToolPermissionContext,
    precomputed_paths: Option<&[String]>,
    ctx: &FsPermissionContext,
) -> Vec<PermissionUpdate> {
    let is_outside =
        !path_in_allowed_working_path(file_path, tool_permission_context, precomputed_paths, ctx);

    let should_suggest_accept_edits = matches!(
        tool_permission_context.mode,
        PermissionMode::Default | PermissionMode::Plan
    );

    if operation_type == "read" && is_outside {
        let dir_path = get_directory_for_path(file_path);
        let dirs_to_add = get_paths_for_permission_check(&dir_path);
        return dirs_to_add
            .into_iter()
            .map(|dir| create_read_rule_suggestion(&dir, PermissionUpdateDestination::Session))
            .collect();
    }

    if operation_type == "write" || operation_type == "create" {
        let mut updates: Vec<PermissionUpdate> = if should_suggest_accept_edits {
            vec![PermissionUpdate::SetMode {
                destination: PermissionUpdateDestination::Session,
                mode: ExternalPermissionMode::AcceptEdits,
            }]
        } else {
            vec![]
        };

        if is_outside {
            let dir_path = get_directory_for_path(file_path);
            let dirs_to_add = get_paths_for_permission_check(&dir_path);
            updates.push(PermissionUpdate::AddDirectories {
                destination: PermissionUpdateDestination::Session,
                directories: dirs_to_add,
            });
        }
        return updates;
    }

    if should_suggest_accept_edits {
        vec![PermissionUpdate::SetMode {
            destination: PermissionUpdateDestination::Session,
            mode: ExternalPermissionMode::AcceptEdits,
        }]
    } else {
        vec![]
    }
}

fn create_read_rule_suggestion(
    dir: &str,
    destination: PermissionUpdateDestination,
) -> PermissionUpdate {
    PermissionUpdate::AddRules {
        destination,
        rules: vec![PermissionRuleValue {
            tool_name: FILE_READ_TOOL_NAME.to_string(),
            rule_content: Some(format!("{}/**", dir)),
        }],
        behavior: PermissionBehavior::Allow,
    }
}

// ─── Internal Path Checks ────────────────────────────────────────────────────

/// Check if a path is an internal editable path (plan files, scratchpad, etc.).
pub fn check_editable_internal_path(
    absolute_path: &str,
    input: &HashMap<String, serde_json::Value>,
    ctx: &FsPermissionContext,
) -> PermissionResult {
    let normalized = normalize_path(absolute_path);

    // Plan files
    if is_session_plan_file(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Plan files for current session are allowed for writing".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Scratchpad
    if is_scratchpad_path(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Scratchpad files for current session are allowed for writing".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Template job directory
    if ctx.templates_enabled {
        if let Some(ref job_dir) = ctx.mossen_job_dir {
            let jobs_root = format!("{}/jobs", ctx.mossen_config_home_dir);
            let job_dir_forms = get_paths_for_permission_check(job_dir);
            let jobs_root_forms = get_paths_for_permission_check(&jobs_root);
            let sep = std::path::MAIN_SEPARATOR;

            let is_under_jobs_root = job_dir_forms.iter().all(|jd| {
                let njd = normalize_path(jd);
                jobs_root_forms
                    .iter()
                    .any(|jr| njd.starts_with(&format!("{}{}", normalize_path(jr), sep)))
            });

            if is_under_jobs_root {
                let target_forms = get_paths_for_permission_check(absolute_path);
                let all_inside = target_forms.iter().all(|p| {
                    let np = normalize_path(p);
                    job_dir_forms.iter().any(|jd| {
                        let njd = normalize_path(jd);
                        np == njd || np.starts_with(&format!("{}{}", njd, sep))
                    })
                });
                if all_inside {
                    return PermissionResult::Allow(PermissionAllowDecision {
                        updated_input: Some(input.clone()),
                        decision_reason: Some(PermissionDecisionReason::Other {
                            reason: "Job directory files for current job are allowed for writing"
                                .to_string(),
                        }),
                        tool_use_id: None,
                    });
                }
            }
        }
    }

    // .mossen/launch.json
    let launch_path = format!("{}/.mossen/launch.json", ctx.original_cwd);
    if normalize_case_for_comparison(&normalized)
        == normalize_case_for_comparison(&normalize_path(&launch_path))
    {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Preview launch config is allowed for writing".to_string(),
            }),
            tool_use_id: None,
        });
    }

    PermissionResult::Passthrough {
        message: String::new(),
        decision_reason: None,
        suggestions: None,
        blocked_path: None,
    }
}

/// Check if a path is an internal readable path.
pub fn check_readable_internal_path(
    absolute_path: &str,
    input: &HashMap<String, serde_json::Value>,
    ctx: &FsPermissionContext,
) -> PermissionResult {
    let normalized = normalize_path(absolute_path);

    // Session memory
    if is_session_memory_path(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Session memory files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Project directory
    if is_project_dir_path(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Project directory files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Plan files
    if is_session_plan_file(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Plan files for current session are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Tool results directory
    let tool_results_dir = &ctx.tool_results_dir;
    let tr_with_sep = if tool_results_dir.ends_with(MAIN_SEPARATOR) {
        tool_results_dir.clone()
    } else {
        format!("{}{}", tool_results_dir, MAIN_SEPARATOR)
    };
    if normalized == *tool_results_dir || normalized.starts_with(&tr_with_sep) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Tool result files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Scratchpad
    if is_scratchpad_path(&normalized, ctx) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Scratchpad files for current session are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Project temp dir
    let project_temp = get_project_temp_dir(ctx);
    if normalized.starts_with(&project_temp) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Project temp directory files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Tasks directory
    let tasks_dir = format!("{}/tasks/", ctx.mossen_config_home_dir);
    if normalized == tasks_dir.trim_end_matches('/') || normalized.starts_with(&tasks_dir) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Task files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Teams directory
    let teams_dir = format!("{}/teams/", ctx.mossen_config_home_dir);
    if normalized == teams_dir.trim_end_matches('/') || normalized.starts_with(&teams_dir) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Team files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    // Bundled skills root
    let bundled_root = format!("{}/", ctx.bundled_skills_root);
    if normalized.starts_with(&bundled_root) {
        return PermissionResult::Allow(PermissionAllowDecision {
            updated_input: Some(input.clone()),
            decision_reason: Some(PermissionDecisionReason::Other {
                reason: "Bundled skill reference files are allowed for reading".to_string(),
            }),
            tool_use_id: None,
        });
    }

    PermissionResult::Passthrough {
        message: String::new(),
        decision_reason: None,
        suggestions: None,
        blocked_path: None,
    }
}

// ─── Utility Functions ───────────────────────────────────────────────────────

/// Expand a path (resolve ~ and make absolute).
pub fn expand_path_with_ctx(path: &str, ctx: &FsPermissionContext) -> String {
    if path.starts_with('~') {
        return format!("{}{}", ctx.home_dir, &path[1..]);
    }
    if Path::new(path).is_absolute() {
        return path.to_string();
    }
    format!("{}/{}", ctx.original_cwd, path)
}

/// Normalize a path (resolve . and .. components).
pub fn normalize_path(path: &str) -> String {
    let p = PathBuf::from(path);
    // Use lexical normalization
    let mut components = Vec::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !components.is_empty() {
                    components.pop();
                }
            }
            other => components.push(other),
        }
    }
    let result: PathBuf = components.into_iter().collect();
    result.to_string_lossy().to_string()
}

/// Get the directory portion of a path.
fn get_directory_for_path(path: &str) -> String {
    if let Some(idx) = path.rfind('/') {
        path[..idx].to_string()
    } else if let Some(idx) = path.rfind('\\') {
        path[..idx].to_string()
    } else {
        ".".to_string()
    }
}

/// Sanitize a path for use as a directory name component.
fn sanitize_path(path: &str) -> String {
    path.replace(['/', '\\', ':'], "_")
}

/// Get paths for permission check (original + resolved symlinks).
/// In production this would resolve symlinks; here we return the path itself.
pub fn get_paths_for_permission_check(path: &str) -> Vec<String> {
    let expanded = if path.starts_with('~') || Path::new(path).is_absolute() {
        path.to_string()
    } else {
        path.to_string()
    };
    // Try to resolve symlinks
    match std::fs::canonicalize(&expanded) {
        Ok(resolved) => {
            let resolved_str = resolved.to_string_lossy().to_string();
            if resolved_str == expanded {
                vec![expanded]
            } else {
                vec![expanded, resolved_str]
            }
        }
        Err(_) => vec![expanded],
    }
}

/// Convert PermissionResult to PermissionDecision (dropping Passthrough).
fn permission_result_to_decision(result: PermissionResult) -> PermissionDecision {
    match result {
        PermissionResult::Allow(a) => PermissionDecision::Allow(a),
        PermissionResult::Ask(a) => PermissionDecision::Ask(a),
        PermissionResult::Deny(d) => PermissionDecision::Deny(d),
        PermissionResult::Passthrough { message, .. } => {
            PermissionDecision::Ask(PermissionAskDecision {
                message,
                updated_input: None,
                decision_reason: None,
                suggestions: None,
                blocked_path: None,
                metadata: None,
            })
        }
    }
}

// =============================================================================
// 与 TS `permissions/filesystem.ts` 对齐的补充入口。
// =============================================================================

/// scratchpad 是否启用（对应 TS `isScratchpadEnabled`）。
pub fn is_scratchpad_enabled() -> bool {
    !matches!(
        std::env::var("MOSSEN_DISABLE_SCRATCHPAD").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 确保 scratchpad 目录存在并返回路径（对应 TS `ensureScratchpadDir`）。
pub async fn ensure_scratchpad_dir(ctx: &FsPermissionContext) -> std::io::Result<String> {
    let dir = get_scratchpad_dir(ctx);
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// 返回 mossen 临时目录（对应 TS `getMossenTempDir`）。
pub fn get_mossen_temp_dir(ctx: &FsPermissionContext) -> String {
    let name = get_mossen_temp_dir_name(ctx);
    std::env::temp_dir()
        .join(name)
        .to_string_lossy()
        .to_string()
}

/// 返回 bundled skills 根目录（对应 TS `getBundledSkillsRoot`）。
pub fn get_bundled_skills_root() -> String {
    if let Ok(p) = std::env::var("MOSSEN_BUNDLED_SKILLS_ROOT") {
        return p;
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
    {
        return exe_dir.join("skills").to_string_lossy().to_string();
    }
    "/usr/local/share/mossen/skills".to_string()
}

/// 解析所有可写工作目录的绝对路径集合（对应 TS `getResolvedWorkingDirPaths`）。
pub fn get_resolved_working_dir_paths(
    ctx: &ToolPermissionContext,
    original_cwd: &str,
) -> Vec<String> {
    let set = all_working_directories(ctx, original_cwd);
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}
