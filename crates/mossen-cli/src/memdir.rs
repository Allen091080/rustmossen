// memdir.rs — Translation of memdir/ directory:
// memdir/memoryTypes.ts, memdir/memoryAge.ts, memdir/memoryScan.ts,
// memdir/paths.ts, memdir/teamMemPaths.ts, memdir/teamMemPrompts.ts,
// memdir/memdir.ts, memdir/findRelevantMemories.ts

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ============================================================================
// memoryTypes.ts — Memory Type Taxonomy
// ============================================================================

pub const MEMORY_TYPES: &[&str] = &["user", "feedback", "project", "reference"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::User => "user",
            MemoryType::Feedback => "feedback",
            MemoryType::Project => "project",
            MemoryType::Reference => "reference",
        }
    }
}

pub fn parse_memory_type(raw: &str) -> Option<MemoryType> {
    match raw {
        "user" => Some(MemoryType::User),
        "feedback" => Some(MemoryType::Feedback),
        "project" => Some(MemoryType::Project),
        "reference" => Some(MemoryType::Reference),
        _ => None,
    }
}

pub fn types_section_combined() -> Vec<String> {
    vec![
        "## Types of memory".into(),
        String::new(),
        "There are several discrete types of memory that you can store in your memory system. Each type below declares a <scope> of `private`, `team`, or guidance for choosing between the two.".into(),
        String::new(),
        "<types>".into(),
        "<type>".into(),
        "    <name>user</name>".into(),
        "    <scope>always private</scope>".into(),
        "    <description>Contain information about the user's role, goals, responsibilities, and knowledge.</description>".into(),
        "    <when_to_save>When you learn any details about the user's role, preferences, responsibilities, or knowledge</when_to_save>".into(),
        "    <how_to_use>When your work should be informed by the user's profile or perspective.</how_to_use>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>feedback</name>".into(),
        "    <scope>default to private</scope>".into(),
        "    <description>Guidance the user has given you about how to approach work — both what to avoid and what to keep doing.</description>".into(),
        "    <when_to_save>Any time the user corrects your approach or confirms a non-obvious approach worked.</when_to_save>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>project</name>".into(),
        "    <scope>private or team, but strongly bias toward team</scope>".into(),
        "    <description>Information about ongoing work, goals, initiatives, bugs, incidents, or cross-session handoffs.</description>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>reference</name>".into(),
        "    <scope>usually team</scope>".into(),
        "    <description>Stores pointers to where information can be found in external systems.</description>".into(),
        "</type>".into(),
        "</types>".into(),
        String::new(),
    ]
}

pub fn types_section_individual() -> Vec<String> {
    vec![
        "## Types of memory".into(),
        String::new(),
        "There are several discrete types of memory that you can store in your memory system:".into(),
        String::new(),
        "<types>".into(),
        "<type>".into(),
        "    <name>user</name>".into(),
        "    <description>Contain information about the user's role, goals, responsibilities, and knowledge.</description>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>feedback</name>".into(),
        "    <description>Guidance the user has given you about how to approach work.</description>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>project</name>".into(),
        "    <description>Information about ongoing work, goals, initiatives, bugs, incidents, or cross-session handoffs.</description>".into(),
        "</type>".into(),
        "<type>".into(),
        "    <name>reference</name>".into(),
        "    <description>Stores pointers to where information can be found in external systems.</description>".into(),
        "</type>".into(),
        "</types>".into(),
        String::new(),
    ]
}

pub fn what_not_to_save_section() -> Vec<String> {
    vec![
        "## What NOT to save in memory".into(),
        String::new(),
        "- Code patterns, conventions, architecture, file paths, or project structure — these can be derived by reading the current project state.".into(),
        "- Git history, recent changes, or who-changed-what — `git log` / `git blame` are authoritative.".into(),
        "- Debugging solutions or fix recipes — the fix is in the code; the commit message has the context.".into(),
        "- Anything already documented in MOSSEN.md files.".into(),
        "- Ephemeral task details: in-progress work, temporary state, current conversation context, unless they form a concise project handoff.".into(),
        String::new(),
        "These exclusions apply even when the user explicitly asks you to save.".into(),
    ]
}

