//! Hooks are user-defined shell commands that can be executed at various points
//! in Mossen's lifecycle.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

// ─── Constants ───────────────────────────────────────────────────────────────

const TOOL_HOOK_EXECUTION_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const SESSION_END_HOOK_TIMEOUT_MS_DEFAULT: u64 = 1500;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Hook event names that can trigger hooks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionDenied,
    PermissionRequest,
    Notification,
    SessionStart,
    SessionEnd,
    Setup,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    TeammateIdle,
    TaskCreated,
    TaskCompleted,
    UserPromptSubmit,
    PreCompact,
    PostCompact,
    ConfigChange,
    CwdChanged,
    FileChanged,
    InstructionsLoaded,
    Elicitation,
    ElicitationResult,
    WorktreeCreate,
    WorktreeRemove,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::PermissionDenied => "PermissionDenied",
            Self::PermissionRequest => "PermissionRequest",
            Self::Notification => "Notification",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::Setup => "Setup",
            Self::Stop => "Stop",
            Self::StopFailure => "StopFailure",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::TeammateIdle => "TeammateIdle",
            Self::TaskCreated => "TaskCreated",
            Self::TaskCompleted => "TaskCompleted",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreCompact => "PreCompact",
            Self::PostCompact => "PostCompact",
            Self::ConfigChange => "ConfigChange",
            Self::CwdChanged => "CwdChanged",
            Self::FileChanged => "FileChanged",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::Elicitation => "Elicitation",
            Self::ElicitationResult => "ElicitationResult",
            Self::WorktreeCreate => "WorktreeCreate",
            Self::WorktreeRemove => "WorktreeRemove",
        };
        write!(f, "{}", s)
    }
}

/// Shell type for hook execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellType {
    Bash,
    Powershell,
}

impl Default for ShellType {
    fn default() -> Self {
        Self::Bash
    }
}

/// A command-based hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub command: String,
    #[serde(default)]
    pub shell: Option<ShellType>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "async", default)]
    pub is_async: bool,
    #[serde(rename = "asyncRewake", default)]
    pub async_rewake: bool,
    #[serde(rename = "if", default)]
    pub if_condition: Option<String>,
    #[serde(rename = "statusMessage", default)]
    pub status_message: Option<String>,
}

/// A prompt-based hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPrompt {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub prompt: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "if", default)]
    pub if_condition: Option<String>,
    #[serde(rename = "statusMessage", default)]
    pub status_message: Option<String>,
}

/// An agent-based hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAgent {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub prompt: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "if", default)]
    pub if_condition: Option<String>,
    #[serde(rename = "statusMessage", default)]
    pub status_message: Option<String>,
}

/// An HTTP-based hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHttp {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub url: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "if", default)]
    pub if_condition: Option<String>,
    #[serde(rename = "statusMessage", default)]
    pub status_message: Option<String>,
}

/// Unified hook type enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Hook {
    #[serde(rename = "command")]
    Command(HookCommand),
    #[serde(rename = "prompt")]
    Prompt(HookPrompt),
    #[serde(rename = "agent")]
    Agent(HookAgent),
    #[serde(rename = "http")]
    Http(HookHttp),
    #[serde(rename = "callback")]
    Callback(HookCallbackDef),
    #[serde(rename = "function")]
    Function(FunctionHookDef),
}

/// Callback hook definition (internal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCallbackDef {
    #[serde(rename = "type")]
    pub hook_type: String,
    #[serde(default)]
    pub internal: bool,
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Function hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionHookDef {
    #[serde(rename = "type")]
    pub hook_type: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(rename = "errorMessage", default)]
    pub error_message: Option<String>,
}

/// Hook matcher from settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    #[serde(rename = "pluginRoot", default)]
    pub plugin_root: Option<String>,
    #[serde(rename = "pluginId", default)]
    pub plugin_id: Option<String>,
    #[serde(rename = "pluginName", default)]
    pub plugin_name: Option<String>,
    #[serde(rename = "skillRoot", default)]
    pub skill_root: Option<String>,
    #[serde(rename = "skillName", default)]
    pub skill_name: Option<String>,
}

/// Blocking error from a hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookBlockingError {
    pub blocking_error: String,
    pub command: String,
}

/// Elicitation response (mirrors MCP SDK's ElicitResult).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationResponse {
    pub action: String,
    #[serde(default)]
    pub content: Option<Value>,
}

/// Permission request result from hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestResult {
    pub behavior: String,
    #[serde(rename = "updatedInput", default)]
    pub updated_input: Option<Value>,
}

/// Permission behavior decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Ask,
    Deny,
    Allow,
    Passthrough,
}

/// Result of a single hook execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    #[serde(default)]
    pub message: Option<Value>,
    #[serde(default)]
    pub system_message: Option<String>,
    #[serde(default)]
    pub blocking_error: Option<HookBlockingError>,
    pub outcome: HookOutcome,
    #[serde(default)]
    pub prevent_continuation: Option<bool>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub permission_behavior: Option<PermissionBehavior>,
    #[serde(default)]
    pub hook_permission_decision_reason: Option<String>,
    #[serde(default)]
    pub additional_context: Option<String>,
    #[serde(default)]
    pub initial_user_message: Option<String>,
    #[serde(default)]
    pub updated_input: Option<Value>,
    #[serde(default)]
    pub updated_mcp_tool_output: Option<Value>,
    #[serde(default)]
    pub permission_request_result: Option<PermissionRequestResult>,
    #[serde(default)]
    pub elicitation_response: Option<ElicitationResponse>,
    #[serde(default)]
    pub watch_paths: Option<Vec<String>>,
    #[serde(default)]
    pub elicitation_result_response: Option<ElicitationResponse>,
    #[serde(default)]
    pub retry: Option<bool>,
}

/// Outcome of a hook execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    Success,
    Blocking,
    NonBlockingError,
    Cancelled,
}

/// Aggregated result from multiple hooks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedHookResult {
    #[serde(default)]
    pub message: Option<Value>,
    #[serde(default)]
    pub blocking_error: Option<HookBlockingError>,
    #[serde(default)]
    pub prevent_continuation: Option<bool>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub hook_permission_decision_reason: Option<String>,
    #[serde(default)]
    pub hook_source: Option<String>,
    #[serde(default)]
    pub permission_behavior: Option<PermissionBehavior>,
    #[serde(default)]
    pub additional_contexts: Option<Vec<String>>,
    #[serde(default)]
    pub initial_user_message: Option<String>,
    #[serde(default)]
    pub updated_input: Option<Value>,
    #[serde(default)]
    pub updated_mcp_tool_output: Option<Value>,
    #[serde(default)]
    pub permission_request_result: Option<PermissionRequestResult>,
    #[serde(default)]
    pub watch_paths: Option<Vec<String>>,
    #[serde(default)]
    pub elicitation_response: Option<ElicitationResponse>,
    #[serde(default)]
    pub elicitation_result_response: Option<ElicitationResponse>,
    #[serde(default)]
    pub retry: Option<bool>,
}

/// Hook JSON output schema (what hooks emit to stdout).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookJsonOutput {
    #[serde(rename = "continue", default)]
    pub should_continue: Option<bool>,
    #[serde(rename = "suppressOutput", default)]
    pub suppress_output: Option<bool>,
    #[serde(rename = "stopReason", default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(rename = "systemMessage", default)]
    pub system_message: Option<String>,
    #[serde(rename = "async", default)]
    pub is_async: Option<bool>,
    #[serde(rename = "asyncTimeout", default)]
    pub async_timeout: Option<u64>,
    #[serde(rename = "hookSpecificOutput", default)]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

