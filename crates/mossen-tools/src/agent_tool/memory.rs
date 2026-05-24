//! # memory — Agent persistent memory
//!
//! Translates `tools/AgentTool/agentMemory.ts`.
//! Manages persistent agent memory directories and paths for user/project/local scopes.

use std::env;
use std::path::{Path, PathBuf, MAIN_SEPARATOR_STR};

/// Persistent agent memory scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMemoryScope {
    /// User-level memory (~/.mossen/agent-memory/)
    User,
    /// Project-level memory (.mossen/agent-memory/)
    Project,
    /// Local-only memory (.mossen/agent-memory-local/)
    Local,
}

/// Sanitize an agent type name for use as a directory name.
/// Replaces colons (invalid on Windows, used in plugin-namespaced agent
/// types like "my-plugin:my-agent") with dashes.
fn sanitize_agent_type_for_path(agent_type: &str) -> String {
    agent_type.replace(':', "-")
}

/// Sanitize a filesystem path for use in nested directory structures.
/// Replaces path separators and special characters.
fn sanitize_path(path: &str) -> String {
    path.replace(['/', '\\', ':'], "_")
}

/// Get the memory base directory (typically ~/.mossen or from env).
fn get_memory_base_dir() -> PathBuf {
    if let Ok(dir) = env::var("MOSSEN_MEMORY_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".mossen");
    }
    if let Ok(home) = env::var("USERPROFILE") {
        return PathBuf::from(home).join(".mossen");
    }
    PathBuf::from(".mossen")
}

/// Get the current working directory (with fallback).
fn get_cwd() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Get the project root directory (checks env var or uses cwd).
fn get_project_root() -> PathBuf {
    if let Ok(root) = env::var("MOSSEN_PROJECT_ROOT") {
        PathBuf::from(root)
    } else {
        get_cwd()
    }
}

/// Find the canonical git root for a given path (simplified: walks up to find .git).
fn find_canonical_git_root(start: &Path) -> Option<PathBuf> {
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

/// Returns the local agent memory directory, which is project-specific and not checked into VCS.
/// When MOSSEN_CODE_REMOTE_MEMORY_DIR is set, persists to the mount with project namespacing.
/// Otherwise, uses <cwd>/.mossen/agent-memory-local/<agentType>/.
fn get_local_agent_memory_dir(dir_name: &str) -> String {
    if let Ok(remote_dir) = env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR") {
        let project_root = get_project_root();
        let canonical =
            find_canonical_git_root(&project_root).unwrap_or_else(|| project_root.clone());
        let sanitized = sanitize_path(&canonical.to_string_lossy());
        let path = PathBuf::from(&remote_dir)
            .join("projects")
            .join(&sanitized)
            .join("agent-memory-local")
            .join(dir_name);
        format!("{}{}", path.display(), MAIN_SEPARATOR_STR)
    } else {
        let path = get_cwd()
            .join(".mossen")
            .join("agent-memory-local")
            .join(dir_name);
        format!("{}{}", path.display(), MAIN_SEPARATOR_STR)
    }
}

/// Returns the agent memory directory for a given agent type and scope.
/// - 'user' scope: <memoryBase>/agent-memory/<agentType>/
/// - 'project' scope: <cwd>/.mossen/agent-memory/<agentType>/
/// - 'local' scope: see get_local_agent_memory_dir()
pub fn get_agent_memory_dir(agent_type: &str, scope: AgentMemoryScope) -> String {
    let dir_name = sanitize_agent_type_for_path(agent_type);
    match scope {
        AgentMemoryScope::Project => {
            let path = get_cwd()
                .join(".mossen")
                .join("agent-memory")
                .join(&dir_name);
            format!("{}{}", path.display(), MAIN_SEPARATOR_STR)
        }
        AgentMemoryScope::Local => get_local_agent_memory_dir(&dir_name),
        AgentMemoryScope::User => {
            let path = get_memory_base_dir().join("agent-memory").join(&dir_name);
            format!("{}{}", path.display(), MAIN_SEPARATOR_STR)
        }
    }
}