pub const MEMORY_DRIFT_CAVEAT: &str = "- Memory records can become stale over time. Use memory as context for what was true at a given point in time. Before answering the user or building assumptions based solely on information in memory records, verify that the memory is still correct and up-to-date.";

pub fn when_to_access_section() -> Vec<String> {
    vec![
        "## When to access memories".into(),
        "- When memories seem relevant, or the user references prior-conversation work.".into(),
        "- You MUST access memory when the user explicitly asks you to check, recall, or remember."
            .into(),
        "- If the user says to *ignore* or *not use* memory: proceed as if MEMORY.md were empty."
            .into(),
        MEMORY_DRIFT_CAVEAT.into(),
    ]
}

pub fn trusting_recall_section() -> Vec<String> {
    vec![
        "## Before recommending from memory".into(),
        String::new(),
        "A memory that names a specific function, file, or flag is a claim that it existed *when the memory was written*. Before recommending it:".into(),
        String::new(),
        "- If the memory names a file path: check the file exists.".into(),
        "- If the memory names a function or flag: grep for it.".into(),
        "- If the user is about to act on your recommendation, verify first.".into(),
        String::new(),
        "\"The memory says X exists\" is not the same as \"X exists now.\"".into(),
    ]
}

pub fn memory_frontmatter_example() -> Vec<String> {
    vec![
        "```markdown".into(),
        "---".into(),
        "name: {{memory name}}".into(),
        "description: {{one-line description}}".into(),
        format!("type: {{{{{}}}}}", MEMORY_TYPES.join(", ")),
        "---".into(),
        String::new(),
        "{{memory content}}".into(),
        "```".into(),
    ]
}

// ============================================================================
// memoryAge.ts — Memory Age Utilities
// ============================================================================

pub fn memory_age_days(mtime_ms: i64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let diff = now - mtime_ms;
    if diff < 0 {
        return 0;
    }
    (diff / 86_400_000) as u64
}

pub fn memory_age(mtime_ms: i64) -> String {
    let d = memory_age_days(mtime_ms);
    match d {
        0 => "today".into(),
        1 => "yesterday".into(),
        _ => format!("{} days ago", d),
    }
}

pub fn memory_freshness_text(mtime_ms: i64) -> String {
    let d = memory_age_days(mtime_ms);
    if d <= 1 {
        return String::new();
    }
    format!(
        "This memory is {} days old. Memories are point-in-time observations, not live state — \
         claims about code behavior or file:line citations may be outdated. \
         Verify against current code before asserting as fact.",
        d
    )
}

pub fn memory_freshness_note(mtime_ms: i64) -> String {
    let text = memory_freshness_text(mtime_ms);
    if text.is_empty() {
        return String::new();
    }
    format!("<system-reminder>{}</system-reminder>\n", text)
}

// ============================================================================
// memoryScan.ts — Memory-directory Scanning
// ============================================================================

#[derive(Debug, Clone)]
pub struct MemoryHeader {
    pub filename: String,
    pub file_path: PathBuf,
    pub mtime_ms: i64,
    pub description: Option<String>,
    pub memory_type: Option<MemoryType>,
}

const MAX_MEMORY_FILES: usize = 200;
const FRONTMATTER_MAX_LINES: usize = 30;

pub async fn scan_memory_files(memory_dir: &Path) -> Vec<MemoryHeader> {
    let entries = match tokio::fs::read_dir(memory_dir).await {
        Ok(mut rd) => {
            let mut files = Vec::new();
            while let Ok(Some(entry)) = rd.next_entry().await {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if name != "MEMORY.md" {
                        files.push(path);
                    }
                }
            }
            files
        }
        Err(_) => return Vec::new(),
    };

    let mut headers = Vec::new();
    for file_path in entries {
        if let Ok(content) = tokio::fs::read_to_string(&file_path).await {
            let mtime_ms = tokio::fs::metadata(&file_path)
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let lines: Vec<&str> = content.lines().take(FRONTMATTER_MAX_LINES).collect();
            let (description, memory_type) = parse_frontmatter_fields(&lines);

            headers.push(MemoryHeader {
                filename: file_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                file_path,
                mtime_ms,
                description,
                memory_type,
            });
        }
    }

    headers.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    headers.truncate(MAX_MEMORY_FILES);
    headers
}