/// Hook-specific output for different event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName", default)]
    pub hook_event_name: Option<String>,
    #[serde(rename = "permissionDecision", default)]
    pub permission_decision: Option<String>,
    #[serde(rename = "permissionDecisionReason", default)]
    pub permission_decision_reason: Option<String>,
    #[serde(rename = "updatedInput", default)]
    pub updated_input: Option<Value>,
    #[serde(rename = "additionalContext", default)]
    pub additional_context: Option<String>,
    #[serde(rename = "initialUserMessage", default)]
    pub initial_user_message: Option<String>,
    #[serde(rename = "watchPaths", default)]
    pub watch_paths: Option<Vec<String>>,
    #[serde(rename = "updatedMCPToolOutput", default)]
    pub updated_mcp_tool_output: Option<Value>,
    #[serde(default)]
    pub retry: Option<bool>,
    #[serde(default)]
    pub decision: Option<PermissionRequestResult>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(rename = "worktreePath", default)]
    pub worktree_path: Option<String>,
}

/// Hook input (common fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    pub hook_event_name: String,
    /// Additional fields are stored as a flat map.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Result from executing hooks outside the REPL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutsideReplResult {
    pub command: String,
    pub succeeded: bool,
    pub output: String,
    pub blocked: bool,
    #[serde(default)]
    pub watch_paths: Option<Vec<String>>,
    #[serde(default)]
    pub system_message: Option<String>,
}

/// A matched hook with its context.
#[derive(Debug, Clone)]
pub struct MatchedHook {
    pub hook: Value,
    pub hook_type: String,
    pub plugin_root: Option<String>,
    pub plugin_id: Option<String>,
    pub skill_root: Option<String>,
    pub hook_source: Option<String>,
}

/// Command hook execution result.
#[derive(Debug, Clone)]
pub struct CommandHookResult {
    pub stdout: String,
    pub stderr: String,
    pub output: String,
    pub status: i32,
    pub aborted: bool,
    pub backgrounded: bool,
}

/// Config change source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigChangeSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
    Skills,
}

/// Instructions load reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionsLoadReason {
    SessionStart,
    NestedTraversal,
    PathGlobMatch,
    Include,
    Compact,
}

/// Instructions memory type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionsMemoryType {
    User,
    Project,
    Local,
    Managed,
}

/// Elicitation hook result.
#[derive(Debug, Clone, Default)]
pub struct ElicitationHookResult {
    pub elicitation_response: Option<ElicitationResponse>,
    pub blocking_error: Option<HookBlockingError>,
}

/// Elicitation result hook result.
#[derive(Debug, Clone, Default)]
pub struct ElicitationResultHookResult {
    pub elicitation_result_response: Option<ElicitationResponse>,
    pub blocking_error: Option<HookBlockingError>,
}
// ─── Functions ───────────────────────────────────────────────────────────────

/// Get the session end hook timeout in milliseconds.
/// Overridable via MOSSEN_CODE_SESSIONEND_HOOKS_TIMEOUT_MS environment variable.
pub fn get_session_end_hook_timeout_ms() -> u64 {
    env::var("MOSSEN_CODE_SESSIONEND_HOOKS_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(SESSION_END_HOOK_TIMEOUT_MS_DEFAULT)
}

/// Check if a hook should be skipped due to lack of workspace trust.
///
/// ALL hooks require workspace trust because they execute arbitrary commands from
/// .mossen/settings.json. This is a defense-in-depth security measure.
pub fn should_skip_hook_due_to_trust(
    is_non_interactive: bool,
    has_trust: bool,
) -> bool {
    // In non-interactive mode (SDK), trust is implicit - always execute
    if is_non_interactive {
        return false;
    }
    // In interactive mode, ALL hooks require trust
    !has_trust
}

/// Creates the base hook input that's common to all hook types.
pub fn create_base_hook_input(
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    permission_mode: Option<&str>,
    agent_id: Option<&str>,
    agent_type: Option<&str>,
) -> HookInput {
    HookInput {
        session_id: session_id.to_string(),
        transcript_path: transcript_path.to_string(),
        cwd: cwd.to_string(),
        permission_mode: permission_mode.map(|s| s.to_string()),
        agent_id: agent_id.map(|s| s.to_string()),
        agent_type: agent_type.map(|s| s.to_string()),
        hook_event_name: String::new(),
        extra: HashMap::new(),
    }
}

/// Parse and validate a JSON string against the hook output schema.
/// Returns the validated output or formatted validation errors.
fn validate_hook_json(json_string: &str) -> Result<HookJsonOutput, String> {
    match serde_json::from_str::<HookJsonOutput>(json_string) {
        Ok(output) => {
            debug!("Successfully parsed and validated hook JSON output");
            Ok(output)
        }
        Err(e) => Err(format!(
            "Hook JSON output validation failed:\n  - {}\n\nThe hook's output was: {}",
            e, json_string
        )),
    }
}

/// Parse hook output from stdout.
fn parse_hook_output(stdout: &str) -> ParsedHookOutput {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        debug!("Hook output does not start with {{}}, treating as plain text");
        return ParsedHookOutput {
            json: None,
            plain_text: Some(stdout.to_string()),
            validation_error: None,
        };
    }

    match validate_hook_json(trimmed) {
        Ok(json) => ParsedHookOutput {
            json: Some(json),
            plain_text: None,
            validation_error: None,
        },
        Err(validation_error) => {
            let error_message = format!(
                "{}\n\nExpected schema:\n{}",
                validation_error,
                serde_json::json!({
                    "continue": "boolean (optional)",
                    "suppressOutput": "boolean (optional)",
                    "stopReason": "string (optional)",
                    "decision": "\"approve\" | \"block\" (optional)",
                    "reason": "string (optional)",
                    "systemMessage": "string (optional)",
                    "permissionDecision": "\"allow\" | \"deny\" | \"ask\" (optional)"
                })
            );
            debug!("{}", error_message);
            ParsedHookOutput {
                json: None,
                plain_text: Some(stdout.to_string()),
                validation_error: Some(error_message),
            }
        }
    }
}

/// Parsed hook output structure.
struct ParsedHookOutput {
    json: Option<HookJsonOutput>,
    plain_text: Option<String>,
    validation_error: Option<String>,
}

/// Parse HTTP hook output (must be JSON).
fn parse_http_hook_output(body: &str) -> HttpParsedOutput {
    let trimmed = body.trim();

    if trimmed.is_empty() {
        // Empty body treated as empty JSON object
        match serde_json::from_str::<HookJsonOutput>("{}") {
            Ok(json) => {
                debug!("HTTP hook returned empty body, treating as empty JSON object");
                return HttpParsedOutput {
                    json: Some(json),
                    validation_error: None,
                };
            }
            Err(_) => {}
        }
    }

    if !trimmed.starts_with('{') {
        let truncated = if trimmed.len() > 200 {
            format!("{}…", &trimmed[..200])
        } else {
            trimmed.to_string()
        };
        let err = format!(
            "HTTP hook must return JSON, but got non-JSON response body: {}",
            truncated
        );
        debug!("{}", err);
        return HttpParsedOutput {
            json: None,
            validation_error: Some(err),
        };
    }

    match validate_hook_json(trimmed) {
        Ok(json) => HttpParsedOutput {
            json: Some(json),
            validation_error: None,
        },
        Err(validation_error) => {
            debug!("{}", validation_error);
            HttpParsedOutput {
                json: None,
                validation_error: Some(validation_error),
            }
        }
    }
}

