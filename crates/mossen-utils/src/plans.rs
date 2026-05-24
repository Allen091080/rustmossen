use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAX_SLUG_RETRIES: usize = 10;
const PROMPT_SLUG_MAX_LENGTH: usize = 48;
const PROMPT_SLUG_MIN_LENGTH: usize = 2;
const PROMPT_SLUG_INPUT_SAMPLE: usize = 120;
const PROMPT_SLUG_ASCII_RATIO_FLOOR: f64 = 0.5;

/// Derive an ASCII-safe slug from a user prompt for use as a plan file name.
/// Returns None when the prompt cannot produce a safe slug.
pub fn generate_prompt_plan_slug(prompt: &str) -> Option<String> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Sample first PROMPT_SLUG_INPUT_SAMPLE chars
    let sample: String = trimmed.chars().take(PROMPT_SLUG_INPUT_SAMPLE).collect();

    // 2. Strip ANSI escape sequences
    let ansi_csi_re = Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").unwrap();
    let ansi_osc_re = Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").unwrap();
    let ansi_stripped = ansi_csi_re.replace_all(&sample, "");
    let ansi_stripped = ansi_osc_re.replace_all(&ansi_stripped, "");

    // 3. Lowercase
    let lowered = ansi_stripped.to_lowercase();

    // 4. Bias check: count ASCII alnum
    let ascii_alnum_count = lowered
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .count();
    let non_whitespace_count = lowered.chars().filter(|c| !c.is_whitespace()).count();
    if non_whitespace_count == 0
        || (ascii_alnum_count as f64 / non_whitespace_count as f64) < PROMPT_SLUG_ASCII_RATIO_FLOOR
    {
        return None;
    }

    // 5. Collapse anything not in [a-z0-9] to a single dash
    let non_alnum_re = Regex::new(r"[^a-z0-9]+").unwrap();
    let collapsed = non_alnum_re.replace_all(&lowered, "-");
    let collapsed = collapsed.trim_matches('-');
    if collapsed.is_empty() {
        return None;
    }

    // 6. Truncate to PROMPT_SLUG_MAX_LENGTH
    let truncated: String = collapsed.chars().take(PROMPT_SLUG_MAX_LENGTH).collect();
    let truncated = truncated.trim_end_matches('-');
    if truncated.len() < PROMPT_SLUG_MIN_LENGTH {
        return None;
    }

    // 7. Final safety check via worktree slug validator
    if !validate_worktree_slug(truncated) {
        return None;
    }

    Some(truncated.to_string())
}

/// Validate that a slug is safe for worktree/branch/file use.
fn validate_worktree_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 64 {
        return false;
    }
    // Must only contain [a-z0-9-]
    let valid_re = Regex::new(r"^[a-z0-9-]+$").unwrap();
    if !valid_re.is_match(slug) {
        return false;
    }
    // Must not start or end with dash
    if slug.starts_with('-') || slug.ends_with('-') {
        return false;
    }
    // Must not contain consecutive dashes
    if slug.contains("--") {
        return false;
    }
    true
}

