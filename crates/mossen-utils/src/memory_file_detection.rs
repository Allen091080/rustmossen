//! Memory file detection utilities.
//!
//! Detects whether file paths belong to session memory, transcripts,
//! auto-memory (memdir), agent memory, or team memory directories.

use std::path::{Path, PathBuf};

/// Whether the current platform is Windows.
const IS_WINDOWS: bool = cfg!(target_os = "windows");

/// Normalize path separators to forward slashes.
fn to_posix(p: &str) -> String {
    p.replace('\\', "/")
}

/// Convert to a comparable form (forward-slash, lowercased on Windows).
fn to_comparable(p: &str) -> String {
    let posix = to_posix(p);
    if IS_WINDOWS {
        posix.to_lowercase()
    } else {
        posix
    }
}

fn normalize_comparable_dir(dir: &str) -> String {
    let trimmed = dir.trim_end_matches('/');
    if trimmed.is_empty() && dir.starts_with('/') {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn path_starts_with_dir(path: &str, dir: &str) -> bool {
    let dir = normalize_comparable_dir(dir);
    if dir.is_empty() {
        return false;
    }
    if dir == "/" {
        return path.starts_with('/');
    }
    path == dir
        || path
            .strip_prefix(&dir)
            .map(|rest| rest.starts_with('/'))
            .unwrap_or(false)
}

/// Session file types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFileType {
    SessionMemory,
    SessionTranscript,
}

/// Memory scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope {
    Personal,
    Team,
}

/// Configuration for memory file detection.
pub struct MemoryDetectionConfig {
    pub config_dir: PathBuf,
    pub memory_base_dir: PathBuf,
    pub auto_mem_path: Option<PathBuf>,
    pub auto_memory_enabled: bool,
    pub team_memory_enabled: bool,
    pub team_mem_path: Option<PathBuf>,
}

/// Detect if a file path is a session-related file under config dir.
pub fn detect_session_file_type(file_path: &str, config_dir: &str) -> Option<SessionFileType> {
    let normalized = to_comparable(file_path);
    let config_dir_cmp = to_comparable(config_dir);

    if !path_starts_with_dir(&normalized, &config_dir_cmp) {
        return None;
    }

    if normalized.contains("/session-memory/") && normalized.ends_with(".md") {
        return Some(SessionFileType::SessionMemory);
    }
    if normalized.contains("/projects/") && normalized.ends_with(".jsonl") {
        return Some(SessionFileType::SessionTranscript);
    }
    None
}

/// Check if a glob/pattern string indicates session file access intent.
pub fn detect_session_pattern_type(pattern: &str) -> Option<SessionFileType> {
    let normalized = pattern.replace('\\', "/");
    if normalized.contains("session-memory")
        && (normalized.contains(".md") || normalized.ends_with('*'))
    {
        return Some(SessionFileType::SessionMemory);
    }
    if normalized.contains(".jsonl")
        || (normalized.contains("projects") && normalized.contains("*.jsonl"))
    {
        return Some(SessionFileType::SessionTranscript);
    }
    None
}

/// Check if a file path is within the auto-memory directory.
pub fn is_auto_mem_file(file_path: &str, config: &MemoryDetectionConfig) -> bool {
    if !config.auto_memory_enabled {
        return false;
    }
    if let Some(ref auto_mem) = config.auto_mem_path {
        let normalized = to_comparable(file_path);
        let auto_mem_cmp = to_comparable(&auto_mem.to_string_lossy());
        return path_starts_with_dir(&normalized, &auto_mem_cmp);
    }
    false
}

/// Determine which memory store a path belongs to.
pub fn memory_scope_for_path(
    file_path: &str,
    config: &MemoryDetectionConfig,
) -> Option<MemoryScope> {
    if config.team_memory_enabled {
        if is_team_mem_file(file_path, config) {
            return Some(MemoryScope::Team);
        }
    }
    if is_auto_mem_file(file_path, config) {
        return Some(MemoryScope::Personal);
    }
    None
}