struct HttpParsedOutput {
    json: Option<HookJsonOutput>,
    validation_error: Option<String>,
}

/// Check if a hook JSON output is async.
pub fn is_async_hook_json_output(json: &HookJsonOutput) -> bool {
    json.is_async == Some(true)
}

/// Check if a hook JSON output is sync (not async).
pub fn is_sync_hook_json_output(json: &HookJsonOutput) -> bool {
    json.is_async != Some(true)
}

/// Process hook JSON output and extract result fields.
fn process_hook_json_output(
    json: &HookJsonOutput,
    command: &str,
    hook_name: &str,
    hook_event: &HookEvent,
    expected_hook_event: Option<&HookEvent>,
) -> HookResult {
    let mut result = HookResult {
        message: None,
        system_message: None,
        blocking_error: None,
        outcome: HookOutcome::Success,
        prevent_continuation: None,
        stop_reason: None,
        permission_behavior: None,
        hook_permission_decision_reason: None,
        additional_context: None,
        initial_user_message: None,
        updated_input: None,
        updated_mcp_tool_output: None,
        permission_request_result: None,
        elicitation_response: None,
        watch_paths: None,
        elicitation_result_response: None,
        retry: None,
    };

    // Handle common elements
    if json.should_continue == Some(false) {
        result.prevent_continuation = Some(true);
        if let Some(ref reason) = json.stop_reason {
            result.stop_reason = Some(reason.clone());
        }
    }

    // Handle decision field
    if let Some(ref decision) = json.decision {
        match decision.as_str() {
            "approve" => {
                result.permission_behavior = Some(PermissionBehavior::Allow);
            }
            "block" => {
                result.permission_behavior = Some(PermissionBehavior::Deny);
                result.blocking_error = Some(HookBlockingError {
                    blocking_error: json
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Blocked by hook".to_string()),
                    command: command.to_string(),
                });
            }
            other => {
                error!(
                    "Unknown hook decision type: {}. Valid types are: approve, block",
                    other
                );
            }
        }
    }

    // Handle systemMessage field
    if let Some(ref msg) = json.system_message {
        result.system_message = Some(msg.clone());
    }

    // Handle hookSpecificOutput
    if let Some(ref specific) = json.hook_specific_output {
        // Handle PreToolUse specific permission decision
        if specific.hook_event_name.as_deref() == Some("PreToolUse") {
            if let Some(ref pd) = specific.permission_decision {
                match pd.as_str() {
                    "allow" => {
                        result.permission_behavior = Some(PermissionBehavior::Allow);
                    }
                    "deny" => {
                        result.permission_behavior = Some(PermissionBehavior::Deny);
                        result.blocking_error = Some(HookBlockingError {
                            blocking_error: specific
                                .permission_decision_reason
                                .clone()
                                .or_else(|| json.reason.clone())
                                .unwrap_or_else(|| "Blocked by hook".to_string()),
                            command: command.to_string(),
                        });
                    }
                    "ask" => {
                        result.permission_behavior = Some(PermissionBehavior::Ask);
                    }
                    _ => {}
                }
            }
            result.hook_permission_decision_reason =
                specific.permission_decision_reason.clone();
            if let Some(ref ui) = specific.updated_input {
                result.updated_input = Some(ui.clone());
            }
            result.additional_context = specific.additional_context.clone();
        }

        // Validate hook event name matches expected
        if let Some(expected) = expected_hook_event {
            if let Some(ref actual_name) = specific.hook_event_name {
                if actual_name != &expected.to_string() {
                    error!(
                        "Hook returned incorrect event name: expected '{}' but got '{}'",
                        expected, actual_name
                    );
                }
            }
        }

        match specific.hook_event_name.as_deref() {
            Some("UserPromptSubmit") => {
                result.additional_context = specific.additional_context.clone();
            }
            Some("SessionStart") => {
                result.additional_context = specific.additional_context.clone();
                result.initial_user_message = specific.initial_user_message.clone();
                if let Some(ref wp) = specific.watch_paths {
                    result.watch_paths = Some(wp.clone());
                }
            }
            Some("Setup") | Some("SubagentStart") => {
                result.additional_context = specific.additional_context.clone();
            }
            Some("PostToolUse") => {
                result.additional_context = specific.additional_context.clone();
                if let Some(ref mcp_output) = specific.updated_mcp_tool_output {
                    result.updated_mcp_tool_output = Some(mcp_output.clone());
                }
            }
            Some("PostToolUseFailure") => {
                result.additional_context = specific.additional_context.clone();
            }
            Some("PermissionDenied") => {
                result.retry = specific.retry;
            }
            Some("PermissionRequest") => {
                if let Some(ref decision) = specific.decision {
                    result.permission_request_result = Some(decision.clone());
                    if decision.behavior == "allow" {
                        result.permission_behavior = Some(PermissionBehavior::Allow);
                        if let Some(ref ui) = decision.updated_input {
                            result.updated_input = Some(ui.clone());
                        }
                    } else {
                        result.permission_behavior = Some(PermissionBehavior::Deny);
                    }
                }
            }
            Some("Elicitation") => {
                if let Some(ref action) = specific.action {
                    result.elicitation_response = Some(ElicitationResponse {
                        action: action.clone(),
                        content: specific.content.clone(),
                    });
                    if action == "decline" {
                        result.blocking_error = Some(HookBlockingError {
                            blocking_error: json
                                .reason
                                .clone()
                                .unwrap_or_else(|| "Elicitation denied by hook".to_string()),
                            command: command.to_string(),
                        });
                    }
                }
            }
            Some("ElicitationResult") => {
                if let Some(ref action) = specific.action {
                    result.elicitation_result_response = Some(ElicitationResponse {
                        action: action.clone(),
                        content: specific.content.clone(),
                    });
                    if action == "decline" {
                        result.blocking_error = Some(HookBlockingError {
                            blocking_error: json
                                .reason
                                .clone()
                                .unwrap_or_else(|| {
                                    "Elicitation result blocked by hook".to_string()
                                }),
                            command: command.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Set permission decision reason from top-level
    if result.permission_behavior.is_some() && json.reason.is_some() {
        result.hook_permission_decision_reason = json.reason.clone();
    }

    result
}

/// Check if a match query matches a hook matcher pattern.
///
/// The matcher pattern can be:
/// - Simple string for exact match (e.g., 'Write')
/// - Pipe-separated list for multiple exact matches (e.g., 'Write|Edit')
/// - Regex pattern (e.g., '^Write.*', '.*', '^(Write|Edit)$')
pub fn matches_pattern(match_query: &str, matcher: &str) -> bool {
    if matcher.is_empty() || matcher == "*" {
        return true;
    }

    // Check if it's a simple string or pipe-separated list (no regex special chars except |)
    let simple_pattern = Regex::new(r"^[a-zA-Z0-9_|]+$").unwrap();
    if simple_pattern.is_match(matcher) {
        // Handle pipe-separated exact matches
        if matcher.contains('|') {
            let patterns: Vec<&str> = matcher.split('|').map(|p| p.trim()).collect();
            return patterns.contains(&match_query);
        }
        // Simple exact match
        return match_query == matcher;
    }

    // Otherwise treat as regex
    match Regex::new(matcher) {
        Ok(regex) => regex.is_match(match_query),
        Err(_) => {
            debug!("Invalid regex pattern in hook matcher: {}", matcher);
            false
        }
    }
}

/// Format a list of blocking errors from a PreTool hook.
pub fn get_pre_tool_hook_blocking_message(
    hook_name: &str,
    blocking_error: &HookBlockingError,
) -> String {
    format!("{} hook error: {}", hook_name, blocking_error.blocking_error)
}

/// Format a blocking error from a Stop hook.
pub fn get_stop_hook_message(blocking_error: &HookBlockingError) -> String {
    format!("Stop hook feedback:\n{}", blocking_error.blocking_error)
}

/// Format a blocking error from a TeammateIdle hook.
pub fn get_teammate_idle_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TeammateIdle hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error from a TaskCreated hook.
pub fn get_task_created_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TaskCreated hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error from a TaskCompleted hook.
pub fn get_task_completed_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TaskCompleted hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error from a UserPromptSubmit hook.
pub fn get_user_prompt_submit_hook_blocking_message(
    blocking_error: &HookBlockingError,
) -> String {
    format!(
        "UserPromptSubmit operation blocked by hook:\n{}",
        blocking_error.blocking_error
    )
}

/// Check if any results contain a blocking error.
pub fn has_blocking_result(results: &[HookOutsideReplResult]) -> bool {
    results.iter().any(|r| r.blocked)
}

/// Execute a command-based hook using bash or PowerShell.
pub async fn exec_command_hook(
    command: &str,
    hook_event: &str,
    hook_name: &str,
    json_input: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    timeout_ms: u64,
    cancel_token: &CancellationToken,
    shell_type: &ShellType,
) -> CommandHookResult {
    let is_windows = cfg!(target_os = "windows");

    let timeout = Duration::from_millis(timeout_ms);
    let start_time = Instant::now();

    // Build the command based on shell type
    let mut cmd = if *shell_type == ShellType::Powershell {
        let mut c = Command::new("pwsh");
        c.args(&["-NoProfile", "-NonInteractive", "-Command", command]);
        c
    } else if is_windows {
        // On Windows, use Git Bash
        let bash_path = env::var("GIT_BASH_PATH")
            .unwrap_or_else(|_| "C:\\Program Files\\Git\\bin\\bash.exe".to_string());
        let mut c = Command::new(&bash_path);
        c.args(&["-c", command]);
        c
    } else {
        let mut c = Command::new("/bin/sh");
        c.args(&["-c", command]);
        c
    };

    cmd.current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let err_msg = format!("Error occurred while executing hook command: {}", e);
            return CommandHookResult {
                stdout: String::new(),
                stderr: err_msg.clone(),
                output: err_msg,
                status: 1,
                aborted: false,
                backgrounded: false,
            };
        }
    };

    let mut child = child;

    // Write stdin
    if let Some(mut stdin) = child.stdin.take() {
        let input = format!("{}\n", json_input);
        if let Err(e) = stdin.write_all(input.as_bytes()).await {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                debug!(
                    "EPIPE error while writing to hook stdin (hook command likely closed early)"
                );
                let err_msg =
                    "Hook command closed stdin before hook input was fully written (EPIPE)";
                return CommandHookResult {
                    stdout: String::new(),
                    stderr: err_msg.to_string(),
                    output: err_msg.to_string(),
                    status: 1,
                    aborted: false,
                    backgrounded: false,
                };
            }
        }
        drop(stdin);
    }

    // Wait for completion with timeout
    let child_id = child.id();
    let wait_future = child.wait_with_output();
    let result = tokio::select! {
        result = wait_future => {
            match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let combined = format!("{}{}", stdout, stderr);
                    let status = output.status.code().unwrap_or(1);
                    CommandHookResult {
                        stdout,
                        stderr,
                        output: combined,
                        status,
                        aborted: false,
                        backgrounded: false,
                    }
                }
                Err(e) => {
                    let err_msg = format!("Error occurred while executing hook command: {}", e);
                    CommandHookResult {
                        stdout: String::new(),
                        stderr: err_msg.clone(),
                        output: err_msg,
                        status: 1,
                        aborted: false,
                        backgrounded: false,
                    }
                }
            }
        }
        _ = cancel_token.cancelled() => {
            CommandHookResult {
                stdout: String::new(),
                stderr: "Hook cancelled".to_string(),
                output: "Hook cancelled".to_string(),
                status: 1,
                aborted: true,
                backgrounded: false,
            }
        }
        _ = tokio::time::sleep(timeout) => {
            CommandHookResult {
                stdout: String::new(),
                stderr: "Hook timed out".to_string(),
                output: "Hook timed out".to_string(),
                status: 1,
                aborted: true,
                backgrounded: false,
            }
        }
    };

    let duration_ms = start_time.elapsed().as_millis();
    debug!(
        "{} [{}] completed with status {} in {}ms",
        hook_name, command, result.status, duration_ms
    );

    result
}

/// Execute hooks outside of the REPL (e.g. notifications, session end).
pub async fn execute_hooks_outside_repl(
    hook_input: &HookInput,
    matching_hooks: &[MatchedHook],
    cwd: &str,
    env_vars: &HashMap<String, String>,
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Vec<HookOutsideReplResult> {
    if matching_hooks.is_empty() {
        return Vec::new();
    }

    if cancel_token.is_cancelled() {
        return Vec::new();
    }

    let json_input = match serde_json::to_string(hook_input) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialize hook input: {}", e);
            return Vec::new();
        }
    };

    let hook_event = &hook_input.hook_event_name;

    let mut results = Vec::new();

    for (hook_index, matched) in matching_hooks.iter().enumerate() {
        if cancel_token.is_cancelled() {
            break;
        }

        // Handle command hooks
        if matched.hook_type == "command" {
            let command = matched
                .hook
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let hook_timeout = matched
                .hook
                .get("timeout")
                .and_then(|v| v.as_u64())
                .map(|t| t * 1000)
                .unwrap_or(timeout_ms);

            let shell_type = matched
                .hook
                .get("shell")
                .and_then(|v| v.as_str())
                .map(|s| {
                    if s == "powershell" {
                        ShellType::Powershell
                    } else {
                        ShellType::Bash
                    }
                })
                .unwrap_or(ShellType::Bash);

            let hook_name = format!("{}:{}", hook_event, hook_index);

            let result = exec_command_hook(
                command,
                hook_event,
                &hook_name,
                &json_input,
                cwd,
                env_vars,
                hook_timeout,
                cancel_token,
                &shell_type,
            )
            .await;

            if result.aborted {
                debug!("{} [{}] cancelled", hook_name, command);
                results.push(HookOutsideReplResult {
                    command: command.to_string(),
                    succeeded: false,
                    output: "Hook cancelled".to_string(),
                    blocked: false,
                    watch_paths: None,
                    system_message: None,
                });
                continue;
            }

            // Parse JSON output
            let parsed = parse_hook_output(&result.stdout);
            let json_blocked = parsed
                .json
                .as_ref()
                .map(|j| {
                    is_sync_hook_json_output(j) && j.decision.as_deref() == Some("block")
                })
                .unwrap_or(false);
            let blocked = result.status == 2 || json_blocked;

            let output = if result.status == 0 {
                result.stdout.clone()
            } else {
                result.stderr.clone()
            };

            let watch_paths = parsed.json.as_ref().and_then(|j| {
                j.hook_specific_output
                    .as_ref()
                    .and_then(|s| s.watch_paths.clone())
            });

            let system_message = parsed
                .json
                .as_ref()
                .and_then(|j| j.system_message.clone());

            results.push(HookOutsideReplResult {
                command: command.to_string(),
                succeeded: result.status == 0,
                output,
                blocked,
                watch_paths,
                system_message,
            });
        } else if matched.hook_type == "callback" {
            // Callback hooks return empty results when outside REPL
            results.push(HookOutsideReplResult {
                command: "callback".to_string(),
                succeeded: true,
                output: String::new(),
                blocked: false,
                watch_paths: None,
                system_message: None,
            });
        } else if matched.hook_type == "prompt" || matched.hook_type == "agent" {
            // Prompt/agent hooks not supported outside REPL
            let prompt = matched
                .hook
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            results.push(HookOutsideReplResult {
                command: prompt.to_string(),
                succeeded: false,
                output: format!(
                    "{} hooks are not yet supported outside REPL",
                    matched.hook_type
                ),
                blocked: false,
                watch_paths: None,
                system_message: None,
            });
        } else if matched.hook_type == "function" {
            error!(
                "Function hook reached execute_hooks_outside_repl for {}",
                hook_event
            );
            results.push(HookOutsideReplResult {
                command: "function".to_string(),
                succeeded: false,
                output: "Internal error: function hook executed outside REPL context"
                    .to_string(),
                blocked: false,
                watch_paths: None,
                system_message: None,
            });
        } else if matched.hook_type == "http" {
            let url = matched
                .hook
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // HTTP hooks: POST the JSON input to the URL
            match execute_http_hook(url, &json_input, timeout_ms, cancel_token).await {
                Ok(http_result) => {
                    if http_result.aborted {
                        results.push(HookOutsideReplResult {
                            command: url.to_string(),
                            succeeded: false,
                            output: "Hook cancelled".to_string(),
                            blocked: false,
                            watch_paths: None,
                            system_message: None,
                        });
                    } else if !http_result.ok {
                        results.push(HookOutsideReplResult {
                            command: url.to_string(),
                            succeeded: false,
                            output: http_result
                                .error
                                .unwrap_or_else(|| {
                                    format!("HTTP {} from {}", http_result.status_code, url)
                                }),
                            blocked: false,
                            watch_paths: None,
                            system_message: None,
                        });
                    } else {
                        let parsed = parse_http_hook_output(&http_result.body);
                        let json_blocked = parsed
                            .json
                            .as_ref()
                            .map(|j| {
                                is_sync_hook_json_output(j)
                                    && j.decision.as_deref() == Some("block")
                            })
                            .unwrap_or(false);
                        results.push(HookOutsideReplResult {
                            command: url.to_string(),
                            succeeded: true,
                            output: http_result.body,
                            blocked: json_blocked,
                            watch_paths: None,
                            system_message: None,
                        });
                    }
                }
                Err(e) => {
                    results.push(HookOutsideReplResult {
                        command: url.to_string(),
                        succeeded: false,
                        output: format!("{}", e),
                        blocked: false,
                        watch_paths: None,
                        system_message: None,
                    });
                }
            }
        }
    }

    results
}

/// HTTP hook execution result.
struct HttpHookResult {
    ok: bool,
    status_code: u16,
    body: String,
    aborted: bool,
    error: Option<String>,
}

/// Execute an HTTP hook by POSTing JSON to the URL.
async fn execute_http_hook(
    url: &str,
    json_input: &str,
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Result<HttpHookResult, String> {
    let client = reqwest::Client::new();
    let timeout = Duration::from_millis(timeout_ms);

    let request = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(json_input.to_string())
        .timeout(timeout);

    tokio::select! {
        result = request.send() => {
            match result {
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    Ok(HttpHookResult {
                        ok: status.is_success(),
                        status_code: status.as_u16(),
                        body,
                        aborted: false,
                        error: None,
                    })
                }
                Err(e) => {
                    if e.is_timeout() {
                        Ok(HttpHookResult {
                            ok: false,
                            status_code: 0,
                            body: String::new(),
                            aborted: true,
                            error: Some("Hook timed out".to_string()),
                        })
                    } else {
                        Ok(HttpHookResult {
                            ok: false,
                            status_code: 0,
                            body: String::new(),
                            aborted: false,
                            error: Some(format!("{}", e)),
                        })
                    }
                }
            }
        }
        _ = cancel_token.cancelled() => {
            Ok(HttpHookResult {
                ok: false,
                status_code: 0,
                body: String::new(),
                aborted: true,
                error: None,
            })
        }
    }
}

/// Get matching hooks for a given event and input.
pub fn get_matching_hooks(
    hook_matchers: &[HookMatcher],
    hook_event: &HookEvent,
    hook_input: &HookInput,
) -> Vec<MatchedHook> {
    // Determine match query based on hook event
    let match_query: Option<&str> = match hook_event {
        HookEvent::PreToolUse
        | HookEvent::PostToolUse
        | HookEvent::PostToolUseFailure
        | HookEvent::PermissionRequest
        | HookEvent::PermissionDenied => {
            hook_input.extra.get("tool_name").and_then(|v| v.as_str())
        }
        HookEvent::SessionStart => hook_input.extra.get("source").and_then(|v| v.as_str()),
        HookEvent::Setup | HookEvent::PreCompact | HookEvent::PostCompact => {
            hook_input.extra.get("trigger").and_then(|v| v.as_str())
        }
        HookEvent::Notification => hook_input
            .extra
            .get("notification_type")
            .and_then(|v| v.as_str()),
        HookEvent::SessionEnd => hook_input.extra.get("reason").and_then(|v| v.as_str()),
        HookEvent::StopFailure => hook_input.extra.get("error").and_then(|v| v.as_str()),
        HookEvent::SubagentStart | HookEvent::SubagentStop => {
            hook_input.extra.get("agent_type").and_then(|v| v.as_str())
        }
        HookEvent::Elicitation | HookEvent::ElicitationResult => hook_input
            .extra
            .get("mcp_server_name")
            .and_then(|v| v.as_str()),
        HookEvent::ConfigChange => hook_input.extra.get("source").and_then(|v| v.as_str()),
        HookEvent::InstructionsLoaded => hook_input
            .extra
            .get("load_reason")
            .and_then(|v| v.as_str()),
        HookEvent::FileChanged => hook_input
            .extra
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
            }),
        _ => None,
    };

    debug!(
        "Getting matching hook commands for {} with query: {:?}",
        hook_event, match_query
    );
    debug!(
        "Found {} hook matchers in settings",
        hook_matchers.len()
    );

    // Filter matchers based on match query
    let filtered_matchers: Vec<&HookMatcher> = if let Some(query) = match_query {
        hook_matchers
            .iter()
            .filter(|m| {
                m.matcher
                    .as_ref()
                    .map(|pattern| matches_pattern(query, pattern))
                    .unwrap_or(true)
            })
            .collect()
    } else {
        hook_matchers.iter().collect()
    };

    // Extract hooks with their plugin context
    let mut matched_hooks: Vec<MatchedHook> = Vec::new();
    for matcher in &filtered_matchers {
        let hook_source = if matcher.plugin_root.is_some() {
            matcher
                .plugin_name
                .as_ref()
                .map(|n| format!("plugin:{}", n))
                .or(Some("plugin".to_string()))
        } else if matcher.skill_root.is_some() {
            matcher
                .skill_name
                .as_ref()
                .map(|n| format!("skill:{}", n))
                .or(Some("skill".to_string()))
        } else {
            Some("settings".to_string())
        };

        for hook_value in &matcher.hooks {
            let hook_type = hook_value
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("command")
                .to_string();

            matched_hooks.push(MatchedHook {
                hook: hook_value.clone(),
                hook_type,
                plugin_root: matcher.plugin_root.clone(),
                plugin_id: matcher.plugin_id.clone(),
                skill_root: matcher.skill_root.clone(),
                hook_source: hook_source.clone(),
            });
        }
    }

    // Deduplicate hooks by command within the same source context
    // Fast-path: callback/function hooks don't need dedup
    if matched_hooks
        .iter()
        .all(|m| m.hook_type == "callback" || m.hook_type == "function")
    {
        return matched_hooks;
    }

    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut unique_hooks: Vec<MatchedHook> = Vec::new();

    for matched in matched_hooks {
        let prefix = matched
            .plugin_root
            .as_deref()
            .or(matched.skill_root.as_deref())
            .unwrap_or("");

        let dedup_payload = match matched.hook_type.as_str() {
            "command" => {
                let cmd = matched
                    .hook
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let shell = matched
                    .hook
                    .get("shell")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bash");
                let if_cond = matched
                    .hook
                    .get("if")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("{}\0{}\0{}", shell, cmd, if_cond)
            }
            "prompt" | "agent" => {
                let prompt = matched
                    .hook
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let if_cond = matched
                    .hook
                    .get("if")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("{}\0{}", prompt, if_cond)
            }
            "http" => {
                let url = matched
                    .hook
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let if_cond = matched
                    .hook
                    .get("if")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("{}\0{}", url, if_cond)
            }
            // callback/function hooks are always unique
            _ => {
                unique_hooks.push(matched);
                continue;
            }
        };

        let key = format!("{}\0{}", prefix, dedup_payload);
        if seen_keys.insert(key) {
            unique_hooks.push(matched);
        }
    }

    // Filter hooks based on `if` condition
    let filtered_hooks: Vec<MatchedHook> = unique_hooks
        .into_iter()
        .filter(|h| {
            let if_condition = h.hook.get("if").and_then(|v| v.as_str());
            if let Some(_cond) = if_condition {
                // For tool-related events, if condition filtering would be applied
                // For non-tool events, skip hooks with if conditions
                match hook_event {
                    HookEvent::PreToolUse
                    | HookEvent::PostToolUse
                    | HookEvent::PostToolUseFailure
                    | HookEvent::PermissionRequest => true, // condition evaluation deferred
                    _ => {
                        debug!(
                            "Hook if condition cannot be evaluated for non-tool event {}",
                            hook_event
                        );
                        false
                    }
                }
            } else {
                true
            }
        })
        .collect();

    // HTTP hooks are not supported for SessionStart/Setup events
    let final_hooks = if *hook_event == HookEvent::SessionStart
        || *hook_event == HookEvent::Setup
    {
        filtered_hooks
            .into_iter()
            .filter(|h| {
                if h.hook_type == "http" {
                    let url = h.hook.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    debug!(
                        "Skipping HTTP hook {} — HTTP hooks are not supported for {}",
                        url, hook_event
                    );
                    false
                } else {
                    true
                }
            })
            .collect()
    } else {
        filtered_hooks
    };

    debug!(
        "Matched {} unique hooks for query {:?}",
        final_hooks.len(),
        match_query
    );

    final_hooks
}