fn parse_frontmatter_fields(lines: &[&str]) -> (Option<String>, Option<MemoryType>) {
    let mut description = None;
    let mut memory_type = None;
    let mut in_frontmatter = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_frontmatter {
                break; // end of frontmatter
            }
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if let Some(val) = trimmed.strip_prefix("description:") {
                description = Some(val.trim().to_string());
            } else if let Some(val) = trimmed.strip_prefix("type:") {
                memory_type = parse_memory_type(val.trim());
            }
        }
    }

    (description, memory_type)
}

pub fn format_memory_manifest(memories: &[MemoryHeader]) -> String {
    memories
        .iter()
        .map(|m| {
            let tag = m
                .memory_type
                .map(|t| format!("[{}] ", t.as_str()))
                .unwrap_or_default();
            let ts = chrono::DateTime::from_timestamp_millis(m.mtime_ms)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();
            if let Some(ref desc) = m.description {
                format!("- {}{} ({}): {}", tag, m.filename, ts, desc)
            } else {
                format!("- {}{} ({})", tag, m.filename, ts)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// paths.ts — Auto-memory Path Resolution
// ============================================================================

pub fn is_auto_memory_enabled() -> bool {
    if let Ok(val) = std::env::var("MOSSEN_CODE_DISABLE_AUTO_MEMORY") {
        if is_env_truthy(&val) {
            return false;
        }
        if is_env_defined_falsy(&val) {
            return true;
        }
    }
    if let Ok(val) = std::env::var("MOSSEN_CODE_SIMPLE") {
        if is_env_truthy(&val) {
            return false;
        }
    }
    if let Ok(remote) = std::env::var("MOSSEN_CODE_REMOTE") {
        if is_env_truthy(&remote) && std::env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR").is_err() {
            return false;
        }
    }
    true
}

pub fn get_memory_base_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR") {
        return PathBuf::from(dir);
    }
    mossen_utils::env::get_mossen_config_home_dir()
}

const AUTO_MEM_DIRNAME: &str = "memory";
const AUTO_MEM_ENTRYPOINT_NAME: &str = "MEMORY.md";

pub fn get_auto_mem_path(project_root: &Path) -> PathBuf {
    if let Ok(override_path) = std::env::var("MOSSEN_COWORK_MEMORY_PATH_OVERRIDE") {
        if let Some(validated) = validate_memory_path(&override_path, false) {
            return validated;
        }
    }
    let base = get_memory_base_dir();
    let sanitized = sanitize_path_for_mem(project_root);
    base.join("projects").join(sanitized).join(AUTO_MEM_DIRNAME)
}

pub fn get_auto_mem_daily_log_path(auto_mem_path: &Path) -> PathBuf {
    let now = chrono::Local::now();
    let yyyy = now.format("%Y").to_string();
    let mm = now.format("%m").to_string();
    let dd = now.format("%Y-%m-%d").to_string();
    auto_mem_path
        .join("logs")
        .join(&yyyy)
        .join(&mm)
        .join(format!("{}.md", dd))
}

pub fn get_auto_mem_entrypoint(project_root: &Path) -> PathBuf {
    get_auto_mem_path(project_root).join(AUTO_MEM_ENTRYPOINT_NAME)
}

pub fn is_auto_mem_path(absolute_path: &Path, project_root: &Path) -> bool {
    let auto_mem = get_auto_mem_path(project_root);
    absolute_path.starts_with(&auto_mem)
}

pub fn has_auto_mem_path_override() -> bool {
    std::env::var("MOSSEN_COWORK_MEMORY_PATH_OVERRIDE")
        .ok()
        .and_then(|v| validate_memory_path(&v, false))
        .is_some()
}

fn path_starts_with_dir(path: &Path, dir: &Path) -> bool {
    path == dir || path.strip_prefix(dir).is_ok()
}

fn validate_memory_path(raw: &str, _expand_tilde: bool) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return None;
    }
    let normalized = path.to_string_lossy().to_string();
    if normalized.len() < 3 {
        return None;
    }
    if normalized.contains('\0') {
        return None;
    }
    Some(path)
}

