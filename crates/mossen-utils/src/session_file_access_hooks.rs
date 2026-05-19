//! Session file access analytics hooks.
//!
//! Tracks access to session memory and transcript files via Read, Grep, Glob tools.
//! Also tracks memdir file access via Read, Grep, Glob, Edit, and Write tools.

use std::collections::HashMap;

/// Tool name constants.
pub const FILE_READ_TOOL_NAME: &str = "Read";
pub const FILE_EDIT_TOOL_NAME: &str = "Edit";
pub const FILE_WRITE_TOOL_NAME: &str = "Write";
pub const GLOB_TOOL_NAME: &str = "Glob";
pub const GREP_TOOL_NAME: &str = "Grep";

/// Type of session file detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFileType {
    SessionMemory,
    SessionTranscript,
}

/// Hook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEventName {
    PostToolUse,
}

/// Input to a hook callback.
#[derive(Debug, Clone)]
pub struct HookInput {
    pub hook_event_name: HookEventName,
    pub tool_name: String,
    pub tool_input: ToolInput,
}

/// Parsed tool input for file access detection.
#[derive(Debug, Clone)]
pub struct ToolInput {
    /// File path for Read/Edit/Write tools.
    pub file_path: Option<String>,
    /// Path for Grep/Glob tools.
    pub path: Option<String>,
    /// Glob pattern for Glob tool.
    pub pattern: Option<String>,
    /// Glob pattern for Grep tool.
    pub glob: Option<String>,
}

/// Hook JSON output (empty object for analytics-only hooks).
#[derive(Debug, Clone, Default)]
pub struct HookJsonOutput;

/// Memory scope for a file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope {
    Project,
    User,
    Team,
}

/// Callback type for session file access tracking.
pub type HookCallback = Box<dyn Fn(&HookInput) -> HookJsonOutput + Send + Sync>;

/// Matcher definition for hook registration.
#[derive(Debug, Clone)]
pub struct HookMatcher {
    pub tool_name: String,
    pub callbacks: Vec<HookCallbackDef>,
}

/// Hook callback definition.
#[derive(Debug, Clone)]
pub struct HookCallbackDef {
    pub timeout_secs: u64,
    pub internal: bool,
}

/// Session file access hook configuration.
pub struct SessionFileAccessHooks {
    /// Functions to detect session file type.
    pub detect_session_file_type: Box<dyn Fn(&str) -> Option<SessionFileType> + Send + Sync>,
    /// Function to detect session pattern type.
    pub detect_session_pattern_type: Box<dyn Fn(&str) -> Option<SessionFileType> + Send + Sync>,
    /// Function to check if a path is an auto-mem file.
    pub is_auto_mem_file: Box<dyn Fn(&str) -> bool + Send + Sync>,
    /// Function to check if a path is a team-mem file.
    pub is_team_mem_file: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    /// Function to get memory scope for a path.
    pub memory_scope_for_path: Box<dyn Fn(&str) -> Option<MemoryScope> + Send + Sync>,
    /// Analytics event logger.
    pub log_event: Box<dyn Fn(&str, &HashMap<String, String>) + Send + Sync>,
    /// Get subagent log name.
    pub get_subagent_log_name: Box<dyn Fn() -> Option<String> + Send + Sync>,
    /// Whether team memory feature is enabled.
    pub team_mem_enabled: bool,
    /// Whether memory shape telemetry is enabled.
    pub memory_shape_telemetry_enabled: bool,
    /// Memory shape write logger.
    pub log_memory_write_shape:
        Option<Box<dyn Fn(&str, &ToolInput, &str, MemoryScope) + Send + Sync>>,
    /// Team memory write notifier.
    pub notify_team_memory_write: Option<Box<dyn Fn() + Send + Sync>>,
}

/// Extract file path from tool input for memdir detection.
fn get_file_path_from_input(tool_name: &str, tool_input: &ToolInput) -> Option<String> {
    match tool_name {
        FILE_READ_TOOL_NAME | FILE_EDIT_TOOL_NAME | FILE_WRITE_TOOL_NAME => {
            tool_input.file_path.clone()
        }
        _ => None,
    }
}

/// Extract session file type from tool input.
fn get_session_file_type_from_input(
    tool_name: &str,
    tool_input: &ToolInput,
    hooks: &SessionFileAccessHooks,
) -> Option<SessionFileType> {
    match tool_name {
        FILE_READ_TOOL_NAME => {
            let path = tool_input.file_path.as_deref()?;
            (hooks.detect_session_file_type)(path)
        }
        GREP_TOOL_NAME => {
            // Check path if provided
            if let Some(ref path) = tool_input.path {
                if let Some(file_type) = (hooks.detect_session_file_type)(path) {
                    return Some(file_type);
                }
            }
            // Check glob pattern
            if let Some(ref glob) = tool_input.glob {
                if let Some(pattern_type) = (hooks.detect_session_pattern_type)(glob) {
                    return Some(pattern_type);
                }
            }
            None
        }
        GLOB_TOOL_NAME => {
            // Check path if provided
            if let Some(ref path) = tool_input.path {
                if let Some(file_type) = (hooks.detect_session_file_type)(path) {
                    return Some(file_type);
                }
            }
            // Check pattern
            if let Some(ref pattern) = tool_input.pattern {
                if let Some(pattern_type) = (hooks.detect_session_pattern_type)(pattern) {
                    return Some(pattern_type);
                }
            }
            None
        }
        _ => None,
    }
}