/// Execute notification hooks if configured.
pub async fn execute_notification_hooks(
    message: &str,
    title: Option<&str>,
    notification_type: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Vec<HookOutsideReplResult> {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "Notification".to_string();
    hook_input
        .extra
        .insert("message".to_string(), Value::String(message.to_string()));
    if let Some(t) = title {
        hook_input
            .extra
            .insert("title".to_string(), Value::String(t.to_string()));
    }
    hook_input.extra.insert(
        "notification_type".to_string(),
        Value::String(notification_type.to_string()),
    );

    let matching = get_matching_hooks(hook_matchers, &HookEvent::Notification, &hook_input);
    execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await
}

/// Execute session end hooks if configured.
pub async fn execute_session_end_hooks(
    reason: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Vec<HookOutsideReplResult> {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "SessionEnd".to_string();
    hook_input
        .extra
        .insert("reason".to_string(), Value::String(reason.to_string()));

    let matching = get_matching_hooks(hook_matchers, &HookEvent::SessionEnd, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    // During shutdown, write errors to stderr
    for result in &results {
        if !result.succeeded && !result.output.is_empty() {
            eprintln!(
                "SessionEnd hook [{}] failed: {}",
                result.command, result.output
            );
        }
    }

    results
}

/// Execute config change hooks when configuration files change.
pub async fn execute_config_change_hooks(
    source: &ConfigChangeSource,
    file_path: Option<&str>,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Vec<HookOutsideReplResult> {
    let source_str = serde_json::to_string(source)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();

    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "ConfigChange".to_string();
    hook_input
        .extra
        .insert("source".to_string(), Value::String(source_str.clone()));
    if let Some(fp) = file_path {
        hook_input
            .extra
            .insert("file_path".to_string(), Value::String(fp.to_string()));
    }

    let matching = get_matching_hooks(hook_matchers, &HookEvent::ConfigChange, &hook_input);
    let mut results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    // Policy settings are enterprise-managed — hooks fire for audit logging
    // but must never block policy changes
    if *source == ConfigChangeSource::PolicySettings {
        for r in &mut results {
            r.blocked = false;
        }
    }

    results
}

/// Execute CwdChanged hooks.
pub async fn execute_cwd_changed_hooks(
    old_cwd: &str,
    new_cwd: &str,
    session_id: &str,
    transcript_path: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> (Vec<HookOutsideReplResult>, Vec<String>, Vec<String>) {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        new_cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "CwdChanged".to_string();
    hook_input
        .extra
        .insert("old_cwd".to_string(), Value::String(old_cwd.to_string()));
    hook_input
        .extra
        .insert("new_cwd".to_string(), Value::String(new_cwd.to_string()));

    let matching = get_matching_hooks(hook_matchers, &HookEvent::CwdChanged, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        new_cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    let watch_paths: Vec<String> = results
        .iter()
        .flat_map(|r| r.watch_paths.clone().unwrap_or_default())
        .collect();
    let system_messages: Vec<String> = results
        .iter()
        .filter_map(|r| r.system_message.clone())
        .collect();

    (results, watch_paths, system_messages)
}

/// Execute FileChanged hooks.
pub async fn execute_file_changed_hooks(
    file_path: &str,
    event: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> (Vec<HookOutsideReplResult>, Vec<String>, Vec<String>) {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "FileChanged".to_string();
    hook_input
        .extra
        .insert("file_path".to_string(), Value::String(file_path.to_string()));
    hook_input
        .extra
        .insert("event".to_string(), Value::String(event.to_string()));

    let matching = get_matching_hooks(hook_matchers, &HookEvent::FileChanged, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    let watch_paths: Vec<String> = results
        .iter()
        .flat_map(|r| r.watch_paths.clone().unwrap_or_default())
        .collect();
    let system_messages: Vec<String> = results
        .iter()
        .filter_map(|r| r.system_message.clone())
        .collect();

    (results, watch_paths, system_messages)
}

/// Execute InstructionsLoaded hooks.
pub async fn execute_instructions_loaded_hooks(
    file_path: &str,
    memory_type: &InstructionsMemoryType,
    load_reason: &InstructionsLoadReason,
    globs: Option<&[String]>,
    trigger_file_path: Option<&str>,
    parent_file_path: Option<&str>,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "InstructionsLoaded".to_string();
    hook_input
        .extra
        .insert("file_path".to_string(), Value::String(file_path.to_string()));
    hook_input.extra.insert(
        "memory_type".to_string(),
        serde_json::to_value(memory_type).unwrap_or(Value::Null),
    );
    hook_input.extra.insert(
        "load_reason".to_string(),
        serde_json::to_value(load_reason).unwrap_or(Value::Null),
    );
    if let Some(g) = globs {
        hook_input.extra.insert(
            "globs".to_string(),
            serde_json::to_value(g).unwrap_or(Value::Null),
        );
    }
    if let Some(tfp) = trigger_file_path {
        hook_input.extra.insert(
            "trigger_file_path".to_string(),
            Value::String(tfp.to_string()),
        );
    }
    if let Some(pfp) = parent_file_path {
        hook_input.extra.insert(
            "parent_file_path".to_string(),
            Value::String(pfp.to_string()),
        );
    }

    let load_reason_str = serde_json::to_string(load_reason)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let matching = get_matching_hooks(
        hook_matchers,
        &HookEvent::InstructionsLoaded,
        &hook_input,
    );
    let _ = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;
}

/// Execute elicitation hooks.
pub async fn execute_elicitation_hooks(
    server_name: &str,
    message: &str,
    requested_schema: Option<&Value>,
    permission_mode: Option<&str>,
    mode: Option<&str>,
    url: Option<&str>,
    elicitation_id: Option<&str>,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> ElicitationHookResult {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        permission_mode,
        None,
        None,
    );
    hook_input.hook_event_name = "Elicitation".to_string();
    hook_input.extra.insert(
        "mcp_server_name".to_string(),
        Value::String(server_name.to_string()),
    );
    hook_input
        .extra
        .insert("message".to_string(), Value::String(message.to_string()));
    if let Some(schema) = requested_schema {
        hook_input
            .extra
            .insert("requested_schema".to_string(), schema.clone());
    }
    if let Some(m) = mode {
        hook_input
            .extra
            .insert("mode".to_string(), Value::String(m.to_string()));
    }
    if let Some(u) = url {
        hook_input
            .extra
            .insert("url".to_string(), Value::String(u.to_string()));
    }
    if let Some(eid) = elicitation_id {
        hook_input
            .extra
            .insert("elicitation_id".to_string(), Value::String(eid.to_string()));
    }

    let matching = get_matching_hooks(hook_matchers, &HookEvent::Elicitation, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    let mut elicitation_response = None;
    let mut blocking_error = None;

    for result in &results {
        let parsed = parse_elicitation_hook_output(result, "Elicitation");
        if let Some(err) = parsed.blocking_error {
            blocking_error = Some(err);
        }
        if let Some(resp) = parsed.response {
            elicitation_response = Some(resp);
        }
    }

    ElicitationHookResult {
        elicitation_response,
        blocking_error,
    }
}

/// Execute elicitation result hooks.
pub async fn execute_elicitation_result_hooks(
    server_name: &str,
    action: &str,
    content: Option<&Value>,
    permission_mode: Option<&str>,
    mode: Option<&str>,
    elicitation_id: Option<&str>,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> ElicitationResultHookResult {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        permission_mode,
        None,
        None,
    );
    hook_input.hook_event_name = "ElicitationResult".to_string();
    hook_input.extra.insert(
        "mcp_server_name".to_string(),
        Value::String(server_name.to_string()),
    );
    hook_input
        .extra
        .insert("action".to_string(), Value::String(action.to_string()));
    if let Some(c) = content {
        hook_input.extra.insert("content".to_string(), c.clone());
    }
    if let Some(m) = mode {
        hook_input
            .extra
            .insert("mode".to_string(), Value::String(m.to_string()));
    }
    if let Some(eid) = elicitation_id {
        hook_input
            .extra
            .insert("elicitation_id".to_string(), Value::String(eid.to_string()));
    }

    let matching =
        get_matching_hooks(hook_matchers, &HookEvent::ElicitationResult, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    let mut elicitation_result_response = None;
    let mut blocking_error = None;

    for result in &results {
        let parsed = parse_elicitation_hook_output(result, "ElicitationResult");
        if let Some(err) = parsed.blocking_error {
            blocking_error = Some(err);
        }
        if let Some(resp) = parsed.response {
            elicitation_result_response = Some(resp);
        }
    }

    ElicitationResultHookResult {
        elicitation_result_response,
        blocking_error,
    }
}

/// Parse elicitation-specific fields from a HookOutsideReplResult.
fn parse_elicitation_hook_output(
    result: &HookOutsideReplResult,
    expected_event_name: &str,
) -> ParsedElicitationOutput {
    // Exit code 2 = blocking
    if result.blocked && !result.succeeded {
        return ParsedElicitationOutput {
            response: None,
            blocking_error: Some(HookBlockingError {
                blocking_error: if result.output.is_empty() {
                    "Elicitation blocked by hook".to_string()
                } else {
                    result.output.clone()
                },
                command: result.command.clone(),
            }),
        };
    }

    let trimmed = result.output.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return ParsedElicitationOutput {
            response: None,
            blocking_error: None,
        };
    }

    // Try to parse JSON
    let parsed: Result<HookJsonOutput, _> = serde_json::from_str(trimmed);
    match parsed {
        Ok(json) => {
            if is_async_hook_json_output(&json) || !is_sync_hook_json_output(&json) {
                return ParsedElicitationOutput {
                    response: None,
                    blocking_error: None,
                };
            }

            // Check for top-level decision: 'block'
            if json.decision.as_deref() == Some("block") || result.blocked {
                return ParsedElicitationOutput {
                    response: None,
                    blocking_error: Some(HookBlockingError {
                        blocking_error: json
                            .reason
                            .unwrap_or_else(|| "Elicitation blocked by hook".to_string()),
                        command: result.command.clone(),
                    }),
                };
            }

            let specific = match json.hook_specific_output {
                Some(ref s) if s.hook_event_name.as_deref() == Some(expected_event_name) => s,
                _ => {
                    return ParsedElicitationOutput {
                        response: None,
                        blocking_error: None,
                    }
                }
            };

            let action = match &specific.action {
                Some(a) => a.clone(),
                None => {
                    return ParsedElicitationOutput {
                        response: None,
                        blocking_error: None,
                    }
                }
            };

            let response = ElicitationResponse {
                action: action.clone(),
                content: specific.content.clone(),
            };

            let blocking_error = if action == "decline" {
                Some(HookBlockingError {
                    blocking_error: json.reason.unwrap_or_else(|| {
                        if expected_event_name == "Elicitation" {
                            "Elicitation denied by hook".to_string()
                        } else {
                            "Elicitation result blocked by hook".to_string()
                        }
                    }),
                    command: result.command.clone(),
                })
            } else {
                None
            };

            ParsedElicitationOutput {
                response: Some(response),
                blocking_error,
            }
        }
        Err(_) => ParsedElicitationOutput {
            response: None,
            blocking_error: None,
        },
    }
}

struct ParsedElicitationOutput {
    response: Option<ElicitationResponse>,
    blocking_error: Option<HookBlockingError>,
}

/// Execute WorktreeCreate hooks.
/// Returns the worktree path from hook stdout.
pub async fn execute_worktree_create_hook(
    name: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Result<String, String> {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "WorktreeCreate".to_string();
    hook_input
        .extra
        .insert("name".to_string(), Value::String(name.to_string()));

    let matching = get_matching_hooks(hook_matchers, &HookEvent::WorktreeCreate, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    // Find the first successful result with non-empty output
    let successful_result = results
        .iter()
        .find(|r| r.succeeded && !r.output.trim().is_empty());

    match successful_result {
        Some(result) => Ok(result.output.trim().to_string()),
        None => {
            let failed_outputs: Vec<String> = results
                .iter()
                .filter(|r| !r.succeeded)
                .map(|r| {
                    let output = r.output.trim();
                    if output.is_empty() {
                        format!("{}: no output", r.command)
                    } else {
                        format!("{}: {}", r.command, output)
                    }
                })
                .collect();
            Err(format!(
                "WorktreeCreate hook failed: {}",
                if failed_outputs.is_empty() {
                    "no successful output".to_string()
                } else {
                    failed_outputs.join("; ")
                }
            ))
        }
    }
}

/// Execute WorktreeRemove hooks.
/// Returns true if hooks were configured and ran.
pub async fn execute_worktree_remove_hook(
    worktree_path: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> bool {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "WorktreeRemove".to_string();
    hook_input.extra.insert(
        "worktree_path".to_string(),
        Value::String(worktree_path.to_string()),
    );

    let matching =
        get_matching_hooks(hook_matchers, &HookEvent::WorktreeRemove, &hook_input);
    if matching.is_empty() {
        return false;
    }

    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    if results.is_empty() {
        return false;
    }

    for result in &results {
        if !result.succeeded {
            debug!(
                "WorktreeRemove hook failed [{}]: {}",
                result.command,
                result.output.trim()
            );
        }
    }

    true
}

/// Check if InstructionsLoaded hooks are configured.
pub fn has_instructions_loaded_hook(hook_matchers: &[HookMatcher]) -> bool {
    !hook_matchers.is_empty()
}

/// Check if WorktreeCreate hooks are configured.
pub fn has_worktree_create_hook(hook_matchers: &[HookMatcher]) -> bool {
    !hook_matchers.is_empty()
}

/// Execute pre-compact hooks.
pub async fn execute_pre_compact_hooks(
    trigger: &str,
    custom_instructions: Option<&str>,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> (Option<String>, Option<String>) {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "PreCompact".to_string();
    hook_input
        .extra
        .insert("trigger".to_string(), Value::String(trigger.to_string()));
    if let Some(ci) = custom_instructions {
        hook_input.extra.insert(
            "custom_instructions".to_string(),
            Value::String(ci.to_string()),
        );
    }

    let matching = get_matching_hooks(hook_matchers, &HookEvent::PreCompact, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    if results.is_empty() {
        return (None, None);
    }

    let successful_outputs: Vec<String> = results
        .iter()
        .filter(|r| r.succeeded && !r.output.trim().is_empty())
        .map(|r| r.output.trim().to_string())
        .collect();

    let mut display_messages: Vec<String> = Vec::new();
    for result in &results {
        let output = result.output.trim();
        if result.succeeded {
            if output.is_empty() {
                display_messages.push(format!(
                    "PreCompact [{}] completed successfully",
                    result.command
                ));
            } else {
                display_messages.push(format!(
                    "PreCompact [{}] completed successfully: {}",
                    result.command, output
                ));
            }
        } else if output.is_empty() {
            display_messages.push(format!("PreCompact [{}] failed", result.command));
        } else {
            display_messages.push(format!(
                "PreCompact [{}] failed: {}",
                result.command, output
            ));
        }
    }

    let new_custom_instructions = if successful_outputs.is_empty() {
        None
    } else {
        Some(successful_outputs.join("\n\n"))
    };

    let user_display_message = if display_messages.is_empty() {
        None
    } else {
        Some(display_messages.join("\n"))
    };

    (new_custom_instructions, user_display_message)
}

/// Execute post-compact hooks.
pub async fn execute_post_compact_hooks(
    trigger: &str,
    compact_summary: &str,
    session_id: &str,
    transcript_path: &str,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    hook_matchers: &[HookMatcher],
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Option<String> {
    let mut hook_input = create_base_hook_input(
        session_id,
        transcript_path,
        cwd,
        None,
        None,
        None,
    );
    hook_input.hook_event_name = "PostCompact".to_string();
    hook_input
        .extra
        .insert("trigger".to_string(), Value::String(trigger.to_string()));
    hook_input.extra.insert(
        "compact_summary".to_string(),
        Value::String(compact_summary.to_string()),
    );

    let matching = get_matching_hooks(hook_matchers, &HookEvent::PostCompact, &hook_input);
    let results = execute_hooks_outside_repl(
        &hook_input,
        &matching,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
    )
    .await;

    if results.is_empty() {
        return None;
    }

    let mut display_messages: Vec<String> = Vec::new();
    for result in &results {
        let output = result.output.trim();
        if result.succeeded {
            if output.is_empty() {
                display_messages.push(format!(
                    "PostCompact [{}] completed successfully",
                    result.command
                ));
            } else {
                display_messages.push(format!(
                    "PostCompact [{}] completed successfully: {}",
                    result.command, output
                ));
            }
        } else if output.is_empty() {
            display_messages.push(format!("PostCompact [{}] failed", result.command));
        } else {
            display_messages.push(format!(
                "PostCompact [{}] failed: {}",
                result.command, output
            ));
        }
    }

    if display_messages.is_empty() {
        None
    } else {
        Some(display_messages.join("\n"))
    }
}

/// Execute status line command if configured.
pub async fn execute_status_line_command(
    status_line_command: &str,
    status_input: &Value,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Option<String> {
    let json_input = serde_json::to_string(status_input).ok()?;

    let result = exec_command_hook(
        status_line_command,
        "StatusLine",
        "statusLine",
        &json_input,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
        &ShellType::Bash,
    )
    .await;

    if result.aborted || result.status != 0 {
        return None;
    }

    let output: String = result
        .stdout
        .trim()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<&str>>()
        .join("\n");

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

/// Execute file suggestion command if configured.
pub async fn execute_file_suggestion_command(
    command: &str,
    suggestion_input: &Value,
    cwd: &str,
    env_vars: &HashMap<String, String>,
    timeout_ms: u64,
    cancel_token: &CancellationToken,
) -> Vec<String> {
    let json_input = match serde_json::to_string(suggestion_input) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let result = exec_command_hook(
        command,
        "FileSuggestion",
        "FileSuggestion",
        &json_input,
        cwd,
        env_vars,
        timeout_ms,
        cancel_token,
        &ShellType::Bash,
    )
    .await;

    if result.aborted || result.status != 0 {
        return Vec::new();
    }

    result
        .stdout
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}