fn sanitize_path_for_mem(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    s.replace(['/', '\\', ':'], "_")
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

fn is_env_defined_falsy(val: &str) -> bool {
    matches!(val.trim().to_lowercase().as_str(), "0" | "false" | "no")
}

pub fn is_extract_mode_active() -> bool {
    if !is_auto_memory_enabled() {
        return false;
    }
    if let Ok(val) = std::env::var("MOSSEN_CODE_ENABLE_EXTRACT_MEMORIES") {
        if is_env_truthy(&val) {
            return true;
        }
        if is_env_defined_falsy(&val) {
            return false;
        }
    }
    false
}

// ============================================================================
// teamMemPaths.ts — Team Memory Paths
// ============================================================================

#[derive(Debug)]
pub struct PathTraversalError {
    pub message: String,
}

impl std::fmt::Display for PathTraversalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PathTraversalError: {}", self.message)
    }
}

impl std::error::Error for PathTraversalError {}

pub fn sanitize_path_key(key: &str) -> Result<String, PathTraversalError> {
    if key.contains('\0') {
        return Err(PathTraversalError {
            message: format!("Null byte in path key: \"{}\"", key),
        });
    }
    if key.contains('\\') {
        return Err(PathTraversalError {
            message: format!("Backslash in path key: \"{}\"", key),
        });
    }
    if key.starts_with('/') {
        return Err(PathTraversalError {
            message: format!("Absolute path key: \"{}\"", key),
        });
    }
    if key.contains("..") {
        return Err(PathTraversalError {
            message: format!("Traversal in path key: \"{}\"", key),
        });
    }
    Ok(key.to_string())
}

pub fn is_team_memory_enabled() -> bool {
    is_auto_memory_enabled() && is_team_memory_rollout_enabled()
}

pub fn is_team_memory_rollout_enabled() -> bool {
    resolve_team_memory_rollout_enabled(
        env_flag(&["MOSSEN_CODE_DISABLE_TEAM_MEMORY"]),
        env_flag(&[
            "MOSSEN_CODE_ENABLE_TEAM_MEMORY",
            "MOSSEN_TEAM_MEMORY",
            "MOSSEN_MEMORY_TEAM_MEMORY_ENABLED",
            "MOSSEN_TEAM_MEMORY_ENABLED",
        ]),
        mossen_agent::services::team_memory_sync::is_team_memory_sync_available(),
    )
}

fn resolve_team_memory_rollout_enabled(
    disable_flag: Option<bool>,
    enable_flag: Option<bool>,
    sync_available: bool,
) -> bool {
    if disable_flag == Some(true) {
        return false;
    }
    if let Some(enabled) = enable_flag {
        return enabled;
    }
    sync_available
}

fn env_flag(names: &[&str]) -> Option<bool> {
    names.iter().find_map(|name| {
        std::env::var(name).ok().and_then(|value| {
            if is_env_truthy(&value) {
                Some(true)
            } else if is_env_defined_falsy(&value) {
                Some(false)
            } else {
                None
            }
        })
    })
}

pub fn get_team_mem_path(project_root: &Path) -> PathBuf {
    get_auto_mem_path(project_root).join("team")
}

pub fn get_team_mem_entrypoint(project_root: &Path) -> PathBuf {
    get_auto_mem_path(project_root)
        .join("team")
        .join("MEMORY.md")
}

pub fn is_team_mem_path(file_path: &Path, project_root: &Path) -> bool {
    let team_dir = get_team_mem_path(project_root);
    path_starts_with_dir(file_path, &team_dir)
}

pub async fn validate_team_mem_write_path(
    file_path: &Path,
    project_root: &Path,
) -> Result<PathBuf, PathTraversalError> {
    if file_path.to_string_lossy().contains('\0') {
        return Err(PathTraversalError {
            message: format!("Null byte in path: {:?}", file_path),
        });
    }
    let resolved = file_path.canonicalize().map_err(|_| PathTraversalError {
        message: format!("Cannot resolve path: {:?}", file_path),
    })?;
    let team_dir = get_team_mem_path(project_root);
    if !path_starts_with_dir(&resolved, &team_dir) {
        return Err(PathTraversalError {
            message: format!("Path escapes team memory directory: {:?}", file_path),
        });
    }
    Ok(resolved)
}