/// Find a unique slug by trying `base`, then `base-2`, `base-3`, ...
fn find_unique_slug_with_suffix<F>(base: &str, exists: F) -> Option<String>
where
    F: Fn(&str) -> bool,
{
    if !exists(base) {
        return Some(base.to_string());
    }
    for i in 2..=MAX_SLUG_RETRIES {
        let candidate = format!("{}-{}", base, i);
        if !exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Plan slug cache, mapping session IDs to slugs.
pub struct PlanSlugCache {
    cache: HashMap<String, String>,
}

impl PlanSlugCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn get(&self, session_id: &str) -> Option<&String> {
        self.cache.get(session_id)
    }

    pub fn set(&mut self, session_id: String, slug: String) {
        self.cache.insert(session_id, slug);
    }

    pub fn delete(&mut self, session_id: &str) {
        self.cache.remove(session_id);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for PlanSlugCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Get or generate a slug for the current session's plan.
pub fn get_plan_slug(
    session_id: &str,
    cache: &mut PlanSlugCache,
    plans_dir: &Path,
    first_user_prompt: Option<&str>,
) -> String {
    if let Some(slug) = cache.get(session_id) {
        return slug.clone();
    }

    let exists = |candidate: &str| -> bool { plans_dir.join(format!("{}.md", candidate)).exists() };

    let mut slug: Option<String> = None;

    // Prefer prompt-derived slug when caller passes a usable prompt
    if let Some(prompt_seed) = first_user_prompt {
        if !prompt_seed.is_empty() {
            if let Some(prompt_slug) = generate_prompt_plan_slug(prompt_seed) {
                if let Some(unique) = find_unique_slug_with_suffix(&prompt_slug, exists) {
                    slug = Some(unique);
                }
            }
        }
    }

    // Fallback: random word slug with collision retry
    if slug.is_none() {
        for _ in 0..MAX_SLUG_RETRIES {
            let candidate = generate_word_slug();
            let file_path = plans_dir.join(format!("{}.md", candidate));
            if !file_path.exists() {
                slug = Some(candidate);
                break;
            }
        }
    }

    let slug = slug.unwrap_or_else(generate_word_slug);
    cache.set(session_id.to_string(), slug.clone());
    slug
}

/// Set a specific plan slug for a session (used when resuming a session).
pub fn set_plan_slug(cache: &mut PlanSlugCache, session_id: &str, slug: &str) {
    cache.set(session_id.to_string(), slug.to_string());
}

/// Clear the plan slug for a session.
pub fn clear_plan_slug(cache: &mut PlanSlugCache, session_id: &str) {
    cache.delete(session_id);
}

/// Clear ALL plan slug entries (all sessions).
pub fn clear_all_plan_slugs(cache: &mut PlanSlugCache) {
    cache.clear();
}

/// Get the plans directory path.
pub fn get_plans_directory(
    settings_plans_dir: Option<&str>,
    cwd: &Path,
    config_home_dir: &Path,
) -> PathBuf {
    if let Some(settings_dir) = settings_plans_dir {
        let resolved = cwd.join(settings_dir);
        let cwd_str = cwd.to_string_lossy();
        let resolved_str = resolved.to_string_lossy();

        // Validate path stays within project root
        if !resolved_str.starts_with(cwd_str.as_ref()) && resolved != *cwd {
            eprintln!(
                "plansDirectory must be within project root: {}",
                settings_dir
            );
            let plans_path = config_home_dir.join("plans");
            let _ = std::fs::create_dir_all(&plans_path);
            return plans_path;
        }

        let _ = std::fs::create_dir_all(&resolved);
        resolved
    } else {
        let plans_path = config_home_dir.join("plans");
        let _ = std::fs::create_dir_all(&plans_path);
        plans_path
    }
}

/// Get the file path for a session's plan.
pub fn get_plan_file_path(plan_slug: &str, plans_dir: &Path, agent_id: Option<&str>) -> PathBuf {
    match agent_id {
        None => plans_dir.join(format!("{}.md", plan_slug)),
        Some(id) => plans_dir.join(format!("{}-agent-{}.md", plan_slug, id)),
    }
}

/// Get the plan content for a session.
pub fn get_plan(plan_file_path: &Path) -> Option<String> {
    match std::fs::read_to_string(plan_file_path) {
        Ok(content) => Some(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("Error reading plan file: {}", e);
            None
        }
    }
}

/// Extract the plan slug from a log's message history.
pub fn get_slug_from_log(messages: &[LogMessage]) -> Option<String> {
    messages.iter().find_map(|m| m.slug.clone())
}

/// A log message with optional slug.
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub slug: Option<String>,
    pub message_type: String,
    pub content: serde_json::Value,
}

/// Log option for plan operations.
#[derive(Debug, Clone)]
pub struct LogOption {
    pub messages: Vec<LogMessage>,
}

/// Copy a plan file for resume. Sets the slug in the session cache.
/// Returns true if a plan file exists (or was recovered) for the slug.
pub async fn copy_plan_for_resume(
    log: &LogOption,
    target_session_id: &str,
    cache: &mut PlanSlugCache,
    plans_dir: &Path,
) -> bool {
    let slug = match get_slug_from_log(&log.messages) {
        Some(s) => s,
        None => return false,
    };

    set_plan_slug(cache, target_session_id, &slug);

    let plan_path = plans_dir.join(format!("{}.md", slug));
    match tokio::fs::read_to_string(&plan_path).await {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Try recovery from message history
            if let Some(recovered) = recover_plan_from_messages(log) {
                match tokio::fs::write(&plan_path, &recovered).await {
                    Ok(_) => true,
                    Err(_) => false,
                }
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

/// Copy a plan file for a forked session.
pub async fn copy_plan_for_fork(
    log: &LogOption,
    target_session_id: &str,
    cache: &mut PlanSlugCache,
    plans_dir: &Path,
) -> bool {
    let original_slug = match get_slug_from_log(&log.messages) {
        Some(s) => s,
        None => return false,
    };

    let original_plan_path = plans_dir.join(format!("{}.md", original_slug));
    let new_slug = get_plan_slug(target_session_id, cache, plans_dir, None);
    let new_plan_path = plans_dir.join(format!("{}.md", new_slug));

    match tokio::fs::copy(&original_plan_path, &new_plan_path).await {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            eprintln!("Error copying plan file: {}", e);
            false
        }
    }
}

/// Recover plan content from the message history.
fn recover_plan_from_messages(log: &LogOption) -> Option<String> {
    for msg in log.messages.iter().rev() {
        if msg.message_type == "assistant" {
            if let Some(content) = msg.content.as_array() {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use")
                        && block.get("name").and_then(|v| v.as_str()) == Some("ExitPlanMode_v2")
                    {
                        if let Some(input) = block.get("input") {
                            if let Some(plan) = input.get("plan").and_then(|v| v.as_str()) {
                                if !plan.is_empty() {
                                    return Some(plan.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if msg.message_type == "user" {
            if let Some(plan_content) = msg.content.get("planContent").and_then(|v| v.as_str()) {
                if !plan_content.is_empty() {
                    return Some(plan_content.to_string());
                }
            }
        }

        if msg.message_type == "attachment" {
            if let Some(attachment) = msg.content.get("attachment") {
                if attachment.get("type").and_then(|v| v.as_str()) == Some("plan_file_reference") {
                    if let Some(plan) = attachment.get("planContent").and_then(|v| v.as_str()) {
                        if !plan.is_empty() {
                            return Some(plan.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find a file entry in the most recent file-snapshot system message.
pub fn find_file_snapshot_entry(messages: &[LogMessage], key: &str) -> Option<FileSnapshotEntry> {
    for msg in messages.iter().rev() {
        if msg.message_type == "system" {
            if let Some(subtype) = msg.content.get("subtype").and_then(|v| v.as_str()) {
                if subtype == "file_snapshot" {
                    if let Some(files) = msg.content.get("snapshotFiles").and_then(|v| v.as_array())
                    {
                        for file in files {
                            if file.get("key").and_then(|v| v.as_str()) == Some(key) {
                                return Some(FileSnapshotEntry {
                                    key: key.to_string(),
                                    path: file
                                        .get("path")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    content: file
                                        .get("content")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// A file snapshot entry.
#[derive(Debug, Clone)]
pub struct FileSnapshotEntry {
    pub key: String,
    pub path: String,
    pub content: String,
}

/// Persist a snapshot of session files to the transcript (only in remote sessions).
pub async fn persist_file_snapshot_if_remote(
    plan: Option<&str>,
    plan_file_path: Option<&Path>,
    is_remote: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !is_remote {
        return Ok(());
    }

    let mut snapshot_files = Vec::new();

    if let (Some(plan_content), Some(path)) = (plan, plan_file_path) {
        snapshot_files.push(serde_json::json!({
            "key": "plan",
            "path": path.to_string_lossy(),
            "content": plan_content,
        }));
    }

    if snapshot_files.is_empty() {
        return Ok(());
    }

    // Record the snapshot message
    // In a real implementation, this would write to the session transcript
    Ok(())
}

/// Generate a random word slug (placeholder implementation).
fn generate_word_slug() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("plan-{:08x}", seed)
}