/// Check if a file path is within the team memory directory.
fn is_team_mem_file(file_path: &str, config: &MemoryDetectionConfig) -> bool {
    if let Some(ref team_path) = config.team_mem_path {
        let normalized = to_comparable(file_path);
        let team_cmp = to_comparable(&team_path.to_string_lossy());
        return path_starts_with_dir(&normalized, &team_cmp);
    }
    false
}

/// Check if a file path is within an agent memory directory.
fn is_agent_mem_file(file_path: &str, config: &MemoryDetectionConfig) -> bool {
    if !config.auto_memory_enabled {
        return false;
    }
    let normalized = to_comparable(file_path);
    normalized.contains("/agent-memory/") || normalized.contains("/agent-memory-local/")
}

/// Check if a file is a Mossen-managed memory file (NOT user-managed).
pub fn is_auto_managed_memory_file(file_path: &str, config: &MemoryDetectionConfig) -> bool {
    if is_auto_mem_file(file_path, config) {
        return true;
    }
    if config.team_memory_enabled && is_team_mem_file(file_path, config) {
        return true;
    }
    if detect_session_file_type(file_path, &config.config_dir.to_string_lossy()).is_some() {
        return true;
    }
    if is_agent_mem_file(file_path, config) {
        return true;
    }
    false
}

/// Check if a directory path is a memory-related directory.
pub fn is_memory_directory(dir_path: &str, config: &MemoryDetectionConfig) -> bool {
    let normalized_path = Path::new(dir_path);
    let normalized_str = normalized_path.to_string_lossy();
    let normalized_cmp = to_comparable(&normalized_str);

    // Agent memory directories
    if config.auto_memory_enabled
        && (normalized_cmp.contains("/agent-memory/")
            || normalized_cmp.contains("/agent-memory-local/"))
    {
        return true;
    }

    // Team memory directories
    if config.team_memory_enabled {
        if let Some(ref team_path) = config.team_mem_path {
            let team_cmp = to_comparable(&team_path.to_string_lossy());
            if path_starts_with_dir(&normalized_cmp, &team_cmp) {
                return true;
            }
        }
    }

    // Auto-memory path override
    if config.auto_memory_enabled {
        if let Some(ref auto_mem) = config.auto_mem_path {
            let auto_mem_str = auto_mem.to_string_lossy();
            let auto_mem_path_cmp = to_comparable(&auto_mem_str);
            if path_starts_with_dir(&normalized_cmp, &auto_mem_path_cmp) {
                return true;
            }
        }
    }

    let config_dir_cmp = to_comparable(&config.config_dir.to_string_lossy());
    let memory_base_cmp = to_comparable(&config.memory_base_dir.to_string_lossy());
    let under_config = path_starts_with_dir(&normalized_cmp, &config_dir_cmp);
    let under_memory_base = path_starts_with_dir(&normalized_cmp, &memory_base_cmp);

    if !under_config && !under_memory_base {
        return false;
    }
    if normalized_cmp.contains("/session-memory/") {
        return true;
    }
    if under_config && normalized_cmp.contains("/projects/") {
        return true;
    }
    if config.auto_memory_enabled && normalized_cmp.contains("/memory/") {
        return true;
    }
    false
}