/// Check if a file path is within an agent memory directory (any scope).
pub fn is_agent_memory_path(absolute_path: &str) -> bool {
    let normalized = Path::new(absolute_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(absolute_path));
    let normalized_str = normalized.to_string_lossy();
    let sep = MAIN_SEPARATOR_STR;

    // User scope: check memory base
    let memory_base = get_memory_base_dir();
    let user_prefix = format!("{}{}agent-memory{}", memory_base.display(), sep, sep);
    if normalized_str.starts_with(&user_prefix) {
        return true;
    }

    // Project scope: always cwd-based
    let cwd = get_cwd();
    let project_prefix = format!("{}{}.mossen{}agent-memory{}", cwd.display(), sep, sep, sep);
    if normalized_str.starts_with(&project_prefix) {
        return true;
    }

    // Local scope
    if let Ok(remote_dir) = env::var("MOSSEN_CODE_REMOTE_MEMORY_DIR") {
        let local_marker = format!("{}agent-memory-local{}", sep, sep);
        let remote_prefix = format!("{}{}projects{}", remote_dir, sep, sep);
        if normalized_str.contains(&local_marker) && normalized_str.starts_with(&remote_prefix) {
            return true;
        }
    } else {
        let local_prefix = format!(
            "{}{}.mossen{}agent-memory-local{}",
            cwd.display(),
            sep,
            sep,
            sep
        );
        if normalized_str.starts_with(&local_prefix) {
            return true;
        }
    }

    false
}

/// Returns the agent memory file path (MEMORY.md) for a given agent type and scope.
pub fn get_agent_memory_entrypoint(agent_type: &str, scope: AgentMemoryScope) -> String {
    let dir = get_agent_memory_dir(agent_type, scope);
    format!("{}MEMORY.md", dir)
}

/// Get a display string describing the memory scope.
pub fn get_memory_scope_display(scope: Option<AgentMemoryScope>) -> String {
    match scope {
        Some(AgentMemoryScope::User) => {
            let base = get_memory_base_dir();
            format!("User ({}/agent-memory/)", base.display())
        }
        Some(AgentMemoryScope::Project) => "Project (.mossen/agent-memory/)".to_string(),
        Some(AgentMemoryScope::Local) => {
            let dir = get_local_agent_memory_dir("...");
            format!("Local ({})", dir)
        }
        None => "None".to_string(),
    }
}

/// Load persistent memory prompt for an agent with memory enabled.
/// Creates the memory directory if needed and returns a prompt with memory contents.
///
/// This is async because it may read files from the memory directory.
pub async fn load_agent_memory_prompt(agent_type: &str, scope: AgentMemoryScope) -> String {
    let scope_note = match scope {
        AgentMemoryScope::User => {
            "- Since this memory is user-scope, keep learnings general since they apply across all projects"
        }
        AgentMemoryScope::Project => {
            "- Since this memory is project-scope and shared with your team via version control, tailor your memories to this project"
        }
        AgentMemoryScope::Local => {
            "- Since this memory is local-scope (not checked into version control), tailor your memories to this project and machine"
        }
    };

    let memory_dir = get_agent_memory_dir(agent_type, scope);

    // Ensure directory exists (fire-and-forget equivalent)
    let dir_path = PathBuf::from(&memory_dir);
    let _ = tokio::fs::create_dir_all(&dir_path).await;

    // Read memory files from directory
    let memory_content = read_memory_dir_contents(&dir_path).await;

    let extra_guidelines = env::var("MOSSEN_COWORK_MEMORY_EXTRA_GUIDELINES")
        .ok()
        .filter(|s| !s.trim().is_empty());

    let mut guidelines = vec![scope_note.to_string()];
    if let Some(extra) = extra_guidelines {
        guidelines.push(extra);
    }

    build_memory_prompt(
        "Persistent Agent Memory",
        &memory_dir,
        &memory_content,
        &guidelines,
    )
}

/// Read all .md files in a memory directory and concatenate their contents.
async fn read_memory_dir_contents(dir: &Path) -> String {
    let mut contents = String::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return contents,
    };

    let mut files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            files.push(path);
        }
    }
    files.sort();

    for file_path in files {
        if let Ok(content) = tokio::fs::read_to_string(&file_path).await {
            if !contents.is_empty() {
                contents.push('\n');
            }
            contents.push_str(&content);
        }
    }

    contents
}

/// Build a formatted memory prompt from directory contents.
fn build_memory_prompt(
    display_name: &str,
    memory_dir: &str,
    memory_content: &str,
    extra_guidelines: &[String],
) -> String {
    let guidelines_section = if extra_guidelines.is_empty() {
        String::new()
    } else {
        let joined = extra_guidelines.join("\n");
        format!("\n\nGuidelines:\n{}", joined)
    };

    let content_section = if memory_content.is_empty() {
        "(No memories stored yet)".to_string()
    } else {
        memory_content.to_string()
    };

    format!(
        "<{display_name}>\nMemory directory: {memory_dir}\n\n{content_section}{guidelines_section}\n</{display_name}>"
    )
}