pub async fn validate_team_mem_key(
    relative_key: &str,
    project_root: &Path,
) -> Result<PathBuf, PathTraversalError> {
    sanitize_path_key(relative_key)?;
    let team_dir = get_team_mem_path(project_root);
    let full_path = team_dir.join(relative_key);
    validate_team_mem_write_path(&full_path, project_root).await
}

pub fn is_team_mem_file(file_path: &Path, project_root: &Path) -> bool {
    is_team_memory_enabled() && is_team_mem_path(file_path, project_root)
}

// ============================================================================
// teamMemPrompts.ts — Combined Memory Prompt Builder
// ============================================================================

pub fn build_combined_memory_prompt(
    project_root: &Path,
    extra_guidelines: Option<&[String]>,
    skip_index: bool,
) -> String {
    let auto_dir = get_auto_mem_path(project_root);
    let team_dir = get_team_mem_path(project_root);
    let auto_dir_str = auto_dir.display();
    let team_dir_str = team_dir.display();

    let mut lines = vec![
        "# Memory".into(),
        String::new(),
        format!("You have a persistent, file-based memory system with two directories: a private directory at `{}` and a shared team directory at `{}`.", auto_dir_str, team_dir_str),
        String::new(),
        "You should build up this memory system over time so that future conversations can have a complete picture of who the user is.".into(),
        String::new(),
        "If the user explicitly asks you to remember something, save it immediately. If they ask you to forget something, find and remove the relevant entry.".into(),
        String::new(),
        "## Memory scope".into(),
        String::new(),
        "There are two scope levels:".into(),
        String::new(),
        format!("- private: memories that are private between you and the current user. Stored at `{}`.", auto_dir_str),
        format!("- team: memories that are shared with all users in this project. Stored at `{}`.", team_dir_str),
        String::new(),
    ];
    lines.extend(types_section_combined());
    lines.extend(what_not_to_save_section());
    lines.push("- You MUST avoid saving sensitive data within shared team memories.".into());
    lines.push(String::new());

    let how_to_save = build_how_to_save_section(skip_index);
    lines.extend(how_to_save);
    lines.push(String::new());

    lines.push("## When to access memories".into());
    lines.push("- When memories seem relevant, or the user references prior work.".into());
    lines.push(
        "- You MUST access memory when the user explicitly asks you to check, recall, or remember."
            .into(),
    );
    lines.push(MEMORY_DRIFT_CAVEAT.into());
    lines.push(String::new());
    lines.extend(trusting_recall_section());
    lines.push(String::new());

    if let Some(guidelines) = extra_guidelines {
        for g in guidelines {
            lines.push(g.clone());
        }
    }

    lines.join("\n")
}

// ============================================================================
// memdir.ts — Memory Directory Management
// ============================================================================

pub const ENTRYPOINT_NAME: &str = "MEMORY.md";
pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;
pub const DIR_EXISTS_GUIDANCE: &str = "This directory already exists — write to it directly with the Write tool (do not run mkdir or check for its existence).";
pub const DIRS_EXIST_GUIDANCE: &str = "Both directories already exist — write to them directly with the Write tool (do not run mkdir or check for their existence).";

#[derive(Debug)]
pub struct EntrypointTruncation {
    pub content: String,
    pub line_count: usize,
    pub byte_count: usize,
    pub was_line_truncated: bool,
    pub was_byte_truncated: bool,
}

