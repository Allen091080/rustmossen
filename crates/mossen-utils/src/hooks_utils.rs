//! # hooks_utils — Hook 执行与管理工具库
//!
//! 对应 TypeScript `utils/hooks.ts`。
//! 提供 hook 匹配、执行、输出解析、结果聚合等功能。
//! React hooks 模式转为普通函数 + struct 方法 + 依赖注入。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing;
use uuid::Uuid;

use mossen_types::hooks::{HookEvent, HookResult as HookResultType};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 10 minutes — default timeout for tool-hook execution.
pub const TOOL_HOOK_EXECUTION_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// 1.5 seconds — default timeout for SessionEnd hooks.
const SESSION_END_HOOK_TIMEOUT_MS_DEFAULT: u64 = 1500;

/// Default hook shell type.
const DEFAULT_HOOK_SHELL: &str = "bash";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Blocking error from a hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookBlockingError {
    pub blocking_error: String,
    pub command: String,
}

/// Elicitation response (re-export compatible with MCP SDK ElicitResult).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationResponse {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

/// Permission request result from hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestResult {
    pub behavior: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<Value>,
}

/// Result of a single hook execution.
#[derive(Debug, Clone)]
pub struct HookResult {
    pub message: Option<Value>,
    pub system_message: Option<String>,
    pub blocking_error: Option<HookBlockingError>,
    pub outcome: HookOutcome,
    pub prevent_continuation: Option<bool>,
    pub stop_reason: Option<String>,
    pub permission_behavior: Option<String>,
    pub hook_permission_decision_reason: Option<String>,
    pub additional_context: Option<String>,
    pub initial_user_message: Option<String>,
    pub updated_input: Option<Value>,
    pub updated_mcp_tool_output: Option<Value>,
    pub permission_request_result: Option<PermissionRequestResult>,
    pub elicitation_response: Option<ElicitationResponse>,
    pub watch_paths: Option<Vec<String>>,
    pub elicitation_result_response: Option<ElicitationResponse>,
    pub retry: Option<bool>,
    pub hook: MatchedHookInfo,
}

/// Hook execution outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookOutcome {
    Success,
    Blocking,
    NonBlockingError,
    Cancelled,
}

/// Aggregated result from hook execution (yielded by executeHooks).
#[derive(Debug, Clone, Default)]
pub struct AggregatedHookResult {
    pub message: Option<Value>,
    pub blocking_error: Option<HookBlockingError>,
    pub prevent_continuation: Option<bool>,
    pub stop_reason: Option<String>,
    pub hook_permission_decision_reason: Option<String>,
    pub hook_source: Option<String>,
    pub permission_behavior: Option<String>,
    pub additional_contexts: Option<Vec<String>>,
    pub initial_user_message: Option<String>,
    pub updated_input: Option<Value>,
    pub updated_mcp_tool_output: Option<Value>,
    pub permission_request_result: Option<PermissionRequestResult>,
    pub watch_paths: Option<Vec<String>>,
    pub elicitation_response: Option<ElicitationResponse>,
    pub elicitation_result_response: Option<ElicitationResponse>,
    pub retry: Option<bool>,
}

/// Result for hooks executed outside REPL.
#[derive(Debug, Clone)]
pub struct HookOutsideReplResult {
    pub command: String,
    pub succeeded: bool,
    pub output: String,
    pub blocked: bool,
    pub watch_paths: Option<Vec<String>>,
    pub system_message: Option<String>,
}

/// Hook command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    #[serde(rename = "asyncRewake", skip_serializing_if = "Option::is_none")]
    pub async_rewake: Option<bool>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_condition: Option<String>,
}

/// Hook callback (SDK native hooks).
#[derive(Clone)]
pub struct HookCallback {
    pub hook_type: String,
    pub internal: bool,
    pub timeout: Option<u64>,
    pub callback: Arc<
        dyn Fn(Value, String, Option<tokio_util::sync::CancellationToken>, usize, Option<Value>)
            -> futures::future::BoxFuture<'static, Value>
            + Send
            + Sync,
    >,
}

impl std::fmt::Debug for HookCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookCallback")
            .field("hook_type", &self.hook_type)
            .field("internal", &self.internal)
            .field("timeout", &self.timeout)
            .field("callback", &"<callback>")
            .finish()
    }
}

/// Function hook (session-based).
#[derive(Clone)]
pub struct FunctionHook {
    pub hook_type: String,
    pub error_message: String,
    pub timeout: Option<u64>,
    pub callback: Arc<
        dyn Fn(Vec<Value>, Option<tokio_util::sync::CancellationToken>)
            -> futures::future::BoxFuture<'static, bool>
            + Send
            + Sync,
    >,
}

impl std::fmt::Debug for FunctionHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionHook")
            .field("hook_type", &self.hook_type)
            .field("error_message", &self.error_message)
            .field("timeout", &self.timeout)
            .field("callback", &"<callback>")
            .finish()
    }
}

/// Prompt hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptHook {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_condition: Option<String>,
}

/// Agent hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHook {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_condition: Option<String>,
}

/// HTTP hook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHook {
    #[serde(rename = "type")]
    pub hook_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_condition: Option<String>,
}

/// A single hook (union of all types).
#[derive(Debug, Clone)]
pub enum Hook {
    Command(HookCommand),
    Callback(HookCallback),
    Function(FunctionHook),
    Prompt(PromptHook),
    Agent(AgentHook),
    Http(HttpHook),
}

impl Hook {
    pub fn hook_type(&self) -> &str {
        match self {
            Hook::Command(_) => "command",
            Hook::Callback(_) => "callback",
            Hook::Function(_) => "function",
            Hook::Prompt(_) => "prompt",
            Hook::Agent(_) => "agent",
            Hook::Http(_) => "http",
        }
    }

    pub fn timeout(&self) -> Option<u64> {
        match self {
            Hook::Command(h) => h.timeout,
            Hook::Callback(h) => h.timeout,
            Hook::Function(h) => h.timeout,
            Hook::Prompt(h) => h.timeout,
            Hook::Agent(h) => h.timeout,
            Hook::Http(h) => h.timeout,
        }
    }

    pub fn if_condition(&self) -> Option<&str> {
        match self {
            Hook::Command(h) => h.if_condition.as_deref(),
            Hook::Prompt(h) => h.if_condition.as_deref(),
            Hook::Agent(h) => h.if_condition.as_deref(),
            Hook::Http(h) => h.if_condition.as_deref(),
            _ => None,
        }
    }
}

/// Info about a matched hook (for results).
#[derive(Debug, Clone)]
pub struct MatchedHookInfo {
    pub hook_type: String,
    pub display_text: String,
}

/// A hook paired with optional plugin context.
#[derive(Debug, Clone)]
pub struct MatchedHook {
    pub hook: Hook,
    pub plugin_root: Option<String>,
    pub plugin_id: Option<String>,
    pub skill_root: Option<String>,
    pub hook_source: Option<String>,
}

/// Hook matcher from configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_name: Option<String>,
}

/// Sync JSON output from hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHookJsonOutput {
    #[serde(rename = "continue", skip_serializing_if = "Option::is_none")]
    pub should_continue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<Value>,
}

/// Async JSON output from hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncHookJsonOutput {
    #[serde(rename = "async")]
    pub is_async: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async_timeout: Option<u64>,
}

/// Base hook input common to all hook types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseHookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

/// Config change source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigChangeSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
    Skills,
}

/// Instructions load reason type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionsLoadReason {
    SessionStart,
    NestedTraversal,
    PathGlobMatch,
    Include,
    Compact,
}

/// Instructions memory type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Command hook execution result.
#[derive(Debug, Clone)]
pub struct CommandHookExecResult {
    pub stdout: String,
    pub stderr: String,
    pub output: String,
    pub status: i32,
    pub aborted: bool,
    pub backgrounded: bool,
}

// ---------------------------------------------------------------------------
// Context (dependency injection replacing TS module-level globals)
// ---------------------------------------------------------------------------