/// Check if a shell command targets memory files.
pub fn is_shell_command_targeting_memory(command: &str, config: &MemoryDetectionConfig) -> bool {
    let config_dir = config.config_dir.to_string_lossy();
    let memory_base = config.memory_base_dir.to_string_lossy();
    let auto_mem_dir = config
        .auto_mem_path
        .as_ref()
        .map(|p| {
            p.to_string_lossy()
                .trim_end_matches(|c| c == '/' || c == '\\')
                .to_string()
        })
        .unwrap_or_default();

    let command_cmp = to_comparable(command);
    let dirs: Vec<&str> = [
        config_dir.as_ref(),
        memory_base.as_ref(),
        auto_mem_dir.as_str(),
    ]
    .into_iter()
    .filter(|d| !d.is_empty())
    .collect();

    let matches_any_dir = dirs.iter().any(|d| {
        if command_cmp.contains(&to_comparable(d)) {
            return true;
        }
        if IS_WINDOWS {
            // Check MinGW form
            let mingw = windows_path_to_posix(d);
            return command_cmp.contains(&mingw.to_lowercase());
        }
        false
    });

    if !matches_any_dir {
        return false;
    }

    // Extract absolute path-like tokens
    let re = regex::Regex::new(r#"(?:[A-Za-z]:[/\\]|/)[^\s'""]+"#).unwrap();
    let matches: Vec<&str> = re.find_iter(command).map(|m| m.as_str()).collect();

    if matches.is_empty() {
        return false;
    }

    for mat in matches {
        let clean_path = mat.trim_end_matches(|c| ",;|&>".contains(c));
        let native_path = if IS_WINDOWS {
            posix_path_to_windows(clean_path)
        } else {
            clean_path.to_string()
        };
        if is_auto_managed_memory_file(&native_path, config)
            || is_memory_directory(&native_path, config)
        {
            return true;
        }
    }

    false
}

/// Check if a pattern targets auto-managed memory files only.
pub fn is_auto_managed_memory_pattern(pattern: &str, config: &MemoryDetectionConfig) -> bool {
    if detect_session_pattern_type(pattern).is_some() {
        return true;
    }
    if config.auto_memory_enabled {
        let normalized = pattern.replace(std::path::MAIN_SEPARATOR, "/");
        if normalized.contains("agent-memory/") || normalized.contains("agent-memory-local/") {
            return true;
        }
    }
    false
}

/// Convert Windows path to POSIX form (for WSL/MinGW).
fn windows_path_to_posix(path: &str) -> String {
    let sep = char::from(b'\\');
    if path.len() >= 3 && path.as_bytes()[1] == b':' {
        let drive = (path.as_bytes()[0] as char).to_lowercase().next().unwrap();
        format!("/{}{}", drive, path[2..].replace(sep, "/"))
    } else {
        path.replace(sep, "/")
    }
}

/// Convert POSIX/MinGW path to Windows form.
fn posix_path_to_windows(path: &str) -> String {
    let sep = String::from(char::from(b'\\'));
    if path.len() >= 3 && path.starts_with('/') && path.as_bytes()[2] == b'/' {
        let drive = (path.as_bytes()[1] as char).to_uppercase().next().unwrap();
        let rest = path[2..].replace('/', sep.as_str());
        format!("{}:{}", drive, rest)
    } else {
        path.replace('/', sep.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection_config() -> MemoryDetectionConfig {
        MemoryDetectionConfig {
            config_dir: PathBuf::from("/tmp/mossen"),
            memory_base_dir: PathBuf::from("/tmp/memory-base"),
            auto_mem_path: Some(PathBuf::from("/tmp/mossen/projects/_repo/memory")),
            auto_memory_enabled: true,
            team_memory_enabled: true,
            team_mem_path: Some(PathBuf::from("/tmp/mossen/projects/_repo/memory/team")),
        }
    }

    #[test]
    fn memory_path_detection_uses_component_boundaries() {
        let config = detection_config();

        assert!(is_auto_mem_file(
            "/tmp/mossen/projects/_repo/memory/notes.md",
            &config
        ));
        assert!(!is_auto_mem_file(
            "/tmp/mossen/projects/_repo/memory-other/notes.md",
            &config
        ));
        assert_eq!(
            memory_scope_for_path("/tmp/mossen/projects/_repo/memory/team/MEMORY.md", &config),
            Some(MemoryScope::Team)
        );
        assert_eq!(
            memory_scope_for_path(
                "/tmp/mossen/projects/_repo/memory/team-other/MEMORY.md",
                &config
            ),
            Some(MemoryScope::Personal)
        );
    }

    #[test]
    fn session_config_dir_detection_uses_component_boundaries() {
        assert_eq!(
            detect_session_file_type("/tmp/mossen/session-memory/current.md", "/tmp/mossen"),
            Some(SessionFileType::SessionMemory)
        );
        assert_eq!(
            detect_session_file_type("/tmp/mossen-other/session-memory/current.md", "/tmp/mossen"),
            None
        );
    }

    #[test]
    fn memory_directory_detection_uses_component_boundaries() {
        let config = detection_config();

        assert!(is_memory_directory(
            "/tmp/mossen/projects/_repo/memory/team",
            &config
        ));
        assert!(!is_memory_directory(
            "/tmp/memory-base-other/projects",
            &config
        ));
    }
}