pub fn truncate_entrypoint_content(raw: &str) -> EntrypointTruncation {
    let trimmed = raw.trim();
    let content_lines: Vec<&str> = trimmed.split('\n').collect();
    let line_count = content_lines.len();
    let byte_count = trimmed.len();

    let was_line_truncated = line_count > MAX_ENTRYPOINT_LINES;
    let was_byte_truncated = byte_count > MAX_ENTRYPOINT_BYTES;

    if !was_line_truncated && !was_byte_truncated {
        return EntrypointTruncation {
            content: trimmed.to_string(),
            line_count,
            byte_count,
            was_line_truncated,
            was_byte_truncated,
        };
    }

    let mut truncated = if was_line_truncated {
        content_lines[..MAX_ENTRYPOINT_LINES].join("\n")
    } else {
        trimmed.to_string()
    };

    if truncated.len() > MAX_ENTRYPOINT_BYTES {
        let cut_at = truncated[..MAX_ENTRYPOINT_BYTES]
            .rfind('\n')
            .unwrap_or(MAX_ENTRYPOINT_BYTES);
        truncated = truncated[..cut_at].to_string();
    }

    let reason = if was_byte_truncated && !was_line_truncated {
        format!(
            "{} bytes (limit: {}) — index entries are too long",
            byte_count, MAX_ENTRYPOINT_BYTES
        )
    } else if was_line_truncated && !was_byte_truncated {
        format!("{} lines (limit: {})", line_count, MAX_ENTRYPOINT_LINES)
    } else {
        format!("{} lines and {} bytes", line_count, byte_count)
    };

    truncated.push_str(&format!(
        "\n\n> WARNING: {} is {}. Only part of it was loaded.",
        ENTRYPOINT_NAME, reason,
    ));

    EntrypointTruncation {
        content: truncated,
        line_count,
        byte_count,
        was_line_truncated,
        was_byte_truncated,
    }
}

pub async fn ensure_memory_dir_exists(memory_dir: &Path) {
    let _ = tokio::fs::create_dir_all(memory_dir).await;
}

pub fn build_memory_lines(
    display_name: &str,
    memory_dir: &Path,
    extra_guidelines: Option<&[String]>,
    skip_index: bool,
) -> Vec<String> {
    let dir_str = memory_dir.display();
    let mut lines = vec![
        format!("# {}", display_name),
        String::new(),
        format!("You have a persistent, file-based memory system at `{}`. {}", dir_str, DIR_EXISTS_GUIDANCE),
        String::new(),
        "You should build up this memory system over time so that future conversations can have a complete picture of who the user is.".into(),
        String::new(),
        "If the user explicitly asks you to remember something, save it immediately.".into(),
        String::new(),
    ];
    lines.extend(types_section_individual());
    lines.extend(what_not_to_save_section());
    lines.push(String::new());

    lines.extend(build_how_to_save_section(skip_index));
    lines.push(String::new());
    lines.extend(when_to_access_section());
    lines.push(String::new());
    lines.extend(trusting_recall_section());
    lines.push(String::new());

    lines.push("## Memory and other forms of persistence".into());
    lines.push("Memory is one of several persistence mechanisms. The distinction is that memory can be recalled in future conversations.".into());
    lines.push(String::new());

    if let Some(guidelines) = extra_guidelines {
        for g in guidelines {
            lines.push(g.clone());
        }
        lines.push(String::new());
    }

    lines
}

pub fn build_memory_prompt(display_name: &str, memory_dir: &Path) -> String {
    let entrypoint = memory_dir.join(ENTRYPOINT_NAME);
    let entrypoint_content = std::fs::read_to_string(&entrypoint).unwrap_or_default();

    let mut lines = build_memory_lines(display_name, memory_dir, None, false);

    let trimmed = entrypoint_content.trim();
    if !trimmed.is_empty() {
        let t = truncate_entrypoint_content(&entrypoint_content);
        lines.push(format!("## {}", ENTRYPOINT_NAME));
        lines.push(String::new());
        lines.push(t.content);
    } else {
        lines.push(format!("## {}", ENTRYPOINT_NAME));
        lines.push(String::new());
        lines.push(format!(
            "Your {} is currently empty. When you save new memories, they will appear here.",
            ENTRYPOINT_NAME
        ));
    }

    lines.join("\n")
}

pub async fn load_memory_prompt(project_root: &Path) -> Option<String> {
    if !is_auto_memory_enabled() {
        return None;
    }

    if is_team_memory_enabled() {
        let auto_dir = get_auto_mem_path(project_root);
        let team_dir = get_team_mem_path(project_root);
        ensure_memory_dir_exists(&auto_dir).await;
        ensure_memory_dir_exists(&team_dir).await;
        return Some(build_combined_memory_prompt(project_root, None, false));
    }

    let auto_dir = get_auto_mem_path(project_root);
    ensure_memory_dir_exists(&auto_dir).await;
    Some(build_memory_prompt("auto memory", &auto_dir))
}