/// Context for hook execution, injecting all external dependencies.
pub struct HooksContext {
    pub session_id: String,
    pub original_cwd: String,
    pub project_root: String,
    pub is_non_interactive: bool,
    pub trust_accepted: bool,
    pub hooks_config_snapshot: Option<HashMap<String, Vec<HookMatcher>>>,
    pub registered_hooks: Option<HashMap<String, Vec<HookMatcher>>>,
    pub disable_all_hooks: bool,
    pub managed_hooks_only: bool,
    pub main_thread_agent_type: Option<String>,
    pub custom_backend_enabled: bool,
    pub simple_mode: bool,
    /// Callback to get transcript path for a session ID.
    pub get_transcript_path: Arc<dyn Fn(&str) -> String + Send + Sync>,
    /// Callback to get agent transcript path.
    pub get_agent_transcript_path: Arc<dyn Fn(&str) -> String + Send + Sync>,
    /// Callback to log debug messages.
    pub log_debug: Arc<dyn Fn(&str) + Send + Sync>,
    /// Callback to log errors.
    pub log_error: Arc<dyn Fn(&str) + Send + Sync>,
    /// Callback for analytics event logging.
    pub log_event: Arc<dyn Fn(&str, &Value) + Send + Sync>,
    /// Callback to get settings.
    pub get_settings: Arc<dyn Fn() -> Option<Value> + Send + Sync>,
    /// Callback to get settings for source.
    pub get_settings_for_source: Arc<dyn Fn(&str) -> Option<Value> + Send + Sync>,
    /// Callback to invalidate session env cache.
    pub invalidate_session_env_cache: Arc<dyn Fn() + Send + Sync>,
    /// Subprocess environment variables.
    pub subprocess_env: HashMap<String, String>,
    /// Known official marketplace plugin names.
    pub allowed_official_marketplace_names: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Get session end hook timeout from env or default.
pub fn get_session_end_hook_timeout_ms() -> u64 {
    std::env::var("MOSSEN_CODE_SESSIONEND_HOOKS_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(SESSION_END_HOOK_TIMEOUT_MS_DEFAULT)
}

/// Check if a hook should be skipped due to lack of workspace trust.
pub fn should_skip_hook_due_to_trust(ctx: &HooksContext) -> bool {
    if ctx.is_non_interactive {
        return false;
    }
    !ctx.trust_accepted
}

/// Create the base hook input common to all hook types.
pub fn create_base_hook_input(
    ctx: &HooksContext,
    permission_mode: Option<&str>,
    session_id: Option<&str>,
    agent_info: Option<(&str, &str)>,
) -> BaseHookInput {
    let resolved_session_id = session_id
        .unwrap_or(&ctx.session_id)
        .to_string();
    let resolved_agent_type = agent_info
        .map(|(_, at)| at.to_string())
        .or_else(|| ctx.main_thread_agent_type.clone());
    let transcript_path = (ctx.get_transcript_path)(&resolved_session_id);

    BaseHookInput {
        session_id: resolved_session_id,
        transcript_path,
        cwd: ctx.original_cwd.clone(),
        permission_mode: permission_mode.map(|s| s.to_string()),
        agent_id: agent_info.map(|(id, _)| id.to_string()),
        agent_type: resolved_agent_type,
    }
}

/// Check if results contain a blocking result.
pub fn has_blocking_result(results: &[HookOutsideReplResult]) -> bool {
    results.iter().any(|r| r.blocked)
}

/// Format a blocking error message for PreToolUse hooks.
pub fn get_pre_tool_hook_blocking_message(
    hook_name: &str,
    blocking_error: &HookBlockingError,
) -> String {
    format!("{} hook error: {}", hook_name, blocking_error.blocking_error)
}

/// Format a blocking error message for Stop hooks.
pub fn get_stop_hook_message(blocking_error: &HookBlockingError) -> String {
    format!("Stop hook feedback:\n{}", blocking_error.blocking_error)
}

/// Format a blocking error message for TeammateIdle hooks.
pub fn get_teammate_idle_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TeammateIdle hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error message for TaskCreated hooks.
pub fn get_task_created_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TaskCreated hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error message for TaskCompleted hooks.
pub fn get_task_completed_hook_message(blocking_error: &HookBlockingError) -> String {
    format!(
        "TaskCompleted hook feedback:\n{}",
        blocking_error.blocking_error
    )
}

/// Format a blocking error message for UserPromptSubmit hooks.
pub fn get_user_prompt_submit_hook_blocking_message(
    blocking_error: &HookBlockingError,
) -> String {
    format!(
        "UserPromptSubmit operation blocked by hook:\n{}",
        blocking_error.blocking_error
    )
}

/// Get the display text for a hook.
pub fn get_hook_display_text(hook: &Hook) -> String {
    match hook {
        Hook::Command(h) => h.command.clone(),
        Hook::Callback(_) => "callback".to_string(),
        Hook::Function(_) => "function".to_string(),
        Hook::Prompt(h) => format!("prompt: {}", first_line_of(&h.prompt)),
        Hook::Agent(h) => format!("agent: {}", first_line_of(&h.prompt)),
        Hook::Http(h) => h.url.clone(),
    }
}

/// Get the first line of a string.
fn first_line_of(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// Normalize a legacy tool name (identity for now).
fn normalize_legacy_tool_name(name: &str) -> String {
    name.to_string()
}

/// Get legacy tool names for a given name (returns empty for now).
fn get_legacy_tool_names(_name: &str) -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Hook JSON validation and parsing
// ---------------------------------------------------------------------------

/// Check if a parsed JSON value is an async hook output.
pub fn is_async_hook_json_output(val: &Value) -> bool {
    val.get("async")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Check if a parsed JSON value is a sync hook output.
pub fn is_sync_hook_json_output(val: &Value) -> bool {
    !is_async_hook_json_output(val)
}

/// Parse and validate a JSON string against hook output schema.
/// Returns validated output or validation error message.
fn validate_hook_json(json_string: &str) -> std::result::Result<Value, String> {
    let parsed: Value = serde_json::from_str(json_string)
        .map_err(|e| format!("Failed to parse hook JSON: {}", e))?;

    // Basic validation: must be an object
    if !parsed.is_object() {
        return Err(format!(
            "Hook JSON output must be an object, got: {}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        ));
    }

    // Validate known fields
    if let Some(obj) = parsed.as_object() {
        // Validate 'continue' field if present
        if let Some(cont) = obj.get("continue") {
            if !cont.is_boolean() {
                return Err("'continue' field must be a boolean".to_string());
            }
        }
        // Validate 'decision' field if present
        if let Some(decision) = obj.get("decision") {
            if let Some(s) = decision.as_str() {
                if s != "approve" && s != "block" {
                    return Err(format!(
                        "Unknown decision type: {}. Valid types are: approve, block",
                        s
                    ));
                }
            }
        }
        // Validate 'hookSpecificOutput' if present
        if let Some(specific) = obj.get("hookSpecificOutput") {
            if !specific.is_object() {
                return Err("'hookSpecificOutput' must be an object".to_string());
            }
        }
    }

    Ok(parsed)
}

/// Parse hook output from stdout.
pub fn parse_hook_output(stdout: &str) -> ParsedHookOutput {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
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
        Err(error) => {
            let error_message = format!(
                "{}\n\nExpected schema:\n{}",
                error,
                serde_json::to_string_pretty(&serde_json::json!({
                    "continue": "boolean (optional)",
                    "suppressOutput": "boolean (optional)",
                    "stopReason": "string (optional)",
                    "decision": "\"approve\" | \"block\" (optional)",
                    "reason": "string (optional)",
                    "systemMessage": "string (optional)",
                    "permissionDecision": "\"allow\" | \"deny\" | \"ask\" (optional)",
                }))
                .unwrap_or_default()
            );
            ParsedHookOutput {
                json: None,
                plain_text: Some(stdout.to_string()),
                validation_error: Some(error_message),
            }
        }
    }
}

/// Parsed hook output container.
#[derive(Debug, Clone)]
pub struct ParsedHookOutput {
    pub json: Option<Value>,
    pub plain_text: Option<String>,
    pub validation_error: Option<String>,
}

/// Parse HTTP hook output.
pub fn parse_http_hook_output(body: &str) -> ParsedHttpHookOutput {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        // Empty body treated as empty JSON object
        return ParsedHttpHookOutput {
            json: Some(serde_json::json!({})),
            validation_error: None,
        };
    }

    if !trimmed.starts_with('{') {
        let error = format!(
            "HTTP hook must return JSON, but got non-JSON response body: {}",
            if trimmed.len() > 200 {
                format!("{}…", &trimmed[..200])
            } else {
                trimmed.to_string()
            }
        );
        return ParsedHttpHookOutput {
            json: None,
            validation_error: Some(error),
        };
    }

    match validate_hook_json(trimmed) {
        Ok(json) => ParsedHttpHookOutput {
            json: Some(json),
            validation_error: None,
        },
        Err(error) => ParsedHttpHookOutput {
            json: None,
            validation_error: Some(error),
        },
    }
}

/// Parsed HTTP hook output container.
#[derive(Debug, Clone)]
pub struct ParsedHttpHookOutput {
    pub json: Option<Value>,
    pub validation_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Process hook JSON output
// ---------------------------------------------------------------------------

/// Process validated sync hook JSON output into partial HookResult fields.
pub fn process_hook_json_output(
    json: &Value,
    command: &str,
    hook_name: &str,
    _tool_use_id: &str,
    hook_event: &str,
    expected_hook_event: Option<&str>,
    _stdout: Option<&str>,
    _stderr: Option<&str>,
    _exit_code: Option<i32>,
    _duration_ms: Option<u64>,
) -> PartialHookResult {
    let mut result = PartialHookResult::default();

    // Handle common elements
    if json.get("continue").and_then(|v| v.as_bool()) == Some(false) {
        result.prevent_continuation = Some(true);
        if let Some(reason) = json.get("stopReason").and_then(|v| v.as_str()) {
            result.stop_reason = Some(reason.to_string());
        }
    }

    // Handle decision
    if let Some(decision) = json.get("decision").and_then(|v| v.as_str()) {
        match decision {
            "approve" => {
                result.permission_behavior = Some("allow".to_string());
            }
            "block" => {
                result.permission_behavior = Some("deny".to_string());
                result.blocking_error = Some(HookBlockingError {
                    blocking_error: json
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Blocked by hook")
                        .to_string(),
                    command: command.to_string(),
                });
            }
            _ => {
                // Unknown decision type
            }
        }
    }

    // Handle systemMessage
    if let Some(msg) = json.get("systemMessage").and_then(|v| v.as_str()) {
        result.system_message = Some(msg.to_string());
    }

    // Handle hookSpecificOutput
    if let Some(specific) = json.get("hookSpecificOutput") {
        // Validate event name
        if let Some(expected) = expected_hook_event {
            if let Some(actual) = specific.get("hookEventName").and_then(|v| v.as_str()) {
                if actual != expected {
                    // Event name mismatch — treated as error
                    return result;
                }
            }
        }

        let event_name = specific
            .get("hookEventName")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match event_name {
            "PreToolUse" => {
                // Handle permission decision
                if let Some(pd) = specific.get("permissionDecision").and_then(|v| v.as_str()) {
                    match pd {
                        "allow" => result.permission_behavior = Some("allow".to_string()),
                        "deny" => {
                            result.permission_behavior = Some("deny".to_string());
                            result.blocking_error = Some(HookBlockingError {
                                blocking_error: specific
                                    .get("permissionDecisionReason")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| json.get("reason").and_then(|v| v.as_str()))
                                    .unwrap_or("Blocked by hook")
                                    .to_string(),
                                command: command.to_string(),
                            });
                        }
                        "ask" => result.permission_behavior = Some("ask".to_string()),
                        _ => {}
                    }
                }
                result.hook_permission_decision_reason = specific
                    .get("permissionDecisionReason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(updated) = specific.get("updatedInput") {
                    result.updated_input = Some(updated.clone());
                }
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            "UserPromptSubmit" => {
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            "SessionStart" => {
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.initial_user_message = specific
                    .get("initialUserMessage")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(wp) = specific.get("watchPaths").and_then(|v| v.as_array()) {
                    result.watch_paths = Some(
                        wp.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect(),
                    );
                }
            }
            "Setup" | "SubagentStart" => {
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            "PostToolUse" => {
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(updated) = specific.get("updatedMCPToolOutput") {
                    result.updated_mcp_tool_output = Some(updated.clone());
                }
            }
            "PostToolUseFailure" => {
                result.additional_context = specific
                    .get("additionalContext")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            "PermissionDenied" => {
                result.retry = specific.get("retry").and_then(|v| v.as_bool());
            }
            "PermissionRequest" => {
                if let Some(decision) = specific.get("decision") {
                    let behavior = decision
                        .get("behavior")
                        .and_then(|v| v.as_str())
                        .unwrap_or("deny");
                    result.permission_request_result = Some(PermissionRequestResult {
                        behavior: behavior.to_string(),
                        updated_input: decision.get("updatedInput").cloned(),
                    });
                    result.permission_behavior = Some(
                        if behavior == "allow" { "allow" } else { "deny" }.to_string(),
                    );
                    if behavior == "allow" {
                        if let Some(ui) = decision.get("updatedInput") {
                            result.updated_input = Some(ui.clone());
                        }
                    }
                }
            }
            "Elicitation" => {
                if let Some(action) = specific.get("action").and_then(|v| v.as_str()) {
                    result.elicitation_response = Some(ElicitationResponse {
                        action: action.to_string(),
                        content: specific.get("content").cloned(),
                    });
                    if action == "decline" {
                        result.blocking_error = Some(HookBlockingError {
                            blocking_error: json
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Elicitation denied by hook")
                                .to_string(),
                            command: command.to_string(),
                        });
                    }
                }
            }
            "ElicitationResult" => {
                if let Some(action) = specific.get("action").and_then(|v| v.as_str()) {
                    result.elicitation_result_response = Some(ElicitationResponse {
                        action: action.to_string(),
                        content: specific.get("content").cloned(),
                    });
                    if action == "decline" {
                        result.blocking_error = Some(HookBlockingError {
                            blocking_error: json
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Elicitation result blocked by hook")
                                .to_string(),
                            command: command.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Handle permission reason
    if result.permission_behavior.is_some() {
        if let Some(reason) = json.get("reason").and_then(|v| v.as_str()) {
            if result.hook_permission_decision_reason.is_none() {
                result.hook_permission_decision_reason = Some(reason.to_string());
            }
        }
    }

    // Generate result message based on blocking error or success
    if result.blocking_error.is_some() {
        result.message = Some(create_attachment_message_value(
            "hook_blocking_error",
            hook_name,
            hook_event,
            &result.blocking_error,
        ));
    } else {
        result.message = Some(create_attachment_message_value(
            "hook_success",
            hook_name,
            hook_event,
            &None,
        ));
    }

    result
}

/// Partial hook result (used during JSON output processing).
#[derive(Debug, Clone, Default)]
pub struct PartialHookResult {
    pub message: Option<Value>,
    pub system_message: Option<String>,
    pub blocking_error: Option<HookBlockingError>,
    pub prevent_continuation: Option<bool>,
    pub stop_reason: Option<String>,
    pub permission_behavior: Option<String>,
    pub hook_permission_decision_reason: Option<String>,
    pub additional_context: Option<String>,
    pub initial_user_message: Option<String>,
    pub updated_input: Option<Value>,
    pub updated_mcp_tool_output: Option<Value>,
    pub permission_request_result: Option<PermissionRequestResult>,
    pub elicitation_response: Option<ElicitationResponse>,
    pub elicitation_result_response: Option<ElicitationResponse>,
    pub watch_paths: Option<Vec<String>>,
    pub retry: Option<bool>,
}

/// Create an attachment message value.
fn create_attachment_message_value(
    msg_type: &str,
    hook_name: &str,
    hook_event: &str,
    blocking_error: &Option<HookBlockingError>,
) -> Value {
    let mut msg = serde_json::json!({
        "type": "attachment",
        "attachment": {
            "type": msg_type,
            "hookName": hook_name,
            "hookEvent": hook_event,
        }
    });
    if let Some(err) = blocking_error {
        msg["attachment"]["blockingError"] = serde_json::json!({
            "blockingError": err.blocking_error,
            "command": err.command,
        });
    }
    msg
}

// ---------------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------------

/// Check if a match query matches a hook matcher pattern.
fn matches_pattern(match_query: &str, matcher: &str) -> bool {
    if matcher.is_empty() || matcher == "*" {
        return true;
    }

    // Check if it's a simple string or pipe-separated list
    let is_simple = matcher
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '|');

    if is_simple {
        if matcher.contains('|') {
            let patterns: Vec<String> = matcher
                .split('|')
                .map(|p| normalize_legacy_tool_name(p.trim()))
                .collect();
            return patterns.contains(&match_query.to_string());
        }
        return match_query == normalize_legacy_tool_name(matcher);
    }

    // Treat as regex
    match Regex::new(matcher) {
        Ok(re) => {
            if re.is_match(match_query) {
                return true;
            }
            // Also test against legacy names
            for legacy_name in get_legacy_tool_names(match_query) {
                if re.is_match(&legacy_name) {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Check if a matched hook is internal (callback with internal=true).
fn is_internal_hook(matched: &MatchedHook) -> bool {
    if let Hook::Callback(cb) = &matched.hook {
        cb.internal
    } else {
        false
    }
}

/// Build a dedup key for a matched hook.
fn hook_dedup_key(m: &MatchedHook, payload: &str) -> String {
    let prefix = m
        .plugin_root
        .as_deref()
        .or(m.skill_root.as_deref())
        .unwrap_or("");
    format!("{}\0{}", prefix, payload)
}

/// Build a map of {sanitized plugin name: hook count}.
fn get_plugin_hook_counts(
    hooks: &[MatchedHook],
    allowed_names: &HashSet<String>,
) -> Option<HashMap<String, usize>> {
    let plugin_hooks: Vec<&MatchedHook> = hooks.iter().filter(|h| h.plugin_id.is_some()).collect();
    if plugin_hooks.is_empty() {
        return None;
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for h in plugin_hooks {
        let plugin_id = h.plugin_id.as_ref().unwrap();
        let at_index = plugin_id.rfind('@');
        let is_official = at_index.map_or(false, |idx| {
            idx > 0 && allowed_names.contains(&plugin_id[idx + 1..])
        });
        let key = if is_official {
            plugin_id.clone()
        } else {
            "third-party".to_string()
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    Some(counts)
}

/// Build a map of {hook type: count}.
fn get_hook_type_counts(hooks: &[MatchedHook]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for h in hooks {
        let t = h.hook.hook_type().to_string();
        *counts.entry(t).or_insert(0) += 1;
    }
    counts
}

// ---------------------------------------------------------------------------
// Hook matching
// ---------------------------------------------------------------------------

/// Get hooks configuration by merging snapshot, registered, and session hooks.
fn get_hooks_config(
    ctx: &HooksContext,
    hook_event: &str,
) -> Vec<HookMatcher> {
    let mut hooks: Vec<HookMatcher> = Vec::new();

    // From snapshot
    if let Some(snapshot) = &ctx.hooks_config_snapshot {
        if let Some(event_hooks) = snapshot.get(hook_event) {
            hooks.extend(event_hooks.iter().cloned());
        }
    }

    // From registered hooks
    if !ctx.managed_hooks_only {
        if let Some(registered) = &ctx.registered_hooks {
            if let Some(event_hooks) = registered.get(hook_event) {
                for matcher in event_hooks {
                    if ctx.managed_hooks_only && matcher.plugin_root.is_some() {
                        continue;
                    }
                    hooks.push(matcher.clone());
                }
            }
        }
    }

    hooks
}

/// Lightweight check if any hooks exist for a given event.
fn has_hook_for_event(ctx: &HooksContext, hook_event: &str) -> bool {
    if let Some(snapshot) = &ctx.hooks_config_snapshot {
        if let Some(hooks) = snapshot.get(hook_event) {
            if !hooks.is_empty() {
                return true;
            }
        }
    }
    if let Some(registered) = &ctx.registered_hooks {
        if let Some(hooks) = registered.get(hook_event) {
            if !hooks.is_empty() {
                return true;
            }
        }
    }
    false
}

/// Get hook commands that match the given query.
pub async fn get_matching_hooks(
    ctx: &HooksContext,
    hook_event: &str,
    hook_input: &Value,
    match_query: Option<&str>,
) -> Vec<MatchedHook> {
    let hook_matchers = get_hooks_config(ctx, hook_event);
    if hook_matchers.is_empty() {
        return Vec::new();
    }

    // Filter by matcher pattern
    let filtered_matchers = if let Some(query) = match_query {
        hook_matchers
            .into_iter()
            .filter(|m| {
                m.matcher
                    .as_deref()
                    .map_or(true, |pattern| matches_pattern(query, pattern))
            })
            .collect::<Vec<_>>()
    } else {
        hook_matchers
    };

    // Extract hooks with plugin context
    let mut matched_hooks: Vec<MatchedHook> = Vec::new();
    for matcher in &filtered_matchers {
        let plugin_root = matcher.plugin_root.clone();
        let plugin_id = matcher.plugin_id.clone();
        let skill_root = matcher.skill_root.clone();
        let hook_source = if plugin_root.is_some() {
            matcher
                .plugin_name
                .as_ref()
                .map(|n| format!("plugin:{}", n))
                .or(Some("plugin".to_string()))
        } else if skill_root.is_some() {
            matcher
                .skill_name
                .as_ref()
                .map(|n| format!("skill:{}", n))
                .or(Some("skill".to_string()))
        } else {
            Some("settings".to_string())
        };

        for hook_val in &matcher.hooks {
            let hook_type = hook_val
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("command");

            let hook = match hook_type {
                "command" => {
                    if let Ok(cmd) = serde_json::from_value::<HookCommand>(hook_val.clone()) {
                        Hook::Command(cmd)
                    } else {
                        continue;
                    }
                }
                "prompt" => {
                    if let Ok(p) = serde_json::from_value::<PromptHook>(hook_val.clone()) {
                        Hook::Prompt(p)
                    } else {
                        continue;
                    }
                }
                "agent" => {
                    if let Ok(a) = serde_json::from_value::<AgentHook>(hook_val.clone()) {
                        Hook::Agent(a)
                    } else {
                        continue;
                    }
                }
                "http" => {
                    if let Ok(h) = serde_json::from_value::<HttpHook>(hook_val.clone()) {
                        Hook::Http(h)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            matched_hooks.push(MatchedHook {
                hook,
                plugin_root: plugin_root.clone(),
                plugin_id: plugin_id.clone(),
                skill_root: skill_root.clone(),
                hook_source: hook_source.clone(),
            });
        }
    }

    // Check if all hooks are callback/function — skip dedup
    if matched_hooks.iter().all(|m| {
        matches!(m.hook, Hook::Callback(_) | Hook::Function(_))
    }) {
        return matched_hooks;
    }

    // Deduplicate command hooks
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut unique_hooks: Vec<MatchedHook> = Vec::new();

    for m in matched_hooks {
        let key = match &m.hook {
            Hook::Command(h) => {
                let shell = h.shell.as_deref().unwrap_or(DEFAULT_HOOK_SHELL);
                let if_cond = h.if_condition.as_deref().unwrap_or("");
                Some(hook_dedup_key(
                    &m,
                    &format!("{}\0{}\0{}", shell, h.command, if_cond),
                ))
            }
            Hook::Prompt(h) => {
                let if_cond = h.if_condition.as_deref().unwrap_or("");
                Some(hook_dedup_key(&m, &format!("{}\0{}", h.prompt, if_cond)))
            }
            Hook::Agent(h) => {
                let if_cond = h.if_condition.as_deref().unwrap_or("");
                Some(hook_dedup_key(&m, &format!("{}\0{}", h.prompt, if_cond)))
            }
            Hook::Http(h) => {
                let if_cond = h.if_condition.as_deref().unwrap_or("");
                Some(hook_dedup_key(&m, &format!("{}\0{}", h.url, if_cond)))
            }
            Hook::Callback(_) | Hook::Function(_) => None,
        };

        if let Some(k) = key {
            if seen_keys.insert(k) {
                unique_hooks.push(m);
            }
        } else {
            unique_hooks.push(m);
        }
    }

    // Filter by `if` condition
    let has_if_condition = unique_hooks.iter().any(|h| h.hook.if_condition().is_some());
    if !has_if_condition {
        // Skip HTTP hooks for SessionStart/Setup
        if hook_event == "SessionStart" || hook_event == "Setup" {
            unique_hooks.retain(|h| !matches!(h.hook, Hook::Http(_)));
        }
        return unique_hooks;
    }

    // For hooks with if conditions, evaluate them
    let filtered: Vec<MatchedHook> = unique_hooks
        .into_iter()
        .filter(|h| {
            if let Some(if_cond) = h.hook.if_condition() {
                // Simplified if-condition evaluation
                // In full implementation, this would use tool lookup + permission matcher
                !if_cond.is_empty()
            } else {
                true
            }
        })
        .collect();

    // Skip HTTP hooks for SessionStart/Setup
    if hook_event == "SessionStart" || hook_event == "Setup" {
        return filtered
            .into_iter()
            .filter(|h| !matches!(h.hook, Hook::Http(_)))
            .collect();
    }

    filtered
}

// ---------------------------------------------------------------------------
// Hook execution — command hooks
// ---------------------------------------------------------------------------

/// Execute a command-based hook using shell.
pub async fn exec_command_hook(
    hook: &HookCommand,
    hook_event: &str,
    hook_name: &str,
    json_input: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ctx: &HooksContext,
    plugin_root: Option<&str>,
    plugin_id: Option<&str>,
    skill_root: Option<&str>,
) -> Result<CommandHookExecResult> {
    let hook_timeout_ms = hook
        .timeout
        .map(|t| t * 1000)
        .unwrap_or(TOOL_HOOK_EXECUTION_TIMEOUT_MS);

    let mut command_str = hook.command.clone();

    // Substitute plugin variables
    if let Some(pr) = plugin_root {
        command_str = command_str.replace("${MOSSEN_PLUGIN_ROOT}", pr);
        if let Some(pid) = plugin_id {
            let data_dir = format!("{}/data", pr); // Simplified plugin data dir
            command_str = command_str.replace("${MOSSEN_PLUGIN_DATA}", &data_dir);
        }
    }

    // Build environment variables
    let mut env_vars: HashMap<String, String> = ctx.subprocess_env.clone();
    env_vars.insert(
        "MOSSEN_PROJECT_DIR".to_string(),
        ctx.project_root.clone(),
    );
    if let Some(pr) = plugin_root {
        env_vars.insert("MOSSEN_PLUGIN_ROOT".to_string(), pr.to_string());
    }
    if let Some(sr) = skill_root {
        env_vars.insert("MOSSEN_PLUGIN_ROOT".to_string(), sr.to_string());
    }

    // Determine safe cwd
    let safe_cwd = if tokio::fs::metadata(&ctx.original_cwd).await.is_ok() {
        &ctx.original_cwd
    } else {
        &ctx.project_root
    };

    // Spawn the command
    let shell_type = hook.shell.as_deref().unwrap_or(DEFAULT_HOOK_SHELL);
    let mut child = if shell_type == "powershell" {
        let mut cmd = Command::new("pwsh");
        cmd.args(&["-NoProfile", "-NonInteractive", "-Command", &command_str]);
        cmd.envs(&env_vars);
        cmd.current_dir(safe_cwd);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.spawn()?
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(&["-c", &command_str]);
        cmd.envs(&env_vars);
        cmd.current_dir(safe_cwd);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.spawn()?
    };

    // Write stdin
    if let Some(stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let mut stdin = stdin;
        let input = format!("{}\n", json_input);
        let _ = stdin.write_all(input.as_bytes()).await;
        drop(stdin);
    }

    // Wait for completion with timeout
    let timeout = Duration::from_millis(hook_timeout_ms);
    let result = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let status = output.status.code().unwrap_or(1);
            CommandHookExecResult {
                output: format!("{}{}", stdout, stderr),
                stdout,
                stderr,
                status,
                aborted: false,
                backgrounded: false,
            }
        }
        Ok(Err(e)) => {
            let err_msg = format!("Error executing hook command: {}", e);
            CommandHookExecResult {
                stdout: String::new(),
                stderr: err_msg.clone(),
                output: err_msg,
                status: 1,
                aborted: false,
                backgrounded: false,
            }
        }
        Err(_) => {
            CommandHookExecResult {
                stdout: String::new(),
                stderr: "Hook cancelled (timeout)".to_string(),
                output: "Hook cancelled (timeout)".to_string(),
                status: 1,
                aborted: true,
                backgrounded: false,
            }
        }
    };

    // Check cancellation token
    if let Some(token) = cancel_token {
        if token.is_cancelled() {
            return Ok(CommandHookExecResult {
                stdout: String::new(),
                stderr: "Hook cancelled".to_string(),
                output: "Hook cancelled".to_string(),
                status: 1,
                aborted: true,
                backgrounded: false,
            });
        }
    }

    Ok(result)
}
// ---------------------------------------------------------------------------
// Core hook execution
// ---------------------------------------------------------------------------

/// Execute hooks and collect aggregated results.
/// Replaces the TS async generator `executeHooks`.
pub async fn execute_hooks(
    ctx: &HooksContext,
    hook_input: &Value,
    tool_use_id: &str,
    match_query: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    messages: Option<&[Value]>,
    force_sync_execution: bool,
) -> Vec<AggregatedHookResult> {
    if ctx.disable_all_hooks {
        return Vec::new();
    }
    if ctx.simple_mode {
        return Vec::new();
    }

    let hook_event = hook_input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let hook_name = match match_query {
        Some(q) => format!("{}:{}", hook_event, q),
        None => hook_event.to_string(),
    };

    // Security: all hooks require workspace trust in interactive mode
    if should_skip_hook_due_to_trust(ctx) {
        (ctx.log_debug)(&format!(
            "Skipping {} hook execution - workspace trust not accepted",
            hook_name
        ));
        return Vec::new();
    }

    let matching_hooks = get_matching_hooks(ctx, hook_event, hook_input, match_query).await;
    if matching_hooks.is_empty() {
        return Vec::new();
    }

    if let Some(token) = cancel_token {
        if token.is_cancelled() {
            return Vec::new();
        }
    }

    let user_hooks: Vec<&MatchedHook> = matching_hooks
        .iter()
        .filter(|h| !is_internal_hook(h))
        .collect();

    // Fast-path: all hooks are internal callbacks
    if user_hooks.is_empty() {
        let batch_start = Instant::now();
        // Execute internal callbacks sequentially
        for (_i, matched) in matching_hooks.iter().enumerate() {
            if let Hook::Callback(cb) = &matched.hook {
                let token = cancel_token.map(|t| t.clone());
                let _result = (cb.callback)(
                    hook_input.clone(),
                    tool_use_id.to_string(),
                    token,
                    0,
                    None,
                )
                .await;
            }
        }
        let total_ms = batch_start.elapsed().as_millis() as u64;
        (ctx.log_event)(
            "tengu_repl_hook_finished",
            &serde_json::json!({
                "hookName": hook_name,
                "numCommands": matching_hooks.len(),
                "numSuccess": matching_hooks.len(),
                "numBlocking": 0,
                "numNonBlockingError": 0,
                "numCancelled": 0,
                "totalDurationMs": total_ms,
            }),
        );
        return Vec::new();
    }

    // Log analytics for user hooks
    if !user_hooks.is_empty() {
        let plugin_hook_counts = get_plugin_hook_counts(
            &matching_hooks,
            &ctx.allowed_official_marketplace_names,
        );
        let hook_type_counts = get_hook_type_counts(&matching_hooks);
        let mut event_data = serde_json::json!({
            "hookName": hook_name,
            "numCommands": user_hooks.len(),
            "hookTypeCounts": serde_json::to_string(&hook_type_counts).unwrap_or_default(),
        });
        if let Some(counts) = plugin_hook_counts {
            event_data["pluginHookCounts"] =
                serde_json::Value::String(serde_json::to_string(&counts).unwrap_or_default());
        }
        (ctx.log_event)("tengu_run_hook", &event_data);
    }

    let batch_start = Instant::now();
    let mut all_results: Vec<AggregatedHookResult> = Vec::new();
    let mut outcomes = HookOutcomeCounters::default();

    // Serialize json input once
    let json_input = match serde_json::to_string(hook_input) {
        Ok(s) => s,
        Err(e) => {
            (ctx.log_error)(&format!("Failed to stringify hook {} input: {}", hook_name, e));
            return Vec::new();
        }
    };

    // Execute hooks (in parallel for command hooks, sequentially for callbacks)
    let mut hook_futures: Vec<futures::future::BoxFuture<'_, Vec<HookResult>>> = Vec::new();

    for (hook_index, matched) in matching_hooks.iter().enumerate() {
        let hook_id = Uuid::new_v4().to_string();
        let hook_start = Instant::now();
        let hook_command = get_hook_display_text(&matched.hook);

        match &matched.hook {
            Hook::Callback(cb) => {
                let token = cancel_token.map(|t| t.clone());
                let cb = cb.clone();
                let input = hook_input.clone();
                let tu_id = tool_use_id.to_string();
                let hn = hook_name.clone();
                let he = hook_event.to_string();

                let fut = Box::pin(async move {
                    let json = (cb.callback)(input, tu_id.clone(), token, hook_index, None).await;
                    if is_async_hook_json_output(&json) {
                        return vec![HookResult {
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
                            hook: MatchedHookInfo {
                                hook_type: "callback".to_string(),
                                display_text: "callback".to_string(),
                            },
                        }];
                    }
                    let processed = process_hook_json_output(
                        &json, "callback", &hn, &tu_id, &he, Some(&he),
                        None, None, None, None,
                    );
                    vec![HookResult {
                        message: processed.message,
                        system_message: processed.system_message,
                        blocking_error: processed.blocking_error,
                        outcome: HookOutcome::Success,
                        prevent_continuation: processed.prevent_continuation,
                        stop_reason: processed.stop_reason,
                        permission_behavior: processed.permission_behavior,
                        hook_permission_decision_reason: processed.hook_permission_decision_reason,
                        additional_context: processed.additional_context,
                        initial_user_message: processed.initial_user_message,
                        updated_input: processed.updated_input,
                        updated_mcp_tool_output: processed.updated_mcp_tool_output,
                        permission_request_result: processed.permission_request_result,
                        elicitation_response: processed.elicitation_response,
                        watch_paths: processed.watch_paths,
                        elicitation_result_response: processed.elicitation_result_response,
                        retry: processed.retry,
                        hook: MatchedHookInfo {
                            hook_type: "callback".to_string(),
                            display_text: "callback".to_string(),
                        },
                    }]
                });
                hook_futures.push(fut);
            }
            Hook::Function(fh) => {
                let fh = fh.clone();
                let hn = hook_name.clone();
                let he = hook_event.to_string();
                let tu_id = tool_use_id.to_string();
                let msgs = messages.map(|m| m.to_vec()).unwrap_or_default();
                let token = cancel_token.map(|t| t.clone());

                let fut = Box::pin(async move {
                    if msgs.is_empty() {
                        return vec![HookResult {
                            message: Some(create_attachment_message_value(
                                "hook_error_during_execution",
                                &hn, &he, &None,
                            )),
                            system_message: None,
                            blocking_error: None,
                            outcome: HookOutcome::NonBlockingError,
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
                            hook: MatchedHookInfo {
                                hook_type: "function".to_string(),
                                display_text: "function".to_string(),
                            },
                        }];
                    }
                    let passed = (fh.callback)(msgs, token).await;
                    if passed {
                        vec![HookResult {
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
                            hook: MatchedHookInfo {
                                hook_type: "function".to_string(),
                                display_text: "function".to_string(),
                            },
                        }]
                    } else {
                        vec![HookResult {
                            message: None,
                            system_message: None,
                            blocking_error: Some(HookBlockingError {
                                blocking_error: fh.error_message.clone(),
                                command: "function".to_string(),
                            }),
                            outcome: HookOutcome::Blocking,
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
                            hook: MatchedHookInfo {
                                hook_type: "function".to_string(),
                                display_text: "function".to_string(),
                            },
                        }]
                    }
                });
                hook_futures.push(fut);
            }
            Hook::Command(cmd) => {
                // Command hooks executed via shell
                let cmd = cmd.clone();
                let hn = hook_name.clone();
                let he = hook_event.to_string();
                let tu_id = tool_use_id.to_string();
                let ji = json_input.clone();
                let hc = hook_command.clone();
                let pr = matched.plugin_root.clone();
                let pid = matched.plugin_id.clone();
                let sr = matched.skill_root.clone();
                // We need to pass ctx references — use async block
                // Note: we can't easily send &ctx across threads, so we clone needed fields
                let subprocess_env = ctx.subprocess_env.clone();
                let project_root = ctx.project_root.clone();
                let original_cwd = ctx.original_cwd.clone();

                let fut = Box::pin(async move {
                    let hook_timeout_ms = cmd.timeout
                        .map(|t| t * 1000)
                        .unwrap_or(TOOL_HOOK_EXECUTION_TIMEOUT_MS);

                    let mut command_str = cmd.command.clone();
                    if let Some(ref pr) = pr {
                        command_str = command_str.replace("${MOSSEN_PLUGIN_ROOT}", pr);
                    }

                    let mut env_vars = subprocess_env;
                    env_vars.insert("MOSSEN_PROJECT_DIR".to_string(), project_root.clone());
                    if let Some(ref pr) = pr {
                        env_vars.insert("MOSSEN_PLUGIN_ROOT".to_string(), pr.clone());
                    }
                    if let Some(ref sr) = sr {
                        env_vars.insert("MOSSEN_PLUGIN_ROOT".to_string(), sr.clone());
                    }

                    let safe_cwd = if tokio::fs::metadata(&original_cwd).await.is_ok() {
                        original_cwd.clone()
                    } else {
                        project_root.clone()
                    };

                    let shell_type = cmd.shell.as_deref().unwrap_or(DEFAULT_HOOK_SHELL);
                    let spawn_result = if shell_type == "powershell" {
                        Command::new("pwsh")
                            .args(&["-NoProfile", "-NonInteractive", "-Command", &command_str])
                            .envs(&env_vars)
                            .current_dir(&safe_cwd)
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                    } else {
                        Command::new("sh")
                            .args(&["-c", &command_str])
                            .envs(&env_vars)
                            .current_dir(&safe_cwd)
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                    };

                    let mut child = match spawn_result {
                        Ok(c) => c,
                        Err(e) => {
                            let err_msg = format!("Failed to run: {}", e);
                            return vec![HookResult {
                                message: Some(create_attachment_message_value(
                                    "hook_non_blocking_error", &hn, &he, &None,
                                )),
                                system_message: None,
                                blocking_error: None,
                                outcome: HookOutcome::NonBlockingError,
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
                                hook: MatchedHookInfo {
                                    hook_type: "command".to_string(),
                                    display_text: hc.clone(),
                                },
                            }];
                        }
                    };

                    // Write stdin
                    if let Some(stdin) = child.stdin.take() {
                        use tokio::io::AsyncWriteExt;
                        let mut stdin = stdin;
                        let input = format!("{}\n", ji);
                        let _ = stdin.write_all(input.as_bytes()).await;
                        drop(stdin);
                    }

                    let timeout = Duration::from_millis(hook_timeout_ms);
                    let exec_result = match tokio::time::timeout(timeout, child.wait_with_output()).await {
                        Ok(Ok(output)) => {
                            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                            let status = output.status.code().unwrap_or(1);
                            CommandHookExecResult {
                                output: format!("{}{}", stdout, stderr),
                                stdout, stderr, status,
                                aborted: false, backgrounded: false,
                            }
                        }
                        Ok(Err(e)) => CommandHookExecResult {
                            stdout: String::new(),
                            stderr: format!("Error: {}", e),
                            output: format!("Error: {}", e),
                            status: 1, aborted: false, backgrounded: false,
                        },
                        Err(_) => CommandHookExecResult {
                            stdout: String::new(),
                            stderr: "Hook cancelled (timeout)".to_string(),
                            output: "Hook cancelled (timeout)".to_string(),
                            status: 1, aborted: true, backgrounded: false,
                        },
                    };

                    let duration_ms = hook_start.elapsed().as_millis() as u64;

                    if exec_result.aborted {
                        return vec![HookResult {
                            message: Some(create_attachment_message_value(
                                "hook_cancelled", &hn, &he, &None,
                            )),
                            system_message: None,
                            blocking_error: None,
                            outcome: HookOutcome::Cancelled,
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
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }];
                    }

                    // Parse output
                    let parsed = parse_hook_output(&exec_result.stdout);

                    if let Some(ref ve) = parsed.validation_error {
                        return vec![HookResult {
                            message: Some(create_attachment_message_value(
                                "hook_non_blocking_error", &hn, &he, &None,
                            )),
                            system_message: None,
                            blocking_error: None,
                            outcome: HookOutcome::NonBlockingError,
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
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }];
                    }

                    if let Some(ref json) = parsed.json {
                        if is_async_hook_json_output(json) {
                            return vec![HookResult {
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
                                hook: MatchedHookInfo {
                                    hook_type: "command".to_string(),
                                    display_text: hc.clone(),
                                },
                            }];
                        }

                        let processed = process_hook_json_output(
                            json, &hc, &hn, &tu_id, &he, Some(&he),
                            Some(&exec_result.stdout), Some(&exec_result.stderr),
                            Some(exec_result.status), Some(duration_ms),
                        );
                        return vec![HookResult {
                            message: processed.message,
                            system_message: processed.system_message,
                            blocking_error: processed.blocking_error,
                            outcome: HookOutcome::Success,
                            prevent_continuation: processed.prevent_continuation,
                            stop_reason: processed.stop_reason,
                            permission_behavior: processed.permission_behavior,
                            hook_permission_decision_reason: processed.hook_permission_decision_reason,
                            additional_context: processed.additional_context,
                            initial_user_message: processed.initial_user_message,
                            updated_input: processed.updated_input,
                            updated_mcp_tool_output: processed.updated_mcp_tool_output,
                            permission_request_result: processed.permission_request_result,
                            elicitation_response: processed.elicitation_response,
                            watch_paths: processed.watch_paths,
                            elicitation_result_response: processed.elicitation_result_response,
                            retry: processed.retry,
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }];
                    }

                    // Plain text output
                    if exec_result.status == 0 {
                        vec![HookResult {
                            message: Some(create_attachment_message_value(
                                "hook_success", &hn, &he, &None,
                            )),
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
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }]
                    } else if exec_result.status == 2 {
                        // Exit code 2 = blocking
                        vec![HookResult {
                            message: None,
                            system_message: None,
                            blocking_error: Some(HookBlockingError {
                                blocking_error: format!(
                                    "[{}]: {}",
                                    cmd.command,
                                    if exec_result.stderr.is_empty() {
                                        "No stderr output"
                                    } else {
                                        &exec_result.stderr
                                    }
                                ),
                                command: cmd.command.clone(),
                            }),
                            outcome: HookOutcome::Blocking,
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
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }]
                    } else {
                        vec![HookResult {
                            message: Some(create_attachment_message_value(
                                "hook_non_blocking_error", &hn, &he, &None,
                            )),
                            system_message: None,
                            blocking_error: None,
                            outcome: HookOutcome::NonBlockingError,
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
                            hook: MatchedHookInfo {
                                hook_type: "command".to_string(),
                                display_text: hc.clone(),
                            },
                        }]
                    }
                });
                hook_futures.push(fut);
            }
            // Prompt, Agent, Http hooks use simplified execution
            Hook::Prompt(_) | Hook::Agent(_) | Hook::Http(_) => {
                // These hooks need tool use context which we don't have in pure Rust
                // They delegate to external executors; for now return success placeholder
                let hn = hook_name.clone();
                let he = hook_event.to_string();
                let ht = matched.hook.hook_type().to_string();
                let display = hook_command.clone();

                let fut = Box::pin(async move {
                    vec![HookResult {
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
                        hook: MatchedHookInfo {
                            hook_type: ht,
                            display_text: display,
                        },
                    }]
                });
                hook_futures.push(fut);
            }
        }
    }

    // Run all hooks in parallel
    let results = futures::future::join_all(hook_futures).await;

    // Aggregate results
    let mut permission_behavior: Option<String> = None;

    for hook_results in results {
        for result in hook_results {
            outcomes.count(&result.outcome);

            if result.prevent_continuation == Some(true) {
                all_results.push(AggregatedHookResult {
                    prevent_continuation: Some(true),
                    stop_reason: result.stop_reason.clone(),
                    ..Default::default()
                });
            }

            if let Some(ref err) = result.blocking_error {
                all_results.push(AggregatedHookResult {
                    blocking_error: Some(err.clone()),
                    ..Default::default()
                });
            }

            if let Some(ref msg) = result.message {
                all_results.push(AggregatedHookResult {
                    message: Some(msg.clone()),
                    ..Default::default()
                });
            }

            if let Some(ref sm) = result.system_message {
                all_results.push(AggregatedHookResult {
                    message: Some(create_attachment_message_value(
                        "hook_system_message",
                        &hook_name,
                        hook_event,
                        &None,
                    )),
                    ..Default::default()
                });
            }

            if let Some(ref ac) = result.additional_context {
                all_results.push(AggregatedHookResult {
                    additional_contexts: Some(vec![ac.clone()]),
                    ..Default::default()
                });
            }

            if let Some(ref ium) = result.initial_user_message {
                all_results.push(AggregatedHookResult {
                    initial_user_message: Some(ium.clone()),
                    ..Default::default()
                });
            }

            if let Some(ref wp) = result.watch_paths {
                if !wp.is_empty() {
                    all_results.push(AggregatedHookResult {
                        watch_paths: Some(wp.clone()),
                        ..Default::default()
                    });
                }
            }

            if let Some(ref umto) = result.updated_mcp_tool_output {
                all_results.push(AggregatedHookResult {
                    updated_mcp_tool_output: Some(umto.clone()),
                    ..Default::default()
                });
            }

            // Permission behavior precedence: deny > ask > allow
            if let Some(ref pb) = result.permission_behavior {
                match pb.as_str() {
                    "deny" => permission_behavior = Some("deny".to_string()),
                    "ask" => {
                        if permission_behavior.as_deref() != Some("deny") {
                            permission_behavior = Some("ask".to_string());
                        }
                    }
                    "allow" => {
                        if permission_behavior.is_none() {
                            permission_behavior = Some("allow".to_string());
                        }
                    }
                    "passthrough" => {}
                    _ => {}
                }
            }

            if permission_behavior.is_some() {
                let updated_input = if result.updated_input.is_some()
                    && (result.permission_behavior.as_deref() == Some("allow")
                        || result.permission_behavior.as_deref() == Some("ask"))
                {
                    result.updated_input.clone()
                } else {
                    None
                };
                all_results.push(AggregatedHookResult {
                    permission_behavior: permission_behavior.clone(),
                    hook_permission_decision_reason: result
                        .hook_permission_decision_reason
                        .clone(),
                    updated_input,
                    ..Default::default()
                });
            }

            // Passthrough updated input
            if result.updated_input.is_some() && result.permission_behavior.is_none() {
                all_results.push(AggregatedHookResult {
                    updated_input: result.updated_input.clone(),
                    ..Default::default()
                });
            }

            if let Some(ref prr) = result.permission_request_result {
                all_results.push(AggregatedHookResult {
                    permission_request_result: Some(prr.clone()),
                    ..Default::default()
                });
            }

            if result.retry == Some(true) {
                all_results.push(AggregatedHookResult {
                    retry: Some(true),
                    ..Default::default()
                });
            }

            if let Some(ref er) = result.elicitation_response {
                all_results.push(AggregatedHookResult {
                    elicitation_response: Some(er.clone()),
                    ..Default::default()
                });
            }

            if let Some(ref err) = result.elicitation_result_response {
                all_results.push(AggregatedHookResult {
                    elicitation_result_response: Some(err.clone()),
                    ..Default::default()
                });
            }
        }
    }

    let total_ms = batch_start.elapsed().as_millis() as u64;
    (ctx.log_event)(
        "tengu_repl_hook_finished",
        &serde_json::json!({
            "hookName": hook_name,
            "numCommands": matching_hooks.len(),
            "numSuccess": outcomes.success,
            "numBlocking": outcomes.blocking,
            "numNonBlockingError": outcomes.non_blocking_error,
            "numCancelled": outcomes.cancelled,
            "totalDurationMs": total_ms,
        }),
    );

    all_results
}

/// Outcome counters for hook execution tracking.
#[derive(Debug, Default)]
struct HookOutcomeCounters {
    success: usize,
    blocking: usize,
    non_blocking_error: usize,
    cancelled: usize,
}

impl HookOutcomeCounters {
    fn count(&mut self, outcome: &HookOutcome) {
        match outcome {
            HookOutcome::Success => self.success += 1,
            HookOutcome::Blocking => self.blocking += 1,
            HookOutcome::NonBlockingError => self.non_blocking_error += 1,
            HookOutcome::Cancelled => self.cancelled += 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Execute hooks outside REPL
// ---------------------------------------------------------------------------

/// Execute hooks outside of the REPL (notifications, session end, etc.).
pub async fn execute_hooks_outside_repl(
    ctx: &HooksContext,
    hook_input: &Value,
    match_query: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<HookOutsideReplResult> {
    if ctx.simple_mode {
        return Vec::new();
    }

    let hook_event = hook_input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let hook_name = match match_query {
        Some(q) => format!("{}:{}", hook_event, q),
        None => hook_event.to_string(),
    };

    if ctx.disable_all_hooks {
        (ctx.log_debug)(&format!(
            "Skipping hooks for {} due to 'disableAllHooks' managed setting",
            hook_name
        ));
        return Vec::new();
    }

    if should_skip_hook_due_to_trust(ctx) {
        (ctx.log_debug)(&format!(
            "Skipping {} hook execution - workspace trust not accepted",
            hook_name
        ));
        return Vec::new();
    }

    let matching_hooks = get_matching_hooks(ctx, hook_event, hook_input, match_query).await;
    if matching_hooks.is_empty() {
        return Vec::new();
    }

    if let Some(token) = cancel_token {
        if token.is_cancelled() {
            return Vec::new();
        }
    }

    // Stringify input
    let json_input = match serde_json::to_string(hook_input) {
        Ok(s) => s,
        Err(e) => {
            (ctx.log_error)(&format!("Failed to stringify hook input: {}", e));
            return Vec::new();
        }
    };

    // Execute each hook
    let mut results: Vec<HookOutsideReplResult> = Vec::new();

    for (hook_index, matched) in matching_hooks.iter().enumerate() {
        match &matched.hook {
            Hook::Callback(cb) => {
                let token = cancel_token.map(|t| t.clone());
                let tool_use_id = Uuid::new_v4().to_string();
                let json = (cb.callback)(
                    hook_input.clone(),
                    tool_use_id,
                    token,
                    hook_index,
                    None,
                )
                .await;
                let blocked = is_sync_hook_json_output(&json)
                    && json.get("decision").and_then(|v| v.as_str()) == Some("block");
                results.push(HookOutsideReplResult {
                    command: "callback".to_string(),
                    succeeded: true,
                    output: json
                        .get("systemMessage")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    blocked,
                    watch_paths: None,
                    system_message: None,
                });
            }
            Hook::Prompt(h) => {
                results.push(HookOutsideReplResult {
                    command: h.prompt.clone(),
                    succeeded: false,
                    output: "Prompt stop hooks are not yet supported outside REPL".to_string(),
                    blocked: false,
                    watch_paths: None,
                    system_message: None,
                });
            }
            Hook::Agent(h) => {
                results.push(HookOutsideReplResult {
                    command: h.prompt.clone(),
                    succeeded: false,
                    output: "Agent stop hooks are not yet supported outside REPL".to_string(),
                    blocked: false,
                    watch_paths: None,
                    system_message: None,
                });
            }
            Hook::Function(_) => {
                (ctx.log_error)(&format!(
                    "Function hook reached executeHooksOutsideREPL for {}",
                    hook_event
                ));
                results.push(HookOutsideReplResult {
                    command: "function".to_string(),
                    succeeded: false,
                    output: "Internal error: function hook executed outside REPL context"
                        .to_string(),
                    blocked: false,
                    watch_paths: None,
                    system_message: None,
                });
            }
            Hook::Http(h) => {
                // HTTP hooks would use reqwest; simplified here
                results.push(HookOutsideReplResult {
                    command: h.url.clone(),
                    succeeded: false,
                    output: "HTTP hooks not yet implemented in Rust backend".to_string(),
                    blocked: false,
                    watch_paths: None,
                    system_message: None,
                });
            }
            Hook::Command(cmd) => {
                match exec_command_hook(
                    cmd,
                    hook_event,
                    &hook_name,
                    &json_input,
                    cancel_token,
                    ctx,
                    matched.plugin_root.as_deref(),
                    matched.plugin_id.as_deref(),
                    matched.skill_root.as_deref(),
                )
                .await
                {
                    Ok(exec_result) => {
                        if exec_result.aborted {
                            results.push(HookOutsideReplResult {
                                command: cmd.command.clone(),
                                succeeded: false,
                                output: "Hook cancelled".to_string(),
                                blocked: false,
                                watch_paths: None,
                                system_message: None,
                            });
                            continue;
                        }

                        let parsed = parse_hook_output(&exec_result.stdout);
                        let json_blocked = parsed.json.as_ref().map_or(false, |j| {
                            is_sync_hook_json_output(j)
                                && j.get("decision").and_then(|v| v.as_str()) == Some("block")
                        });
                        let blocked = exec_result.status == 2 || json_blocked;
                        let output = if exec_result.status == 0 {
                            exec_result.stdout
                        } else {
                            exec_result.stderr
                        };

                        let watch_paths = parsed.json.as_ref().and_then(|j| {
                            j.get("hookSpecificOutput")
                                .and_then(|s| s.get("watchPaths"))
                                .and_then(|w| w.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                        });

                        let system_message = parsed.json.as_ref().and_then(|j| {
                            j.get("systemMessage")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        });

                        results.push(HookOutsideReplResult {
                            command: cmd.command.clone(),
                            succeeded: exec_result.status == 0,
                            output,
                            blocked,
                            watch_paths,
                            system_message,
                        });
                    }
                    Err(e) => {
                        results.push(HookOutsideReplResult {
                            command: cmd.command.clone(),
                            succeeded: false,
                            output: e.to_string(),
                            blocked: false,
                            watch_paths: None,
                            system_message: None,
                        });
                    }
                }
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Specific hook execution functions
// ---------------------------------------------------------------------------

/// Execute env hooks (CwdChanged, FileChanged).
async fn execute_env_hooks(
    ctx: &HooksContext,
    hook_input: &Value,
    timeout_ms: u64,
) -> (Vec<HookOutsideReplResult>, Vec<String>, Vec<String>) {
    let results = execute_hooks_outside_repl(ctx, hook_input, None, None, timeout_ms).await;
    if !results.is_empty() {
        (ctx.invalidate_session_env_cache)();
    }
    let watch_paths: Vec<String> = results
        .iter()
        .flat_map(|r| r.watch_paths.iter().flatten().cloned())
        .collect();
    let system_messages: Vec<String> = results
        .iter()
        .filter_map(|r| r.system_message.clone())
        .collect();
    (results, watch_paths, system_messages)
}

/// Execute pre-tool hooks if configured.
pub async fn execute_pre_tool_hooks(
    ctx: &HooksContext,
    tool_name: &str,
    tool_use_id: &str,
    tool_input: &Value,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    if !has_hook_for_event(ctx, "PreToolUse") {
        return Vec::new();
    }

    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "permission_mode": base.permission_mode,
        "agent_id": base.agent_id,
        "agent_type": base.agent_type,
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": tool_use_id,
    });

    execute_hooks(
        ctx,
        &hook_input,
        tool_use_id,
        Some(tool_name),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute post-tool hooks if configured.
pub async fn execute_post_tool_hooks(
    ctx: &HooksContext,
    tool_name: &str,
    tool_use_id: &str,
    tool_input: &Value,
    tool_response: &Value,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "permission_mode": base.permission_mode,
        "hook_event_name": "PostToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_response": tool_response,
        "tool_use_id": tool_use_id,
    });

    execute_hooks(
        ctx,
        &hook_input,
        tool_use_id,
        Some(tool_name),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute post-tool-use-failure hooks.
pub async fn execute_post_tool_use_failure_hooks(
    ctx: &HooksContext,
    tool_name: &str,
    tool_use_id: &str,
    tool_input: &Value,
    error: &str,
    is_interrupt: Option<bool>,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    if !has_hook_for_event(ctx, "PostToolUseFailure") {
        return Vec::new();
    }

    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "PostToolUseFailure",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": tool_use_id,
        "error": error,
        "is_interrupt": is_interrupt,
    });

    execute_hooks(
        ctx,
        &hook_input,
        tool_use_id,
        Some(tool_name),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute permission denied hooks.
pub async fn execute_permission_denied_hooks(
    ctx: &HooksContext,
    tool_name: &str,
    tool_use_id: &str,
    tool_input: &Value,
    reason: &str,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    if !has_hook_for_event(ctx, "PermissionDenied") {
        return Vec::new();
    }

    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "PermissionDenied",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "tool_use_id": tool_use_id,
        "reason": reason,
    });

    execute_hooks(
        ctx,
        &hook_input,
        tool_use_id,
        Some(tool_name),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute notification hooks.
pub async fn execute_notification_hooks(
    ctx: &HooksContext,
    message: &str,
    title: Option<&str>,
    notification_type: &str,
    timeout_ms: u64,
) {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "Notification",
        "message": message,
        "title": title,
        "notification_type": notification_type,
    });

    let _ = execute_hooks_outside_repl(
        ctx,
        &hook_input,
        Some(notification_type),
        None,
        timeout_ms,
    )
    .await;
}

/// Execute stop failure hooks.
pub async fn execute_stop_failure_hooks(
    ctx: &HooksContext,
    last_assistant_text: Option<&str>,
    error: &str,
    error_details: Option<&str>,
    timeout_ms: u64,
) {
    if !has_hook_for_event(ctx, "StopFailure") {
        return;
    }

    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "StopFailure",
        "error": error,
        "error_details": error_details,
        "last_assistant_message": last_assistant_text,
    });

    let _ = execute_hooks_outside_repl(ctx, &hook_input, Some(error), None, timeout_ms).await;
}

/// Execute stop hooks.
pub async fn execute_stop_hooks(
    ctx: &HooksContext,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    stop_hook_active: bool,
    subagent_id: Option<&str>,
    messages: Option<&[Value]>,
    agent_type: Option<&str>,
) -> Vec<AggregatedHookResult> {
    let hook_event = if subagent_id.is_some() {
        "SubagentStop"
    } else {
        "Stop"
    };
    if !has_hook_for_event(ctx, hook_event) {
        return Vec::new();
    }

    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = if let Some(agent_id) = subagent_id {
        let agent_transcript_path = (ctx.get_agent_transcript_path)(agent_id);
        serde_json::json!({
            "session_id": base.session_id,
            "transcript_path": base.transcript_path,
            "cwd": base.cwd,
            "hook_event_name": "SubagentStop",
            "stop_hook_active": stop_hook_active,
            "agent_id": agent_id,
            "agent_transcript_path": agent_transcript_path,
            "agent_type": agent_type.unwrap_or(""),
        })
    } else {
        serde_json::json!({
            "session_id": base.session_id,
            "transcript_path": base.transcript_path,
            "cwd": base.cwd,
            "hook_event_name": "Stop",
            "stop_hook_active": stop_hook_active,
        })
    };

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        None,
        cancel_token,
        timeout_ms,
        messages,
        false,
    )
    .await
}

/// Execute teammate idle hooks.
pub async fn execute_teammate_idle_hooks(
    ctx: &HooksContext,
    teammate_name: &str,
    team_name: &str,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "TeammateIdle",
        "teammate_name": teammate_name,
        "team_name": team_name,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        None,
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute task created hooks.
pub async fn execute_task_created_hooks(
    ctx: &HooksContext,
    task_id: &str,
    task_subject: &str,
    task_description: Option<&str>,
    teammate_name: Option<&str>,
    team_name: Option<&str>,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "TaskCreated",
        "task_id": task_id,
        "task_subject": task_subject,
        "task_description": task_description,
        "teammate_name": teammate_name,
        "team_name": team_name,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        None,
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute task completed hooks.
pub async fn execute_task_completed_hooks(
    ctx: &HooksContext,
    task_id: &str,
    task_subject: &str,
    task_description: Option<&str>,
    teammate_name: Option<&str>,
    team_name: Option<&str>,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "TaskCompleted",
        "task_id": task_id,
        "task_subject": task_subject,
        "task_description": task_description,
        "teammate_name": teammate_name,
        "team_name": team_name,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        None,
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute user prompt submit hooks.
pub async fn execute_user_prompt_submit_hooks(
    ctx: &HooksContext,
    prompt: &str,
    permission_mode: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
) -> Vec<AggregatedHookResult> {
    if !has_hook_for_event(ctx, "UserPromptSubmit") {
        return Vec::new();
    }

    let base = create_base_hook_input(ctx, Some(permission_mode), None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "UserPromptSubmit",
        "prompt": prompt,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        None,
        cancel_token,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
        None,
        false,
    )
    .await
}

/// Execute session start hooks.
pub async fn execute_session_start_hooks(
    ctx: &HooksContext,
    source: &str,
    session_id: Option<&str>,
    agent_type: Option<&str>,
    model: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    force_sync_execution: bool,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, None, session_id, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "SessionStart",
        "source": source,
        "agent_type": agent_type,
        "model": model,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        Some(source),
        cancel_token,
        timeout_ms,
        None,
        force_sync_execution,
    )
    .await
}

/// Execute setup hooks.
pub async fn execute_setup_hooks(
    ctx: &HooksContext,
    trigger: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    force_sync_execution: bool,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "Setup",
        "trigger": trigger,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        Some(trigger),
        cancel_token,
        timeout_ms,
        None,
        force_sync_execution,
    )
    .await
}

/// Execute subagent start hooks.
pub async fn execute_subagent_start_hooks(
    ctx: &HooksContext,
    agent_id: &str,
    agent_type: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "SubagentStart",
        "agent_id": agent_id,
        "agent_type": agent_type,
    });

    let tool_use_id = Uuid::new_v4().to_string();
    execute_hooks(
        ctx,
        &hook_input,
        &tool_use_id,
        Some(agent_type),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute pre-compact hooks.
pub async fn execute_pre_compact_hooks(
    ctx: &HooksContext,
    trigger: &str,
    custom_instructions: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> (Option<String>, Option<String>) {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "PreCompact",
        "trigger": trigger,
        "custom_instructions": custom_instructions,
    });

    let results =
        execute_hooks_outside_repl(ctx, &hook_input, Some(trigger), cancel_token, timeout_ms)
            .await;

    if results.is_empty() {
        return (None, None);
    }

    let successful_outputs: Vec<String> = results
        .iter()
        .filter(|r| r.succeeded && !r.output.trim().is_empty())
        .map(|r| r.output.trim().to_string())
        .collect();

    let display_messages: Vec<String> = results
        .iter()
        .map(|r| {
            if r.succeeded {
                if r.output.trim().is_empty() {
                    format!("PreCompact [{}] completed successfully", r.command)
                } else {
                    format!(
                        "PreCompact [{}] completed successfully: {}",
                        r.command,
                        r.output.trim()
                    )
                }
            } else if r.output.trim().is_empty() {
                format!("PreCompact [{}] failed", r.command)
            } else {
                format!("PreCompact [{}] failed: {}", r.command, r.output.trim())
            }
        })
        .collect();

    let new_instructions = if successful_outputs.is_empty() {
        None
    } else {
        Some(successful_outputs.join("\n\n"))
    };
    let display = if display_messages.is_empty() {
        None
    } else {
        Some(display_messages.join("\n"))
    };

    (new_instructions, display)
}

/// Execute post-compact hooks.
pub async fn execute_post_compact_hooks(
    ctx: &HooksContext,
    trigger: &str,
    compact_summary: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Option<String> {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "PostCompact",
        "trigger": trigger,
        "compact_summary": compact_summary,
    });

    let results =
        execute_hooks_outside_repl(ctx, &hook_input, Some(trigger), cancel_token, timeout_ms)
            .await;

    if results.is_empty() {
        return None;
    }

    let display_messages: Vec<String> = results
        .iter()
        .map(|r| {
            if r.succeeded {
                if r.output.trim().is_empty() {
                    format!("PostCompact [{}] completed successfully", r.command)
                } else {
                    format!(
                        "PostCompact [{}] completed successfully: {}",
                        r.command,
                        r.output.trim()
                    )
                }
            } else if r.output.trim().is_empty() {
                format!("PostCompact [{}] failed", r.command)
            } else {
                format!("PostCompact [{}] failed: {}", r.command, r.output.trim())
            }
        })
        .collect();

    if display_messages.is_empty() {
        None
    } else {
        Some(display_messages.join("\n"))
    }
}

/// Execute session end hooks.
pub async fn execute_session_end_hooks(
    ctx: &HooksContext,
    reason: &str,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) {
    if ctx.custom_backend_enabled {
        if std::env::var("MOSSEN_CODE_ENABLE_CUSTOM_BACKEND_SESSION_END_HOOKS")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }
    }

    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "SessionEnd",
        "reason": reason,
    });

    let results =
        execute_hooks_outside_repl(ctx, &hook_input, Some(reason), cancel_token, timeout_ms).await;

    for result in &results {
        if !result.succeeded && !result.output.is_empty() {
            eprintln!(
                "SessionEnd hook [{}] failed: {}",
                result.command, result.output
            );
        }
    }
}

/// Execute permission request hooks.
pub async fn execute_permission_request_hooks(
    ctx: &HooksContext,
    tool_name: &str,
    tool_use_id: &str,
    tool_input: &Value,
    permission_mode: Option<&str>,
    permission_suggestions: Option<&Value>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<AggregatedHookResult> {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "PermissionRequest",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "permission_suggestions": permission_suggestions,
    });

    execute_hooks(
        ctx,
        &hook_input,
        tool_use_id,
        Some(tool_name),
        cancel_token,
        timeout_ms,
        None,
        false,
    )
    .await
}

/// Execute config change hooks.
pub async fn execute_config_change_hooks(
    ctx: &HooksContext,
    source: ConfigChangeSource,
    file_path: Option<&str>,
    timeout_ms: u64,
) -> Vec<HookOutsideReplResult> {
    let source_str = match source {
        ConfigChangeSource::UserSettings => "user_settings",
        ConfigChangeSource::ProjectSettings => "project_settings",
        ConfigChangeSource::LocalSettings => "local_settings",
        ConfigChangeSource::PolicySettings => "policy_settings",
        ConfigChangeSource::Skills => "skills",
    };

    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "ConfigChange",
        "source": source_str,
        "file_path": file_path,
    });

    let mut results =
        execute_hooks_outside_repl(ctx, &hook_input, Some(source_str), None, timeout_ms).await;

    // Policy settings cannot be blocked
    if source == ConfigChangeSource::PolicySettings {
        for r in &mut results {
            r.blocked = false;
        }
    }

    results
}

/// Execute CwdChanged hooks.
pub async fn execute_cwd_changed_hooks(
    ctx: &HooksContext,
    old_cwd: &str,
    new_cwd: &str,
    timeout_ms: u64,
) -> (Vec<HookOutsideReplResult>, Vec<String>, Vec<String>) {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "CwdChanged",
        "old_cwd": old_cwd,
        "new_cwd": new_cwd,
    });
    execute_env_hooks(ctx, &hook_input, timeout_ms).await
}

/// Execute FileChanged hooks.
pub async fn execute_file_changed_hooks(
    ctx: &HooksContext,
    file_path: &str,
    event: &str,
    timeout_ms: u64,
) -> (Vec<HookOutsideReplResult>, Vec<String>, Vec<String>) {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "FileChanged",
        "file_path": file_path,
        "event": event,
    });
    execute_env_hooks(ctx, &hook_input, timeout_ms).await
}

/// Check if InstructionsLoaded hooks are configured.
pub fn has_instructions_loaded_hook(ctx: &HooksContext) -> bool {
    has_hook_for_event(ctx, "InstructionsLoaded")
}

/// Execute InstructionsLoaded hooks.
pub async fn execute_instructions_loaded_hooks(
    ctx: &HooksContext,
    file_path: &str,
    memory_type: InstructionsMemoryType,
    load_reason: InstructionsLoadReason,
    globs: Option<&[String]>,
    trigger_file_path: Option<&str>,
    parent_file_path: Option<&str>,
    timeout_ms: u64,
) {
    let memory_type_str = match memory_type {
        InstructionsMemoryType::User => "User",
        InstructionsMemoryType::Project => "Project",
        InstructionsMemoryType::Local => "Local",
        InstructionsMemoryType::Managed => "Managed",
    };
    let load_reason_str = match load_reason {
        InstructionsLoadReason::SessionStart => "session_start",
        InstructionsLoadReason::NestedTraversal => "nested_traversal",
        InstructionsLoadReason::PathGlobMatch => "path_glob_match",
        InstructionsLoadReason::Include => "include",
        InstructionsLoadReason::Compact => "compact",
    };

    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "InstructionsLoaded",
        "file_path": file_path,
        "memory_type": memory_type_str,
        "load_reason": load_reason_str,
        "globs": globs,
        "trigger_file_path": trigger_file_path,
        "parent_file_path": parent_file_path,
    });

    let _ = execute_hooks_outside_repl(ctx, &hook_input, Some(load_reason_str), None, timeout_ms)
        .await;
}

/// Parse elicitation-specific fields from a HookOutsideReplResult.
fn parse_elicitation_hook_output(
    result: &HookOutsideReplResult,
    expected_event_name: &str,
) -> (Option<ElicitationResponse>, Option<HookBlockingError>) {
    // Exit code 2 = blocking
    if result.blocked && !result.succeeded {
        return (
            None,
            Some(HookBlockingError {
                blocking_error: if result.output.is_empty() {
                    "Elicitation blocked by hook".to_string()
                } else {
                    result.output.clone()
                },
                command: result.command.clone(),
            }),
        );
    }

    let trimmed = result.output.trim();
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return (None, None);
    }

    let parsed: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    if is_async_hook_json_output(&parsed) || !is_sync_hook_json_output(&parsed) {
        return (None, None);
    }

    // Check for block decision
    if parsed.get("decision").and_then(|v| v.as_str()) == Some("block") || result.blocked {
        return (
            None,
            Some(HookBlockingError {
                blocking_error: parsed
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Elicitation blocked by hook")
                    .to_string(),
                command: result.command.clone(),
            }),
        );
    }

    let specific = match parsed.get("hookSpecificOutput") {
        Some(s) => s,
        None => return (None, None),
    };

    if specific
        .get("hookEventName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        != expected_event_name
    {
        return (None, None);
    }

    let action = match specific.get("action").and_then(|v| v.as_str()) {
        Some(a) => a.to_string(),
        None => return (None, None),
    };

    let response = ElicitationResponse {
        action: action.clone(),
        content: specific.get("content").cloned(),
    };

    let blocking_error = if action == "decline" {
        Some(HookBlockingError {
            blocking_error: parsed
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or(if expected_event_name == "Elicitation" {
                    "Elicitation denied by hook"
                } else {
                    "Elicitation result blocked by hook"
                })
                .to_string(),
            command: result.command.clone(),
        })
    } else {
        None
    };

    (Some(response), blocking_error)
}

/// Execute elicitation hooks.
pub async fn execute_elicitation_hooks(
    ctx: &HooksContext,
    server_name: &str,
    message: &str,
    requested_schema: Option<&Value>,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    mode: Option<&str>,
    url: Option<&str>,
    elicitation_id: Option<&str>,
) -> ElicitationHookResult {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "Elicitation",
        "mcp_server_name": server_name,
        "message": message,
        "mode": mode,
        "url": url,
        "elicitation_id": elicitation_id,
        "requested_schema": requested_schema,
    });

    let results = execute_hooks_outside_repl(
        ctx,
        &hook_input,
        Some(server_name),
        cancel_token,
        timeout_ms,
    )
    .await;

    let mut elicitation_response = None;
    let mut blocking_error = None;
    for result in &results {
        let (resp, err) = parse_elicitation_hook_output(result, "Elicitation");
        if err.is_some() {
            blocking_error = err;
        }
        if resp.is_some() {
            elicitation_response = resp;
        }
    }

    ElicitationHookResult {
        elicitation_response,
        blocking_error,
    }
}

/// Execute elicitation result hooks.
pub async fn execute_elicitation_result_hooks(
    ctx: &HooksContext,
    server_name: &str,
    action: &str,
    content: Option<&Value>,
    permission_mode: Option<&str>,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    mode: Option<&str>,
    elicitation_id: Option<&str>,
) -> ElicitationResultHookResult {
    let base = create_base_hook_input(ctx, permission_mode, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "ElicitationResult",
        "mcp_server_name": server_name,
        "elicitation_id": elicitation_id,
        "mode": mode,
        "action": action,
        "content": content,
    });

    let results = execute_hooks_outside_repl(
        ctx,
        &hook_input,
        Some(server_name),
        cancel_token,
        timeout_ms,
    )
    .await;

    let mut elicitation_result_response = None;
    let mut blocking_error = None;
    for result in &results {
        let (resp, err) = parse_elicitation_hook_output(result, "ElicitationResult");
        if err.is_some() {
            blocking_error = err;
        }
        if resp.is_some() {
            elicitation_result_response = resp;
        }
    }

    ElicitationResultHookResult {
        elicitation_result_response,
        blocking_error,
    }
}

/// Execute status line command.
pub async fn execute_status_line_command(
    ctx: &HooksContext,
    status_line_input: &Value,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
    log_result: bool,
) -> Option<String> {
    if ctx.disable_all_hooks {
        return None;
    }
    if should_skip_hook_due_to_trust(ctx) {
        return None;
    }

    // Get status line config from settings
    let settings = if ctx.managed_hooks_only {
        (ctx.get_settings_for_source)("policySettings")
    } else {
        (ctx.get_settings)()
    };

    let status_line = settings
        .as_ref()
        .and_then(|s| s.get("statusLine"))
        .cloned();
    let status_line = match status_line {
        Some(sl) if sl.get("type").and_then(|v| v.as_str()) == Some("command") => sl,
        _ => return None,
    };

    let command = match status_line.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return None,
    };

    let hook_cmd = HookCommand {
        hook_type: "command".to_string(),
        command: command.clone(),
        timeout: status_line.get("timeout").and_then(|v| v.as_u64()),
        shell: status_line
            .get("shell")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        is_async: None,
        async_rewake: None,
        if_condition: None,
    };

    let json_input = match serde_json::to_string(status_line_input) {
        Ok(s) => s,
        Err(_) => return None,
    };

    match exec_command_hook(
        &hook_cmd,
        "StatusLine",
        "statusLine",
        &json_input,
        cancel_token,
        ctx,
        None,
        None,
        None,
    )
    .await
    {
        Ok(result) => {
            if result.aborted {
                return None;
            }
            if result.status == 0 {
                let output: String = result
                    .stdout
                    .trim()
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");

                if !output.is_empty() {
                    if log_result {
                        (ctx.log_debug)(&format!(
                            "StatusLine [{}] completed with status {}",
                            command, result.status
                        ));
                    }
                    return Some(output);
                }
            } else if log_result {
                (ctx.log_debug)(&format!(
                    "StatusLine [{}] completed with status {}",
                    command, result.status
                ));
            }
            None
        }
        Err(e) => {
            (ctx.log_debug)(&format!("Status hook failed: {}", e));
            None
        }
    }
}

/// Execute file suggestion command.
pub async fn execute_file_suggestion_command(
    ctx: &HooksContext,
    file_suggestion_input: &Value,
    cancel_token: Option<&tokio_util::sync::CancellationToken>,
    timeout_ms: u64,
) -> Vec<String> {
    if ctx.disable_all_hooks {
        return Vec::new();
    }
    if should_skip_hook_due_to_trust(ctx) {
        return Vec::new();
    }

    let settings = if ctx.managed_hooks_only {
        (ctx.get_settings_for_source)("policySettings")
    } else {
        (ctx.get_settings)()
    };

    let file_suggestion = settings
        .as_ref()
        .and_then(|s| s.get("fileSuggestion"))
        .cloned();
    let file_suggestion = match file_suggestion {
        Some(fs) if fs.get("type").and_then(|v| v.as_str()) == Some("command") => fs,
        _ => return Vec::new(),
    };

    let command = match file_suggestion.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return Vec::new(),
    };

    let hook_cmd = HookCommand {
        hook_type: "command".to_string(),
        command,
        timeout: file_suggestion.get("timeout").and_then(|v| v.as_u64()),
        shell: None,
        is_async: None,
        async_rewake: None,
        if_condition: None,
    };

    let json_input = match serde_json::to_string(file_suggestion_input) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    match exec_command_hook(
        &hook_cmd,
        "FileSuggestion",
        "FileSuggestion",
        &json_input,
        cancel_token,
        ctx,
        None,
        None,
        None,
    )
    .await
    {
        Ok(result) => {
            if result.aborted || result.status != 0 {
                return Vec::new();
            }
            result
                .stdout
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(e) => {
            (ctx.log_debug)(&format!("File suggestion helper failed: {}", e));
            Vec::new()
        }
    }
}

/// Check if WorktreeCreate hooks are configured.
pub fn has_worktree_create_hook(ctx: &HooksContext) -> bool {
    if let Some(snapshot) = &ctx.hooks_config_snapshot {
        if let Some(hooks) = snapshot.get("WorktreeCreate") {
            if !hooks.is_empty() {
                return true;
            }
        }
    }
    if let Some(registered) = &ctx.registered_hooks {
        if let Some(hooks) = registered.get("WorktreeCreate") {
            if hooks.is_empty() {
                return false;
            }
            if ctx.managed_hooks_only {
                return hooks.iter().any(|m| m.plugin_root.is_none());
            }
            return true;
        }
    }
    false
}

/// Execute WorktreeCreate hook.
pub async fn execute_worktree_create_hook(
    ctx: &HooksContext,
    name: &str,
) -> Result<String> {
    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "WorktreeCreate",
        "name": name,
    });

    let results = execute_hooks_outside_repl(
        ctx,
        &hook_input,
        None,
        None,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;

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
                    format!(
                        "{}: {}",
                        r.command,
                        if r.output.trim().is_empty() {
                            "no output"
                        } else {
                            r.output.trim()
                        }
                    )
                })
                .collect();
            Err(anyhow!(
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

/// Execute WorktreeRemove hook.
pub async fn execute_worktree_remove_hook(
    ctx: &HooksContext,
    worktree_path: &str,
) -> bool {
    let has_snapshot = ctx
        .hooks_config_snapshot
        .as_ref()
        .and_then(|s| s.get("WorktreeRemove"))
        .map_or(false, |h| !h.is_empty());
    let has_registered = ctx
        .registered_hooks
        .as_ref()
        .and_then(|s| s.get("WorktreeRemove"))
        .map_or(false, |h| !h.is_empty());
    if !has_snapshot && !has_registered {
        return false;
    }

    let base = create_base_hook_input(ctx, None, None, None);
    let hook_input = serde_json::json!({
        "session_id": base.session_id,
        "transcript_path": base.transcript_path,
        "cwd": base.cwd,
        "hook_event_name": "WorktreeRemove",
        "worktree_path": worktree_path,
    });

    let results = execute_hooks_outside_repl(
        ctx,
        &hook_input,
        None,
        None,
        TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    )
    .await;

    if results.is_empty() {
        return false;
    }

    for result in &results {
        if !result.succeeded {
            (ctx.log_debug)(&format!(
                "WorktreeRemove hook failed [{}]: {}",
                result.command,
                result.output.trim()
            ));
        }
    }

    true
}