/// Check if a tool use constitutes a memory file access.
pub fn is_memory_file_access(
    tool_name: &str,
    tool_input: &ToolInput,
    hooks: &SessionFileAccessHooks,
) -> bool {
    if get_session_file_type_from_input(tool_name, tool_input, hooks)
        == Some(SessionFileType::SessionMemory)
    {
        return true;
    }

    if let Some(file_path) = get_file_path_from_input(tool_name, tool_input) {
        if (hooks.is_auto_mem_file)(&file_path) {
            return true;
        }
        if hooks.team_mem_enabled {
            if let Some(ref is_team) = hooks.is_team_mem_file {
                if is_team(&file_path) {
                    return true;
                }
            }
        }
    }

    false
}

/// Handle session file access event (PostToolUse callback).
pub fn handle_session_file_access(
    input: &HookInput,
    hooks: &SessionFileAccessHooks,
) -> HookJsonOutput {
    if input.hook_event_name != HookEventName::PostToolUse {
        return HookJsonOutput;
    }

    let file_type =
        get_session_file_type_from_input(&input.tool_name, &input.tool_input, hooks);

    let subagent_name = (hooks.get_subagent_log_name)();
    let mut props = HashMap::new();
    if let Some(ref name) = subagent_name {
        props.insert("subagent_name".to_string(), name.clone());
    }

    match file_type {
        Some(SessionFileType::SessionMemory) => {
            (hooks.log_event)("tengu_session_memory_accessed", &props);
        }
        Some(SessionFileType::SessionTranscript) => {
            (hooks.log_event)("tengu_transcript_accessed", &props);
        }
        None => {}
    }

    // Memdir access tracking
    if let Some(file_path) = get_file_path_from_input(&input.tool_name, &input.tool_input) {
        if (hooks.is_auto_mem_file)(&file_path) {
            props.insert("tool".to_string(), input.tool_name.clone());
            (hooks.log_event)("tengu_memdir_accessed", &props);

            match input.tool_name.as_str() {
                FILE_READ_TOOL_NAME => {
                    (hooks.log_event)("tengu_memdir_file_read", &props);
                }
                FILE_EDIT_TOOL_NAME => {
                    (hooks.log_event)("tengu_memdir_file_edit", &props);
                }
                FILE_WRITE_TOOL_NAME => {
                    (hooks.log_event)("tengu_memdir_file_write", &props);
                }
                _ => {}
            }
        }

        // Team memory access tracking
        if hooks.team_mem_enabled {
            if let Some(ref is_team) = hooks.is_team_mem_file {
                if is_team(&file_path) {
                    props.insert("tool".to_string(), input.tool_name.clone());
                    (hooks.log_event)("tengu_team_mem_accessed", &props);

                    match input.tool_name.as_str() {
                        FILE_READ_TOOL_NAME => {
                            (hooks.log_event)("tengu_team_mem_file_read", &props);
                        }
                        FILE_EDIT_TOOL_NAME => {
                            (hooks.log_event)("tengu_team_mem_file_edit", &props);
                            if let Some(ref notify) = hooks.notify_team_memory_write {
                                notify();
                            }
                        }
                        FILE_WRITE_TOOL_NAME => {
                            (hooks.log_event)("tengu_team_mem_file_write", &props);
                            if let Some(ref notify) = hooks.notify_team_memory_write {
                                notify();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Memory shape telemetry
        if hooks.memory_shape_telemetry_enabled {
            let scope = (hooks.memory_scope_for_path)(&file_path);
            if let Some(scope) = scope {
                if input.tool_name == FILE_EDIT_TOOL_NAME
                    || input.tool_name == FILE_WRITE_TOOL_NAME
                {
                    if let Some(ref log_shape) = hooks.log_memory_write_shape {
                        log_shape(&input.tool_name, &input.tool_input, &file_path, scope);
                    }
                }
            }
        }
    }

    HookJsonOutput
}

/// Register session file access tracking hooks.
/// Returns the list of tool matchers and their associated callback definitions.
pub fn get_session_file_access_hook_registrations() -> Vec<(String, HookCallbackDef)> {
    let callback_def = HookCallbackDef {
        timeout_secs: 1,
        internal: true,
    };

    vec![
        (FILE_READ_TOOL_NAME.to_string(), callback_def.clone()),
        (GREP_TOOL_NAME.to_string(), callback_def.clone()),
        (GLOB_TOOL_NAME.to_string(), callback_def.clone()),
        (FILE_EDIT_TOOL_NAME.to_string(), callback_def.clone()),
        (FILE_WRITE_TOOL_NAME.to_string(), callback_def),
    ]
}