fn build_how_to_save_section(skip_index: bool) -> Vec<String> {
    if skip_index {
        vec![
            "## How to save memories".into(),
            String::new(),
            "Write each memory to its own file using this frontmatter format:".into(),
            String::new(),
        ]
    } else {
        vec![
            "## How to save memories".into(),
            String::new(),
            "Saving a memory is a two-step process:".into(),
            String::new(),
            "**Step 1** — write the memory to its own file using this frontmatter format:".into(),
            String::new(),
            format!("**Step 2** — add a pointer to that file in `{}`. Each entry should be one line, under ~150 characters.", ENTRYPOINT_NAME),
            String::new(),
            "- Keep the name, description, and type fields in memory files up-to-date with the content".into(),
            "- Organize memory semantically by topic, not chronologically".into(),
            "- Update or remove memories that turn out to be wrong or outdated".into(),
            "- Do not write duplicate memories. First check if there is an existing memory you can update.".into(),
        ]
    }
}

// ============================================================================
// findRelevantMemories.ts — Relevant Memory Selection
// ============================================================================

#[derive(Debug, Clone)]
pub struct RelevantMemory {
    pub path: PathBuf,
    pub mtime_ms: i64,
}

pub async fn find_relevant_memories(
    _query: &str,
    memory_dir: &Path,
    already_surfaced: &std::collections::HashSet<PathBuf>,
) -> Vec<RelevantMemory> {
    let memories = scan_memory_files(memory_dir).await;
    let filtered: Vec<MemoryHeader> = memories
        .into_iter()
        .filter(|m| !already_surfaced.contains(&m.file_path))
        .collect();

    if filtered.is_empty() {
        return Vec::new();
    }

    // In the TS version, this does a sideQuery to Balanced for relevance selection.
    // Here we return all memories (up to 5) sorted by recency as a reasonable default.
    filtered
        .into_iter()
        .take(5)
        .map(|m| RelevantMemory {
            path: m.file_path,
            mtime_ms: m.mtime_ms,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMDIR_ENV_KEYS: &[&str] = &[
        "HOME",
        "MOSSEN_CONFIG_DIR",
        "MOSSEN_COWORK_MEMORY_PATH_OVERRIDE",
        "MOSSEN_CODE_DISABLE_AUTO_MEMORY",
        "MOSSEN_CODE_SIMPLE",
        "MOSSEN_CODE_REMOTE",
        "MOSSEN_CODE_REMOTE_MEMORY_DIR",
        "MOSSEN_CODE_DISABLE_TEAM_MEMORY",
        "MOSSEN_CODE_ENABLE_TEAM_MEMORY",
        "MOSSEN_TEAM_MEMORY",
        "MOSSEN_MEMORY_TEAM_MEMORY_ENABLED",
        "MOSSEN_TEAM_MEMORY_ENABLED",
    ];

    struct EnvGuard(Vec<(&'static str, Option<String>)>);

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..) {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn memdir_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn isolate_memdir_env(root: &Path, auto_mem_dir: &Path) -> EnvGuard {
        let guard = EnvGuard(
            MEMDIR_ENV_KEYS
                .iter()
                .map(|key| (*key, std::env::var(key).ok()))
                .collect(),
        );
        for key in MEMDIR_ENV_KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("HOME", root.join("home"));
        std::env::set_var("MOSSEN_CONFIG_DIR", root.join("home").join(".mossen"));
        std::env::set_var("MOSSEN_COWORK_MEMORY_PATH_OVERRIDE", auto_mem_dir);
        std::env::set_var("MOSSEN_CODE_DISABLE_TEAM_MEMORY", "1");
        guard
    }

    #[test]
    fn team_memory_rollout_uses_explicit_flags_before_sync_availability() {
        assert!(!resolve_team_memory_rollout_enabled(
            Some(true),
            Some(true),
            true
        ));
        assert!(resolve_team_memory_rollout_enabled(None, Some(true), false));
        assert!(!resolve_team_memory_rollout_enabled(
            None,
            Some(false),
            true
        ));
        assert!(resolve_team_memory_rollout_enabled(None, None, true));
        assert!(!resolve_team_memory_rollout_enabled(None, None, false));
    }

    #[test]
    fn team_memory_path_detection_uses_component_boundaries() {
        let project_root = Path::new("/workspace/project");
        let team_dir = get_team_mem_path(project_root);
        assert!(is_team_mem_path(&team_dir.join("MEMORY.md"), project_root));
        assert!(!is_team_mem_path(
            &team_dir
                .parent()
                .expect("team parent")
                .join("team-other")
                .join("MEMORY.md"),
            project_root,
        ));
    }

    #[tokio::test]
    async fn scan_memory_files_parses_all_frontmatter_types() {
        let temp = tempfile::tempdir().expect("tempdir");
        let memory_dir = temp.path().join("memory");
        std::fs::create_dir_all(&memory_dir).expect("memory dir");
        std::fs::write(memory_dir.join("MEMORY.md"), "- index only\n").expect("write index");

        for memory_type in MEMORY_TYPES {
            std::fs::write(
                memory_dir.join(format!("{memory_type}.md")),
                format!(
                    "---\ndescription: {memory_type} description\ntype: {memory_type}\n---\n{memory_type} body\n"
                ),
            )
            .expect("write memory file");
        }

        let headers = scan_memory_files(&memory_dir).await;
        assert_eq!(headers.len(), MEMORY_TYPES.len(), "{headers:#?}");
        assert!(
            headers.iter().all(|header| header.filename != "MEMORY.md"),
            "{headers:#?}"
        );

        for memory_type in MEMORY_TYPES {
            let header = headers
                .iter()
                .find(|header| header.filename == format!("{memory_type}.md"))
                .unwrap_or_else(|| panic!("missing header for {memory_type}: {headers:#?}"));
            assert_eq!(
                header.memory_type.map(|kind| kind.as_str()),
                Some(*memory_type)
            );
            assert_eq!(
                header.description.as_deref(),
                Some(format!("{memory_type} description").as_str())
            );
        }
    }

    #[tokio::test]
    async fn scan_memory_files_reads_written_marker_after_restart() {
        let temp = tempfile::tempdir().expect("tempdir");
        let memory_dir = temp.path().join("memory");
        std::fs::create_dir_all(&memory_dir).expect("memory dir");
        let marker = "MOSSEN_M5_1_USER_PREF_MARKER_xyz";
        std::fs::write(
            memory_dir.join("user_pref_dark_mode.md"),
            format!("---\ndescription: M5.1 fixture\ntype: user\n---\n{marker}\n"),
        )
        .expect("write memory file");

        let headers = scan_memory_files(&memory_dir).await;
        let header = headers
            .iter()
            .find(|header| header.filename == "user_pref_dark_mode.md")
            .expect("written memory file should be visible after fresh scan");
        let content = std::fs::read_to_string(&header.file_path).expect("read scanned memory");
        assert!(content.contains(marker), "{content}");
    }

    #[tokio::test]
    async fn auto_memory_prompt_loads_entrypoint_content_from_override() {
        let _lock = memdir_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let project_root = temp.path().join("project");
        let auto_mem_dir = temp.path().join("automem");
        std::fs::create_dir_all(&project_root).expect("project dir");
        std::fs::create_dir_all(&auto_mem_dir).expect("auto mem dir");
        let _env = isolate_memdir_env(temp.path(), &auto_mem_dir);

        let marker = "MOSSEN_M5_1_ENTRYPOINT_MARKER";
        std::fs::write(auto_mem_dir.join("MEMORY.md"), format!("- {marker}\n"))
            .expect("write memory entrypoint");

        let prompt = load_memory_prompt(&project_root)
            .await
            .expect("auto memory prompt");

        assert!(prompt.contains(marker), "{prompt}");
        assert!(
            prompt.contains(&auto_mem_dir.display().to_string()),
            "{prompt}"
        );
    }
}
