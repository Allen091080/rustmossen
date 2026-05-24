//! MOSSEN.md / memory file discovery, parsing, and processing.
//!
//! Translates `utils/mossenmd.ts` — discovers and loads memory instruction files
//! from managed, user, project, and local sources with @include support.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;
use tokio::fs;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MEMORY_INSTRUCTION_PROMPT: &str =
    "Codebase and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.";

pub const MAX_MEMORY_CHARACTER_COUNT: usize = 40000;

const MAX_INCLUDE_DEPTH: usize = 5;

/// File extensions allowed for @include directives.
static TEXT_FILE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let exts = vec![
        ".md",
        ".txt",
        ".text",
        ".json",
        ".yaml",
        ".yml",
        ".toml",
        ".xml",
        ".csv",
        ".html",
        ".htm",
        ".css",
        ".scss",
        ".sass",
        ".less",
        ".js",
        ".ts",
        ".tsx",
        ".jsx",
        ".mjs",
        ".cjs",
        ".mts",
        ".cts",
        ".py",
        ".pyi",
        ".pyw",
        ".rb",
        ".erb",
        ".rake",
        ".go",
        ".rs",
        ".java",
        ".kt",
        ".kts",
        ".scala",
        ".c",
        ".cpp",
        ".cc",
        ".cxx",
        ".h",
        ".hpp",
        ".hxx",
        ".cs",
        ".swift",
        ".sh",
        ".bash",
        ".zsh",
        ".fish",
        ".ps1",
        ".bat",
        ".cmd",
        ".env",
        ".ini",
        ".cfg",
        ".conf",
        ".config",
        ".properties",
        ".sql",
        ".graphql",
        ".gql",
        ".proto",
        ".vue",
        ".svelte",
        ".astro",
        ".ejs",
        ".hbs",
        ".pug",
        ".jade",
        ".php",
        ".pl",
        ".pm",
        ".lua",
        ".r",
        ".R",
        ".dart",
        ".ex",
        ".exs",
        ".erl",
        ".hrl",
        ".clj",
        ".cljs",
        ".cljc",
        ".edn",
        ".hs",
        ".lhs",
        ".elm",
        ".ml",
        ".mli",
        ".f",
        ".f90",
        ".f95",
        ".for",
        ".cmake",
        ".make",
        ".makefile",
        ".gradle",
        ".sbt",
        ".rst",
        ".adoc",
        ".asciidoc",
        ".org",
        ".tex",
        ".latex",
        ".lock",
        ".log",
        ".diff",
        ".patch",
    ];
    exts.into_iter().collect()
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemoryType {
    User,
    Project,
    Local,
    Managed,
    AutoMem,
    TeamMem,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::User => write!(f, "User"),
            MemoryType::Project => write!(f, "Project"),
            MemoryType::Local => write!(f, "Local"),
            MemoryType::Managed => write!(f, "Managed"),
            MemoryType::AutoMem => write!(f, "AutoMem"),
            MemoryType::TeamMem => write!(f, "TeamMem"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryFileInfo {
    pub path: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub parent: Option<String>,
    pub globs: Option<Vec<String>>,
    pub content_differs_from_disk: bool,
    pub raw_content: Option<String>,
}

pub type InstructionsLoadReason = &'static str;
pub type InstructionsMemoryType = MemoryType;

#[derive(Debug, Clone)]
pub struct ExternalMossenMdInclude {
    pub path: String,
    pub parent: String,
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------

static HAS_LOGGED_INITIAL_LOAD: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
static NEXT_EAGER_LOAD_REASON: Lazy<Mutex<&'static str>> =
    Lazy::new(|| Mutex::new("session_start"));
static SHOULD_FIRE_HOOK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(true));

// Memoized cache for memory files
static MEMORY_FILES_CACHE: Lazy<Mutex<Option<Vec<MemoryFileInfo>>>> =
    Lazy::new(|| Mutex::new(None));

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn path_in_original_cwd(path: &str, original_cwd: &str) -> bool {
    path.starts_with(original_cwd)
}

fn normalize_path_for_comparison(path: &str) -> String {
    #[cfg(windows)]
    {
        path.to_lowercase().replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

fn get_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct ParsedFrontmatter {
    paths: Option<String>,
}

#[derive(Debug, Clone)]
struct FrontmatterResult {
    frontmatter: ParsedFrontmatter,
    content: String,
}

fn parse_frontmatter(raw_content: &str) -> FrontmatterResult {
    if !raw_content.starts_with("---") {
        return FrontmatterResult {
            frontmatter: ParsedFrontmatter::default(),
            content: raw_content.to_string(),
        };
    }

    // Find closing ---
    let after_start = &raw_content[3..];
    if let Some(end_pos) = after_start.find("\n---") {
        let fm_content = &after_start[..end_pos];
        let content_start = 3 + end_pos + 4; // "---\n" + fm_content + "\n---"
        let content = if content_start < raw_content.len() {
            raw_content[content_start..]
                .trim_start_matches('\n')
                .to_string()
        } else {
            String::new()
        };

        // Parse paths from frontmatter
        let paths = fm_content
            .lines()
            .find(|l| l.trim_start().starts_with("paths:"))
            .map(|l| {
                l.trim_start()
                    .strip_prefix("paths:")
                    .unwrap_or("")
                    .trim()
                    .to_string()
            });

        FrontmatterResult {
            frontmatter: ParsedFrontmatter { paths },
            content,
        }
    } else {
        FrontmatterResult {
            frontmatter: ParsedFrontmatter::default(),
            content: raw_content.to_string(),
        }
    }
}

fn split_path_in_frontmatter(paths_str: &str) -> Vec<String> {
    paths_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse raw content to extract content and glob patterns from frontmatter.
fn parse_frontmatter_paths(raw_content: &str) -> (String, Option<Vec<String>>) {
    let result = parse_frontmatter(raw_content);

    if result.frontmatter.paths.is_none() {
        return (result.content, None);
    }

    let paths_str = result.frontmatter.paths.unwrap();
    let patterns: Vec<String> = split_path_in_frontmatter(&paths_str)
        .into_iter()
        .map(|p| {
            if p.ends_with("/**") {
                p[..p.len() - 3].to_string()
            } else {
                p
            }
        })
        .filter(|p| !p.is_empty())
        .collect();

    if patterns.is_empty() || patterns.iter().all(|p| p == "**") {
        return (result.content, None);
    }

    (result.content, Some(patterns))
}

// ---------------------------------------------------------------------------
// HTML comment stripping
// ---------------------------------------------------------------------------

/// Strip block-level HTML comments from markdown content.
pub fn strip_html_comments(content: &str) -> (String, bool) {
    if !content.contains("<!--") {
        return (content.to_string(), false);
    }

    let comment_re = Regex::new(r"(?s)<!--.*?-->").unwrap();
    let mut result = String::new();
    let mut stripped = false;
    let _last_end = 0;

    // Simple block-level comment stripping
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("<!--") && trimmed.contains("-->") {
            let residue = comment_re.replace_all(line, "");
            stripped = true;
            if residue.trim().is_empty() {
                continue;
            }
            result.push_str(&residue);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    (result, stripped)
}

// ---------------------------------------------------------------------------
// @include extraction
// ---------------------------------------------------------------------------

/// Extract @path include references from content and resolve to absolute paths.
fn extract_include_paths(content: &str, base_path: &str) -> Vec<String> {
    let mut paths = HashSet::new();
    let include_re = Regex::new(r"(?:^|\s)@((?:[^\s\\]|\\ )+)").unwrap();

    for cap in include_re.captures_iter(content) {
        let mut path = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if path.is_empty() {
            continue;
        }

        // Strip fragment identifiers
        if let Some(hash_idx) = path.find('#') {
            path = path[..hash_idx].to_string();
        }
        if path.is_empty() {
            continue;
        }

        // Unescape spaces
        path = path.replace("\\ ", " ");

        // Validate path format
        let is_valid = path.starts_with("./")
            || path.starts_with("~/")
            || (path.starts_with('/') && path != "/")
            || (!path.starts_with('@')
                && !path.starts_with(|c: char| "#%^&*()".contains(c))
                && path.starts_with(|c: char| c.is_alphanumeric() || "._-".contains(c)));

        if is_valid {
            let resolved = expand_path(&path, base_path);
            paths.insert(resolved);
        }
    }

    paths.into_iter().collect()
}

fn expand_path(path: &str, base_dir: &str) -> String {
    if path.starts_with("~/") {
        let home = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string());
        format!("{}{}", home, &path[1..])
    } else if path.starts_with('/') {
        path.to_string()
    } else if path.starts_with("./") {
        let parent = Path::new(base_dir)
            .parent()
            .unwrap_or_else(|| Path::new(base_dir));
        parent.join(&path[2..]).to_string_lossy().to_string()
    } else {
        let parent = Path::new(base_dir)
            .parent()
            .unwrap_or_else(|| Path::new(base_dir));
        parent.join(path).to_string_lossy().to_string()
    }
}

// ---------------------------------------------------------------------------
// Memory file parsing
// ---------------------------------------------------------------------------

/// Parse raw memory file content into a MemoryFileInfo. Pure function — no I/O.
fn parse_memory_file_content(
    raw_content: &str,
    file_path: &str,
    memory_type: MemoryType,
    include_base_path: Option<&str>,
) -> (Option<MemoryFileInfo>, Vec<String>) {
    // Skip non-text files
    let ext = get_extension(file_path);
    if !ext.is_empty() && !TEXT_FILE_EXTENSIONS.contains(ext.as_str()) {
        debug!("Skipping non-text file in @include: {}", file_path);
        return (None, Vec::new());
    }

    let (without_frontmatter, paths) = parse_frontmatter_paths(raw_content);

    // Strip HTML comments
    let (stripped_content, _) = strip_html_comments(&without_frontmatter);

    // Extract include paths
    let include_paths = if let Some(base) = include_base_path {
        extract_include_paths(&without_frontmatter, base)
    } else {
        Vec::new()
    };

    // Truncate for AutoMem/TeamMem types (simplified)
    let final_content = stripped_content;

    let content_differs = final_content != raw_content;

    let info = MemoryFileInfo {
        path: file_path.to_string(),
        memory_type,
        content: final_content,
        parent: None,
        globs: paths,
        content_differs_from_disk: content_differs,
        raw_content: if content_differs {
            Some(raw_content.to_string())
        } else {
            None
        },
    };

    (Some(info), include_paths)
}

fn handle_memory_file_read_error(error: &std::io::Error, _file_path: &str) {
    match error.kind() {
        std::io::ErrorKind::NotFound => {} // Expected
        std::io::ErrorKind::PermissionDenied => {
            warn!("Permission denied reading memory file");
        }
        _ => {}
    }
}

/// Read and parse a memory file asynchronously.
async fn safely_read_memory_file_async(
    file_path: &str,
    memory_type: MemoryType,
    include_base_path: Option<&str>,
) -> (Option<MemoryFileInfo>, Vec<String>) {
    match fs::read_to_string(file_path).await {
        Ok(raw_content) => {
            parse_memory_file_content(&raw_content, file_path, memory_type, include_base_path)
        }
        Err(e) => {
            handle_memory_file_read_error(&e, file_path);
            (None, Vec::new())
        }
    }
}

// ---------------------------------------------------------------------------
// Exclude pattern matching
// ---------------------------------------------------------------------------

/// Check if a file path is excluded by mossenMdExcludes patterns.
fn is_mossen_md_excluded(
    file_path: &str,
    memory_type: &MemoryType,
    exclude_patterns: &[String],
) -> bool {
    if !matches!(
        memory_type,
        MemoryType::User | MemoryType::Project | MemoryType::Local
    ) {
        return false;
    }

    if exclude_patterns.is_empty() {
        return false;
    }

    let normalized = file_path.replace('\\', "/");
    for pattern in exclude_patterns {
        if simple_glob_match_path(pattern, &normalized) {
            return true;
        }
    }
    false
}

fn simple_glob_match_path(pattern: &str, path: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    if pattern.contains('*') {
        if pattern.starts_with("**/") {
            let suffix = &pattern[3..];
            return path.ends_with(suffix) || path.contains(&format!("/{}", suffix));
        }
        // Simple wildcard
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }
    path == pattern || path.starts_with(&format!("{}/", pattern))
}

// ---------------------------------------------------------------------------
// Recursive memory file processing
// ---------------------------------------------------------------------------

/// Recursively processes a memory file and all its @include references.
pub async fn process_memory_file(
    file_path: &str,
    memory_type: MemoryType,
    processed_paths: &mut HashSet<String>,
    include_external: bool,
    depth: usize,
    parent: Option<&str>,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let normalized_path = normalize_path_for_comparison(file_path);
    if processed_paths.contains(&normalized_path) || depth >= MAX_INCLUDE_DEPTH {
        return Vec::new();
    }

    if is_mossen_md_excluded(file_path, &memory_type, exclude_patterns) {
        return Vec::new();
    }

    processed_paths.insert(normalized_path);

    let (info, include_paths) =
        safely_read_memory_file_async(file_path, memory_type.clone(), Some(file_path)).await;

    let Some(mut memory_file) = info else {
        return Vec::new();
    };

    if memory_file.content.trim().is_empty() {
        return Vec::new();
    }

    if let Some(p) = parent {
        memory_file.parent = Some(p.to_string());
    }

    let mut result = vec![memory_file];

    for include_path in &include_paths {
        let is_external = !path_in_original_cwd(include_path, original_cwd);
        if is_external && !include_external {
            continue;
        }

        let included = Box::pin(process_memory_file(
            include_path,
            memory_type.clone(),
            processed_paths,
            include_external,
            depth + 1,
            Some(file_path),
            original_cwd,
            exclude_patterns,
        ))
        .await;
        result.extend(included);
    }

    result
}

// ---------------------------------------------------------------------------
// Rules directory processing
// ---------------------------------------------------------------------------

/// Process all .md files in a rules directory and subdirectories.
pub async fn process_md_rules(
    rules_dir: &str,
    memory_type: MemoryType,
    processed_paths: &mut HashSet<String>,
    include_external: bool,
    conditional_rule: bool,
    visited_dirs: &mut HashSet<String>,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    if visited_dirs.contains(rules_dir) {
        return Vec::new();
    }
    visited_dirs.insert(rules_dir.to_string());

    let mut result = Vec::new();

    let mut entries = match fs::read_dir(rules_dir).await {
        Ok(rd) => rd,
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {}
                _ => {}
            }
            return Vec::new();
        }
    };

    let mut dir_entries = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        dir_entries.push(entry);
    }

    for entry in dir_entries {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        if let Ok(meta) = entry.metadata().await {
            if meta.is_dir() {
                let sub_results = Box::pin(process_md_rules(
                    &path_str,
                    memory_type.clone(),
                    processed_paths,
                    include_external,
                    conditional_rule,
                    visited_dirs,
                    original_cwd,
                    exclude_patterns,
                ))
                .await;
                result.extend(sub_results);
            } else if meta.is_file() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".md") {
                    let files = process_memory_file(
                        &path_str,
                        memory_type.clone(),
                        processed_paths,
                        include_external,
                        0,
                        None,
                        original_cwd,
                        exclude_patterns,
                    )
                    .await;
                    let filtered: Vec<MemoryFileInfo> = files
                        .into_iter()
                        .filter(|f| {
                            if conditional_rule {
                                f.globs.is_some()
                            } else {
                                f.globs.is_none()
                            }
                        })
                        .collect();
                    result.extend(filtered);
                }
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Conditioned rules processing
// ---------------------------------------------------------------------------

/// Process rules that match a target path via frontmatter globs.
pub async fn process_conditioned_md_rules(
    target_path: &str,
    rules_dir: &str,
    memory_type: MemoryType,
    processed_paths: &mut HashSet<String>,
    include_external: bool,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let mut visited_dirs = HashSet::new();
    let all_files = process_md_rules(
        rules_dir,
        memory_type.clone(),
        processed_paths,
        include_external,
        true,
        &mut visited_dirs,
        original_cwd,
        exclude_patterns,
    )
    .await;

    // Filter by glob match
    all_files
        .into_iter()
        .filter(|file| {
            let Some(ref globs) = file.globs else {
                return false;
            };
            if globs.is_empty() {
                return false;
            }

            let base_dir = if matches!(memory_type, MemoryType::Project) {
                Path::new(rules_dir)
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| original_cwd.to_string())
            } else {
                original_cwd.to_string()
            };

            let relative = if Path::new(target_path).is_absolute() {
                pathdiff::diff_paths(target_path, &base_dir)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
            } else {
                target_path.to_string()
            };

            if relative.is_empty()
                || relative.starts_with("..")
                || Path::new(&relative).is_absolute()
            {
                return false;
            }

            globs.iter().any(|g| simple_glob_match_path(g, &relative))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Memory file candidate processing
// ---------------------------------------------------------------------------

async fn process_memory_file_candidates(
    file_paths: &[String],
    memory_type: MemoryType,
    processed_paths: &mut HashSet<String>,
    include_external: bool,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let mut result = Vec::new();
    for path in file_paths {
        let files = process_memory_file(
            path,
            memory_type.clone(),
            processed_paths,
            include_external,
            0,
            None,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(files);
    }
    result
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Get all memory files (memoized).
pub async fn get_memory_files(
    force_include_external: bool,
    original_cwd: &str,
    managed_path: Option<&str>,
    home_candidates: &[String],
    project_candidates_fn: impl Fn(&str) -> Vec<String>,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    // Check cache
    {
        let cache = MEMORY_FILES_CACHE.lock().unwrap();
        if let Some(ref cached) = *cache {
            return cached.clone();
        }
    }

    let mut result = Vec::new();
    let mut processed_paths = HashSet::new();

    // Process Managed file
    if let Some(managed) = managed_path {
        let managed_candidates = vec![managed.to_string()];
        result.extend(
            process_memory_file_candidates(
                &managed_candidates,
                MemoryType::Managed,
                &mut processed_paths,
                force_include_external,
                original_cwd,
                exclude_patterns,
            )
            .await,
        );
    }

    // Process User files
    result.extend(
        process_memory_file_candidates(
            home_candidates,
            MemoryType::User,
            &mut processed_paths,
            true,
            original_cwd,
            exclude_patterns,
        )
        .await,
    );

    // Process Project and Local files (traverse from CWD up to root)
    let mut dirs = Vec::new();
    let mut current_dir = PathBuf::from(original_cwd);
    loop {
        dirs.push(current_dir.to_string_lossy().to_string());
        if let Some(parent) = current_dir.parent() {
            if parent == current_dir {
                break;
            }
            current_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    // Process from root downward to CWD
    dirs.reverse();
    for dir in &dirs {
        let candidates = project_candidates_fn(dir);
        result.extend(
            process_memory_file_candidates(
                &candidates,
                MemoryType::Project,
                &mut processed_paths,
                force_include_external,
                original_cwd,
                exclude_patterns,
            )
            .await,
        );

        // Process Local
        let local_path = PathBuf::from(dir)
            .join("MOSSEN.local.md")
            .to_string_lossy()
            .to_string();
        let local_files = process_memory_file(
            &local_path,
            MemoryType::Local,
            &mut processed_paths,
            force_include_external,
            0,
            None,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(local_files);
    }

    // Cache the result
    {
        let mut cache = MEMORY_FILES_CACHE.lock().unwrap();
        *cache = Some(result.clone());
    }

    result
}

/// Clear the getMemoryFiles memoize cache without firing hooks.
pub fn clear_memory_file_caches() {
    let mut cache = MEMORY_FILES_CACHE.lock().unwrap();
    *cache = None;
}

/// Reset the memory files cache and mark for hook firing.
pub fn reset_get_memory_files_cache(reason: &'static str) {
    *NEXT_EAGER_LOAD_REASON.lock().unwrap() = reason;
    *SHOULD_FIRE_HOOK.lock().unwrap() = true;
    clear_memory_file_caches();
}

fn consume_next_eager_load_reason() -> Option<&'static str> {
    let mut should_fire = SHOULD_FIRE_HOOK.lock().unwrap();
    if !*should_fire {
        return None;
    }
    *should_fire = false;
    let mut reason = NEXT_EAGER_LOAD_REASON.lock().unwrap();
    let r = *reason;
    *reason = "session_start";
    Some(r)
}

/// Get memory files that exceed the max character count.
pub fn get_large_memory_files(files: &[MemoryFileInfo]) -> Vec<&MemoryFileInfo> {
    files
        .iter()
        .filter(|f| f.content.len() > MAX_MEMORY_CHARACTER_COUNT)
        .collect()
}

/// Filter injected memory files (AutoMem/TeamMem when feature enabled).
pub fn filter_injected_memory_files(
    files: Vec<MemoryFileInfo>,
    skip_memory_index: bool,
) -> Vec<MemoryFileInfo> {
    if !skip_memory_index {
        return files;
    }
    files
        .into_iter()
        .filter(|f| !matches!(f.memory_type, MemoryType::AutoMem | MemoryType::TeamMem))
        .collect()
}

/// Build the combined memory string from memory files.
pub fn get_mossen_mds(
    memory_files: &[MemoryFileInfo],
    filter: Option<&dyn Fn(&MemoryType) -> bool>,
    skip_project_level: bool,
) -> String {
    let mut memories = Vec::new();

    for file in memory_files {
        if let Some(f) = filter {
            if !f(&file.memory_type) {
                continue;
            }
        }
        if skip_project_level && matches!(file.memory_type, MemoryType::Project | MemoryType::Local)
        {
            continue;
        }
        if !file.content.is_empty() {
            let description = match file.memory_type {
                MemoryType::Project => " (project instructions, checked into the codebase)",
                MemoryType::Local => " (user's private project instructions, not checked in)",
                MemoryType::TeamMem => " (shared team memory, synced across the organization)",
                MemoryType::AutoMem => " (user's auto-memory, persists across conversations)",
                _ => " (user's private global instructions for all projects)",
            };

            let content = file.content.trim();
            if matches!(file.memory_type, MemoryType::TeamMem) {
                memories.push(format!(
                    "Contents of {}{}:\n\n<team-memory-content source=\"shared\">\n{}\n</team-memory-content>",
                    file.path, description, content
                ));
            } else {
                memories.push(format!(
                    "Contents of {}{}:\n\n{}",
                    file.path, description, content
                ));
            }
        }
    }

    if memories.is_empty() {
        return String::new();
    }

    format!("{}\n\n{}", MEMORY_INSTRUCTION_PROMPT, memories.join("\n\n"))
}

// ---------------------------------------------------------------------------
// Managed and User conditional rules
// ---------------------------------------------------------------------------

/// Gets managed and user conditional rules matching a target path.
pub async fn get_managed_and_user_conditional_rules(
    target_path: &str,
    processed_paths: &mut HashSet<String>,
    managed_rules_dirs: &[String],
    home_rules_dirs: &[String],
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let mut result = Vec::new();

    for dir in managed_rules_dirs {
        let files = process_conditioned_md_rules(
            target_path,
            dir,
            MemoryType::Managed,
            processed_paths,
            false,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(files);
    }

    for dir in home_rules_dirs {
        let files = process_conditioned_md_rules(
            target_path,
            dir,
            MemoryType::User,
            processed_paths,
            true,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(files);
    }

    result
}

/// Gets memory files for a nested directory between CWD and target.
pub async fn get_memory_files_for_nested_directory(
    dir: &str,
    target_path: &str,
    processed_paths: &mut HashSet<String>,
    project_candidates_fn: impl Fn(&str) -> Vec<String>,
    rules_dirs_fn: impl Fn(&str) -> Vec<String>,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let mut result = Vec::new();

    // Process project files
    let candidates = project_candidates_fn(dir);
    result.extend(
        process_memory_file_candidates(
            &candidates,
            MemoryType::Project,
            processed_paths,
            false,
            original_cwd,
            exclude_patterns,
        )
        .await,
    );

    // Process local
    let local_path = PathBuf::from(dir)
        .join("MOSSEN.local.md")
        .to_string_lossy()
        .to_string();
    let local_files = process_memory_file(
        &local_path,
        MemoryType::Local,
        processed_paths,
        false,
        0,
        None,
        original_cwd,
        exclude_patterns,
    )
    .await;
    result.extend(local_files);

    // Process rules
    let rules_dirs = rules_dirs_fn(dir);
    let mut visited = HashSet::new();
    for rules_dir in &rules_dirs {
        let unconditional = process_md_rules(
            rules_dir,
            MemoryType::Project,
            processed_paths,
            false,
            false,
            &mut visited,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(unconditional);

        let conditional = process_conditioned_md_rules(
            target_path,
            rules_dir,
            MemoryType::Project,
            processed_paths,
            false,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(conditional);
    }

    result
}

/// Gets conditional rules for a CWD-level directory.
pub async fn get_conditional_rules_for_cwd_level_directory(
    dir: &str,
    target_path: &str,
    processed_paths: &mut HashSet<String>,
    rules_dirs_fn: impl Fn(&str) -> Vec<String>,
    original_cwd: &str,
    exclude_patterns: &[String],
) -> Vec<MemoryFileInfo> {
    let rules_dirs = rules_dirs_fn(dir);
    let mut result = Vec::new();
    for rules_dir in &rules_dirs {
        let files = process_conditioned_md_rules(
            target_path,
            rules_dir,
            MemoryType::Project,
            processed_paths,
            false,
            original_cwd,
            exclude_patterns,
        )
        .await;
        result.extend(files);
    }
    result
}

// ---------------------------------------------------------------------------
// External includes
// ---------------------------------------------------------------------------

pub fn get_external_mossen_md_includes(
    files: &[MemoryFileInfo],
    original_cwd: &str,
) -> Vec<ExternalMossenMdInclude> {
    files
        .iter()
        .filter_map(|file| {
            if !matches!(file.memory_type, MemoryType::User) {
                if let Some(ref parent) = file.parent {
                    if !path_in_original_cwd(&file.path, original_cwd) {
                        return Some(ExternalMossenMdInclude {
                            path: file.path.clone(),
                            parent: parent.clone(),
                        });
                    }
                }
            }
            None
        })
        .collect()
}

pub fn has_external_mossen_md_includes(files: &[MemoryFileInfo], original_cwd: &str) -> bool {
    !get_external_mossen_md_includes(files, original_cwd).is_empty()
}

// ---------------------------------------------------------------------------
// Memory file path detection
// ---------------------------------------------------------------------------

/// Check if a file path is a memory file.
pub fn is_memory_file_path(file_path: &str) -> bool {
    let name = Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if name == "MOSSEN.md" || name == "MOSSEN.local.md" {
        return true;
    }

    if name.ends_with(".md") {
        let sep = std::path::MAIN_SEPARATOR;
        let pattern = format!("{}.mossen{}rules{}", sep, sep, sep);
        if file_path.contains(&pattern) {
            return true;
        }
    }

    false
}

/// Get all memory file paths from files and readFileState.
pub fn get_all_memory_file_paths(
    files: &[MemoryFileInfo],
    read_file_state_paths: &[String],
) -> Vec<String> {
    let mut paths = HashSet::new();
    for file in files {
        if !file.content.trim().is_empty() {
            paths.insert(file.path.clone());
        }
    }
    for path in read_file_state_paths {
        if is_memory_file_path(path) {
            paths.insert(path.clone());
        }
    }
    paths.into_iter().collect()
}

fn is_instructions_memory_type(memory_type: &MemoryType) -> bool {
    matches!(
        memory_type,
        MemoryType::User | MemoryType::Project | MemoryType::Local | MemoryType::Managed
    )
}

/// 对应 TS `shouldShowMossenMdExternalIncludesWarning`：判断是否需要在 UI 中
/// 展示外部包含警告。
///
/// 调用方传入 project 配置中的两个 flag 以及当前 MOSSEN.md 内容片段，函数检
/// 查是否存在外部 `@import` 引用。
pub fn should_show_mossen_md_external_includes_warning(
    has_external_includes_approved: bool,
    has_external_includes_warning_shown: bool,
    memory_files_content: &[String],
) -> bool {
    if has_external_includes_approved || has_external_includes_warning_shown {
        return false;
    }
    let re = regex::Regex::new(r"@import\s+([^\s]+)").unwrap();
    memory_files_content.iter().any(|content| {
        re.captures_iter(content).any(|cap| {
            let import_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            import_path.starts_with('/') || import_path.starts_with("../")
        })
    })
}
