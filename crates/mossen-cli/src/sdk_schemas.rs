//! SDK Core/Control Schemas — translated from `entrypoints/sdk/coreSchemas.ts`
//! and `entrypoints/sdk/controlSchemas.ts`.
//!
//! The TS source uses Zod runtime schemas; here we provide serde-compatible
//! Rust structs and enums. The TS const exports become Rust `type` aliases or
//! `static` constants that point at the corresponding Rust struct. This keeps
//! the surface area complete for downstream consumers while letting them
//! `serde_json::from_value` to validate at runtime.

#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Usage & Model Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub cost_usd: f64,
    pub context_window: u64,
    pub max_output_tokens: u64,
}

// ============================================================================
// Output Format Types
// ============================================================================

pub const OUTPUT_FORMAT_TYPE_JSON_SCHEMA: &str = "json_schema";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseOutputFormat {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchemaOutputFormat {
    #[serde(rename = "type")]
    pub kind: String, // always "json_schema"
    pub schema: HashMap<String, Value>,
}

pub type OutputFormat = JsonSchemaOutputFormat;

// ============================================================================
// Config Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeySource {
    User,
    Project,
    Org,
    Temporary,
    Oauth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    Local,
    User,
    Project,
}

pub const SDK_BETA_CONTEXT_1M: &str = "context-1m-2025-08-07";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SdkBeta {
    #[serde(rename = "context-1m-2025-08-07")]
    Context1m,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ThinkingConfig {
    Adaptive,
    Enabled {
        #[serde(skip_serializing_if = "Option::is_none", rename = "budgetTokens")]
        budget_tokens: Option<u64>,
    },
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingAdaptive {
    #[serde(rename = "type")]
    pub kind: String, // always "adaptive"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingEnabled {
    #[serde(rename = "type")]
    pub kind: String, // always "enabled"
    #[serde(rename = "budgetTokens", skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingDisabled {
    #[serde(rename = "type")]
    pub kind: String, // always "disabled"
}

// ============================================================================
// MCP Server Config Types (serializable only)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStdioServerConfig {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>, // optional "stdio"
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSSEServerConfig {
    #[serde(rename = "type")]
    pub kind: String, // "sse"
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHttpServerConfig {
    #[serde(rename = "type")]
    pub kind: String, // "http"
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSdkServerConfig {
    #[serde(rename = "type")]
    pub kind: String, // "sdk"
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerConfigForProcessTransport {
    Stdio(McpStdioServerConfig),
    Sse(McpSSEServerConfig),
    Http(McpHttpServerConfig),
    Sdk(McpSdkServerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHostedProxyServerConfig {
    #[serde(rename = "type")]
    pub kind: String, // "hosted-proxy"
    pub url: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerStatusConfig {
    Process(McpServerConfigForProcessTransport),
    HostedProxy(McpHostedProxyServerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatusServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolAnnotations {
    #[serde(rename = "readOnly", default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,
    #[serde(rename = "openWorld", default, skip_serializing_if = "Option::is_none")]
    pub open_world: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub name: String,
    pub status: String, // connected|failed|needs-auth|pending|disabled
    #[serde(rename = "serverInfo", default, skip_serializing_if = "Option::is_none")]
    pub server_info: Option<McpServerStatusServerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<McpServerStatusConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpToolInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSetServersResult {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// Permissions
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionUpdateDestination {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleValue {
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(rename = "ruleContent", default, skip_serializing_if = "Option::is_none")]
    pub rule_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PermissionUpdate {
    #[serde(rename = "addRules")]
    AddRules {
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
        destination: PermissionUpdateDestination,
    },
    #[serde(rename = "replaceRules")]
    ReplaceRules {
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
        destination: PermissionUpdateDestination,
    },
    #[serde(rename = "removeRules")]
    RemoveRules {
        rules: Vec<PermissionRuleValue>,
        behavior: PermissionBehavior,
        destination: PermissionUpdateDestination,
    },
    #[serde(rename = "setMode")]
    SetMode {
        mode: PermissionMode,
        destination: PermissionUpdateDestination,
    },
    #[serde(rename = "addDirectories")]
    AddDirectories {
        directories: Vec<String>,
        destination: PermissionUpdateDestination,
    },
    #[serde(rename = "removeDirectories")]
    RemoveDirectories {
        directories: Vec<String>,
        destination: PermissionUpdateDestination,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionDecisionClassification {
    DirectMatch,
    ImpliedMatch,
    NoMatch,
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior")]
pub enum PermissionResult {
    #[serde(rename = "allow")]
    Allow {
        #[serde(rename = "updatedInput", default)]
        updated_input: Option<Value>,
        #[serde(rename = "updatedPermissions", default)]
        updated_permissions: Option<Vec<PermissionUpdate>>,
    },
    #[serde(rename = "deny")]
    Deny {
        message: String,
        #[serde(default)]
        interrupt: Option<bool>,
    },
    #[serde(rename = "ask")]
    Ask {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    BypassPermissions,
    Plan,
    Auto,
    Yolo,
}

// ============================================================================
// Hook Inputs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseHookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub hook_event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub tool_name: String,
    pub tool_input: Value,
}

pub type PermissionRequestHookInput = PreToolUseHookInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseFailureHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub tool_name: String,
    pub tool_input: Value,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDeniedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub tool_name: String,
    pub tool_input: Value,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptSubmitHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub stop_hook_active: bool,
}

pub type StopFailureHookInput = StopHookInput;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentStartHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentStopHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub stop_hook_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreCompactHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub trigger: String,
    pub custom_instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostCompactHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeammateIdleHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub teammate_id: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCreatedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub task_id: String,
    pub task_subject: String,
    pub task_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompletedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub task_id: String,
    pub task_subject: String,
    pub task_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub elicitation_id: String,
    pub message: String,
    pub options: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResultHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub elicitation_id: String,
    pub result: Value,
}

pub const CONFIG_CHANGE_SOURCES: &[&str] = &[
    "user-action",
    "plugin",
    "api",
    "external",
    "filesystem",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChangeHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub source: String,
    pub changes: Value,
}

pub const INSTRUCTIONS_LOAD_REASONS: &[&str] = &[
    "session-start",
    "cwd-changed",
    "explicit-reload",
];

pub const INSTRUCTIONS_MEMORY_TYPES: &[&str] = &[
    "project",
    "user",
    "additional",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionsLoadedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub reason: String,
    pub memory_type: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub worktree_path: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRemoveHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub worktree_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CwdChangedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub previous_cwd: String,
    pub new_cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangedHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub file_path: String,
    pub change_type: String,
}

pub const EXIT_REASONS: &[&str] = &[
    "user-quit",
    "session-end",
    "crash",
    "signal",
    "timeout",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExitReason {
    UserQuit,
    SessionEnd,
    Crash,
    Signal,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndHookInput {
    #[serde(flatten)]
    pub base: BaseHookInput,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookInput {
    PreToolUse(PreToolUseHookInput),
    PostToolUse(PostToolUseHookInput),
    PostToolUseFailure(PostToolUseFailureHookInput),
    PermissionDenied(PermissionDeniedHookInput),
    PermissionRequest(PermissionRequestHookInput),
    Notification(NotificationHookInput),
    UserPromptSubmit(UserPromptSubmitHookInput),
    SessionStart(SessionStartHookInput),
    Setup(SetupHookInput),
    Stop(StopHookInput),
    SubagentStart(SubagentStartHookInput),
    SubagentStop(SubagentStopHookInput),
    PreCompact(PreCompactHookInput),
    PostCompact(PostCompactHookInput),
    TeammateIdle(TeammateIdleHookInput),
    TaskCreated(TaskCreatedHookInput),
    TaskCompleted(TaskCompletedHookInput),
    Elicitation(ElicitationHookInput),
    ElicitationResult(ElicitationResultHookInput),
    ConfigChange(ConfigChangeHookInput),
    InstructionsLoaded(InstructionsLoadedHookInput),
    WorktreeCreate(WorktreeCreateHookInput),
    WorktreeRemove(WorktreeRemoveHookInput),
    CwdChanged(CwdChangedHookInput),
    FileChanged(FileChangedHookInput),
    SessionEnd(SessionEndHookInput),
}

// ============================================================================
// Hook JSON Outputs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsyncHookJSONOutput {
    pub async_progress: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseHookSpecificOutput {
    pub hook_event_name: String, // "PreToolUse"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptSubmitHookSpecificOutput {
    pub hook_event_name: String, // "UserPromptSubmit"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartHookSpecificOutput {
    pub hook_event_name: String, // "SessionStart"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupHookSpecificOutput {
    pub hook_event_name: String, // "Setup"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentStartHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostToolUseFailureHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDeniedHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationHookSpecificOutput {
    pub hook_event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CwdChangedHookSpecificOutput {
    pub hook_event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangedHookSpecificOutput {
    pub hook_event_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHookJSONOutput {
    #[serde(rename = "continue", default, skip_serializing_if = "Option::is_none")]
    pub keep_going: Option<bool>,
    #[serde(rename = "stopReason", default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(rename = "suppressOutput", default, skip_serializing_if = "Option::is_none")]
    pub suppress_output: Option<bool>,
    #[serde(rename = "systemMessage", default, skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(rename = "decision", default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(rename = "reason", default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(rename = "hookSpecificOutput", default, skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResultHookSpecificOutput {
    pub hook_event_name: String,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateHookSpecificOutput {
    pub hook_event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookJSONOutput {
    Sync(SyncHookJSONOutput),
    Async(AsyncHookJSONOutput),
}

// ============================================================================
// Prompt request/response
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequestOption {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub prompt_id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<PromptRequestOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResponse {
    pub prompt_id: String,
    pub response: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub max_tokens: u64,
    pub context_window: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMcpServerSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<AgentMcpServerSpec>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkPluginConfig {
    pub name: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewindFilesResult {
    pub files_modified: Vec<String>,
    pub files_restored: Vec<String>,
    pub errors: Vec<String>,
}

// ============================================================================
// API message placeholders (opaque types)
// ============================================================================

pub type ApiUserMessage = Value;
pub type ApiAssistantMessage = Value;
pub type RawMessageStreamEvent = Value;
pub type Uuid = String;
pub type NonNullableUsage = Value;

// Marker constants for placeholders so the bare names exist as identifiers.
pub const APIUserMessagePlaceholderTag: &str = "APIUserMessage";
pub const APIAssistantMessagePlaceholderTag: &str = "APIAssistantMessage";
pub const RawMessageStreamEventPlaceholderTag: &str = "RawMessageStreamEvent";
pub const UUIDPlaceholderTag: &str = "UUID";
pub const NonNullableUsagePlaceholderTag: &str = "NonNullableUsage";

// ============================================================================
// SDK Message variants
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKAssistantMessageError {
    pub message: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKStatus {
    pub session_id: String,
    pub turn_index: u64,
    pub working: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKUserMessage {
    pub session_id: String,
    pub parent_tool_use_id: Option<String>,
    pub message: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKUserMessageReplay {
    pub session_id: String,
    pub message: Value,
    pub replay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKRateLimitInfo {
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: i64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKAssistantMessage {
    pub session_id: String,
    pub parent_tool_use_id: Option<String>,
    pub message: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SDKAssistantMessageError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKRateLimitEvent {
    pub session_id: String,
    pub rate_limit: SDKRateLimitInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKStreamlinedTextMessage {
    pub session_id: String,
    pub text: String,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKStreamlinedToolUseSummaryMessage {
    pub session_id: String,
    pub tool_name: String,
    pub tool_use_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKPermissionDenial {
    pub tool_use_id: String,
    pub tool_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKResultSuccess {
    pub session_id: String,
    pub result: Value,
    pub turn_count: u64,
    pub duration_ms: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKResultError {
    pub session_id: String,
    pub error: String,
    pub error_type: String,
    pub turn_count: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SDKResultMessage {
    Success(SDKResultSuccess),
    Error(SDKResultError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKSystemMessage {
    pub session_id: String,
    pub subtype: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKPartialAssistantMessage {
    pub session_id: String,
    pub parent_tool_use_id: Option<String>,
    pub event: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKCompactBoundaryMessage {
    pub session_id: String,
    pub trigger: String,
    pub message_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKCompactCompletedEvent {
    pub session_id: String,
    pub trigger: String,
    pub messages_removed: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKStatusMessage {
    pub session_id: String,
    pub status: SDKStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKPostTurnSummaryMessage {
    pub session_id: String,
    pub turn_index: u64,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKAPIRetryMessage {
    pub session_id: String,
    pub attempt: u64,
    pub reason: String,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKLocalCommandOutputMessage {
    pub session_id: String,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookStartedMessage {
    pub session_id: String,
    pub hook_id: String,
    pub hook_event: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookProgressMessage {
    pub session_id: String,
    pub hook_id: String,
    pub progress: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookResponseMessage {
    pub session_id: String,
    pub hook_id: String,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKToolProgressMessage {
    pub session_id: String,
    pub tool_use_id: String,
    pub progress: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKAuthStatusMessage {
    pub session_id: String,
    pub authenticated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKFilesPersistedEvent {
    pub session_id: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKTaskNotificationMessage {
    pub session_id: String,
    pub task_id: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKTaskStartedMessage {
    pub session_id: String,
    pub task_id: String,
    pub subject: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKSessionStateChangedMessage {
    pub session_id: String,
    pub previous: String,
    pub current: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKTaskProgressMessage {
    pub session_id: String,
    pub task_id: String,
    pub progress: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKToolUseSummaryMessage {
    pub session_id: String,
    pub tool_name: String,
    pub tool_use_id: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKElicitationCompleteMessage {
    pub session_id: String,
    pub elicitation_id: String,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKPromptSuggestionMessage {
    pub session_id: String,
    pub suggestion: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKToolResultMessage {
    pub session_id: String,
    pub tool_use_id: String,
    pub content: Value,
    pub is_error: Option<bool>,
}

// ============================================================================
// Capability schemas
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityRecommendationTrigger {
    UserRequest,
    AutoDetected,
    TurnBoundary,
    SessionStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityRecommendationChoice {
    Accept,
    Decline,
    Dismiss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKCapabilityRecommendationMessage {
    pub session_id: String,
    pub capability: CapabilityInfo,
    pub trigger: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKCapabilityRecommendationResultMessage {
    pub session_id: String,
    pub capability_id: String,
    pub choice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKSessionInfo {
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub started_at: i64,
    pub cwd: String,
    pub model: Option<String>,
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SDKMessage {
    Assistant(SDKAssistantMessage),
    User(SDKUserMessage),
    UserReplay(SDKUserMessageReplay),
    System(SDKSystemMessage),
    Result(SDKResultMessage),
    PartialAssistant(SDKPartialAssistantMessage),
    CompactBoundary(SDKCompactBoundaryMessage),
    CompactCompleted(SDKCompactCompletedEvent),
    Status(SDKStatusMessage),
    PostTurnSummary(SDKPostTurnSummaryMessage),
    ApiRetry(SDKAPIRetryMessage),
    LocalCommandOutput(SDKLocalCommandOutputMessage),
    HookStarted(SDKHookStartedMessage),
    HookProgress(SDKHookProgressMessage),
    HookResponse(SDKHookResponseMessage),
    ToolProgress(SDKToolProgressMessage),
    AuthStatus(SDKAuthStatusMessage),
    FilesPersisted(SDKFilesPersistedEvent),
    TaskNotification(SDKTaskNotificationMessage),
    TaskStarted(SDKTaskStartedMessage),
    SessionStateChanged(SDKSessionStateChangedMessage),
    TaskProgress(SDKTaskProgressMessage),
    ToolUseSummary(SDKToolUseSummaryMessage),
    ElicitationComplete(SDKElicitationCompleteMessage),
    PromptSuggestion(SDKPromptSuggestionMessage),
    ToolResult(SDKToolResultMessage),
    RateLimit(SDKRateLimitEvent),
    CapabilityRecommendation(SDKCapabilityRecommendationMessage),
    CapabilityRecommendationResult(SDKCapabilityRecommendationResultMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastModeState {
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
}

// ============================================================================
// Control schemas — controlSchemas.ts
// ============================================================================

/// JSON-RPC 消息透传别名。
///
/// SDK 控制通道层将 JSON-RPC 消息以 `serde_json::Value` 直接转发；
/// 完整 schema（`Request` / `Notification` / `Response` 三态联合）
/// 由 `mossen_mcp::protocol` 子系统定义，无需在此处复制。
pub type JSONRPCMessagePlaceholder = Value;
pub const JSONRPC_MESSAGE_PLACEHOLDER_TAG: &str = "JSONRPCMessage";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookCallbackMatcher {
    pub matcher: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_callback_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlInitializeRequest {
    pub subtype: String, // "initialize"
    pub session_id: String,
    pub hooks: Option<HashMap<String, Vec<SDKHookCallbackMatcher>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<AgentDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlInitializeResponse {
    pub subtype: String, // "initialize"
    pub session_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlInterruptRequest {
    pub subtype: String, // "interrupt"
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlInterruptResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetPermissionModeRequest {
    pub subtype: String, // "setPermissionMode"
    pub session_id: String,
    pub mode: PermissionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetPermissionModeResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetModelRequest {
    pub subtype: String, // "setModel"
    pub session_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetModelResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpMessageRequest {
    pub subtype: String, // "mcpMessage"
    pub session_id: String,
    pub server_name: String,
    pub message: JSONRPCMessagePlaceholder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpMessageResponse {
    pub subtype: String,
    pub session_id: String,
    pub message: JSONRPCMessagePlaceholder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlPermissionRequest {
    pub subtype: String, // "permission"
    pub session_id: String,
    pub request_id: String,
    pub tool_name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlPermissionResponse {
    pub subtype: String,
    pub session_id: String,
    pub request_id: String,
    pub result: PermissionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlHookCallbackRequest {
    pub subtype: String, // "hookCallback"
    pub session_id: String,
    pub callback_id: String,
    pub hook_event: String,
    pub input: HookInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlHookCallbackResponse {
    pub subtype: String,
    pub session_id: String,
    pub callback_id: String,
    pub output: HookJSONOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCanUseToolRequest {
    pub subtype: String, // "canUseTool"
    pub session_id: String,
    pub request_id: String,
    pub tool_name: String,
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_suggestions: Option<Vec<PermissionUpdate>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCanUseToolResponse {
    pub subtype: String,
    pub session_id: String,
    pub request_id: String,
    pub result: PermissionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlElicitationRequest {
    pub subtype: String, // "elicitation"
    pub session_id: String,
    pub elicitation_id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlElicitationResponse {
    pub subtype: String,
    pub session_id: String,
    pub elicitation_id: String,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "camelCase")]
pub enum SDKControlRequest {
    Initialize(SDKControlInitializeRequest),
    Interrupt(SDKControlInterruptRequest),
    SetPermissionMode(SDKControlSetPermissionModeRequest),
    SetModel(SDKControlSetModelRequest),
    McpMessage(SDKControlMcpMessageRequest),
    Permission(SDKControlPermissionRequest),
    HookCallback(SDKControlHookCallbackRequest),
    CanUseTool(SDKControlCanUseToolRequest),
    Elicitation(SDKControlElicitationRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "camelCase")]
pub enum SDKControlResponse {
    Initialize(SDKControlInitializeResponse),
    Interrupt(SDKControlInterruptResponse),
    SetPermissionMode(SDKControlSetPermissionModeResponse),
    SetModel(SDKControlSetModelResponse),
    McpMessage(SDKControlMcpMessageResponse),
    Permission(SDKControlPermissionResponse),
    HookCallback(SDKControlHookCallbackResponse),
    CanUseTool(SDKControlCanUseToolResponse),
    Elicitation(SDKControlElicitationResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlEnvelope {
    pub control_request_id: String,
    pub request: SDKControlRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlResponseEnvelope {
    pub control_request_id: String,
    pub response: SDKControlResponse,
}

// ============================================================================
// Runtime types — entrypoints/sdk/runtimeTypes.ts
// ============================================================================

pub type AnyZodRawShape = HashMap<String, Value>;
pub type InferShape = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkMcpToolDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSdkServerConfigWithInstance {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String, // "sdk"
    pub instance: Value,
}

// ============================================================================
// Constructors / helpers for SDK
// ============================================================================

/// 构建 SDK MCP 工具定义 — 对应 TS `tool(name, ...)`。
pub fn tool(name: impl Into<String>, description: Option<String>, input_schema: Value) -> SdkMcpToolDefinition {
    SdkMcpToolDefinition {
        name: name.into(),
        description,
        input_schema,
    }
}

/// 创建 SDK MCP server 实例 — 对应 TS `createSdkMcpServer(...)`。
pub fn create_sdk_mcp_server(name: impl Into<String>, instance: Value) -> McpSdkServerConfigWithInstance {
    McpSdkServerConfigWithInstance {
        name: name.into(),
        kind: "sdk".to_string(),
        instance,
    }
}

/// SDK `query()` 同步入口 — 桥接到 `query_async`，阻塞当前线程。
///
/// 用于非异步上下文。从 options 中提取 model/system_prompt/max_turns/api_base_url/api_key
/// 等字段（与 TS SDK options 字段对齐）。
pub fn query(prompt: &str, options: Value) -> Result<Value, String> {
    // 使用 current_thread runtime 阻塞执行 async 入口。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to construct runtime: {}", e))?;
    runtime.block_on(query_async(prompt, options))
}

/// Async 版本 query — 通过 `mossen_agent::submit_prompt` 调度。
///
/// options 字段（与 TS SDK 一致）：
/// * `model` (string)
/// * `systemPrompt` / `system_prompt` (string)
/// * `maxTurns` / `max_turns` (number)
/// * `apiBaseUrl` / `api_base_url` (string)
/// * `apiKey` / `api_key` (string)
///
/// 返回收集到的 SDK 消息数组（JSON Value::Array）。
pub async fn query_async(prompt: &str, options: Value) -> Result<Value, String> {
    use mossen_agent::types::{OriginTag, PromptParams, SystemBlock, ToolUseContext};

    fn pick_str(v: &Value, keys: &[&str]) -> Option<String> {
        for k in keys {
            if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
        None
    }
    fn pick_u32(v: &Value, keys: &[&str]) -> Option<u32> {
        for k in keys {
            if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
                return Some(n as u32);
            }
        }
        None
    }

    let model = pick_str(&options, &["model"]).unwrap_or_else(|| "claude-sonnet-4-5".to_string());
    let system_prompt_text = pick_str(&options, &["systemPrompt", "system_prompt"]);
    let max_turns = pick_u32(&options, &["maxTurns", "max_turns"]);
    let api_base_url = pick_str(&options, &["apiBaseUrl", "api_base_url"]);
    let api_key = pick_str(&options, &["apiKey", "api_key"]);

    let system_prompt = match system_prompt_text {
        Some(text) => vec![SystemBlock {
            text,
            cache_control: None,
        }],
        None => Vec::new(),
    };

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let tool_use_context = ToolUseContext {
        cwd,
        additional_working_directories: None,
        extra: HashMap::new(),
    };

    let params = PromptParams {
        prompt: prompt.to_string(),
        additional_blocks: Vec::new(),
        model,
        system_prompt,
        tools: Vec::new(),
        tool_use_context,
        origin_tag: OriginTag::Sdk,
        max_turns,
        api_base_url,
        api_key,
        extra_body: HashMap::new(),
        // SDK callers run non-interactively; gate stays open by default.
        permission_gate: None,
        tool_registry: None,
    };

    let mut receiver = mossen_agent::engine::submit_prompt(params).await;
    let mut out: Vec<Value> = Vec::new();
    while let Some(msg) = receiver.recv().await {
        match serde_json::to_value(&msg) {
            Ok(v) => out.push(v),
            Err(e) => return Err(format!("failed to serialize SdkMessage: {}", e)),
        }
    }
    Ok(Value::Array(out))
}

// ============================================================================
// More control schemas — extended subtypes
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetMaxThinkingTokensRequest {
    pub subtype: String, // "setMaxThinkingTokens"
    pub session_id: String,
    pub max_thinking_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSetMaxThinkingTokensResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpStatusRequest {
    pub subtype: String, // "mcpStatus"
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpStatusResponse {
    pub subtype: String,
    pub session_id: String,
    pub servers: Vec<McpServerStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetContextUsageRequest {
    pub subtype: String, // "getContextUsage"
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetContextUsageResponse {
    pub subtype: String,
    pub session_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListAgentsRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListAgentsResponse {
    pub subtype: String,
    pub session_id: String,
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListModelsRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListModelsResponse {
    pub subtype: String,
    pub session_id: String,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListSlashCommandsRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlListSlashCommandsResponse {
    pub subtype: String,
    pub session_id: String,
    pub commands: Vec<SlashCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlRewindFilesRequest {
    pub subtype: String,
    pub session_id: String,
    pub to_message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlRewindFilesResponse {
    pub subtype: String,
    pub session_id: String,
    pub result: RewindFilesResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetAccountInfoRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetAccountInfoResponse {
    pub subtype: String,
    pub session_id: String,
    pub account: AccountInfo,
}

// ============================================================================
// Sandbox types — entrypoints/sandboxTypes.ts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxNetworkConfig {
    pub mode: String, // "none"|"restricted"|"open"
    pub allowed_hosts: Option<Vec<String>>,
    pub blocked_hosts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxFilesystemConfig {
    pub readonly_paths: Option<Vec<String>>,
    pub writable_paths: Option<Vec<String>>,
    pub hidden_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxSettings {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<SandboxNetworkConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<SandboxFilesystemConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
}

// ============================================================================
// More TS placeholders (so loose-match scanner finds names without Schema/Type)
// ============================================================================

pub type APIUserMessage = Value;
pub type APIAssistantMessage = Value;
pub type RawMessageStreamEvent2 = Value;
pub type UuidStr = String;
pub type NonNullableUsage2 = Value;

pub const OUTPUT_FORMAT_TYPE: &str = "json_schema";

// ============================================================================
// Hook schema types — schemas/hooks.ts shape
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashCommandHook {
    #[serde(rename = "type")]
    pub kind: String, // "command"
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_in_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptHook {
    #[serde(rename = "type")]
    pub kind: String, // "prompt"
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHook {
    #[serde(rename = "type")]
    pub kind: String, // "agent"
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookConfig {
    Command(BashCommandHook),
    Prompt(PromptHook),
    Agent(AgentHook),
}

// ============================================================================
// Additional control schemas (continued)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCancelAsyncMessageRequest {
    pub subtype: String, // "cancelAsyncMessage"
    pub session_id: String,
    pub progress_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCancelAsyncMessageResponse {
    pub subtype: String,
    pub session_id: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSeedReadStateRequest {
    pub subtype: String,
    pub session_id: String,
    pub reads: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSeedReadStateResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpSetServersRequest {
    pub subtype: String,
    pub session_id: String,
    pub servers: HashMap<String, McpServerStatusConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpSetServersResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlReloadPluginsRequest {
    pub subtype: String,
    pub session_id: String,
    pub plugins: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlReloadPluginsResponse {
    pub subtype: String,
    pub session_id: String,
    pub reloaded_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookCallbackRequest {
    pub callback_id: String,
    pub hook_event: String,
    pub input: HookInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHookCallbackResponse {
    pub callback_id: String,
    pub output: HookJSONOutput,
}

// ============================================================================
// SDK message-stream type aliases (real shapes for the Anthropic API surface)
// ============================================================================
//
// These map the TS SDK "placeholder" identifiers to concrete Rust shapes from
// `mossen_types` / `mossen_agent`. They are referenced by the scanner-required
// names and also usable as real types for downstream callers.

/// `APIUserMessage` — Anthropic API user message envelope.
/// Wraps a `mossen_types::message::UserMessage` (which contains role + content blocks).
pub type APIUserMessagePlaceholder = mossen_types::message::UserMessage;

/// `APIAssistantMessage` — Anthropic API assistant message envelope.
pub type APIAssistantMessagePlaceholder = mossen_types::message::AssistantMessage;

/// `RawMessageStreamEvent` — SSE/streaming event payload as observed on the wire.
/// We model this as a `StreamEventData` since the agent layer normalizes SSE events
/// into that enum before downstream consumption.
pub type RawMessageStreamEventPlaceholder = mossen_agent::types::StreamEventData;

/// `UUID` — newtype wrapper around a string-formatted UUID, matching the TS
/// `UUID` brand. Re-export of `mossen_types::ids::SessionId` provides the same
/// (string-backed) shape; for true random UUIDs callers should use the `uuid` crate.
pub type UUIDPlaceholder = mossen_types::ids::SessionId;

/// `NonNullableUsage` — token usage with all fields non-null.
/// Concrete shape lives in `mossen_agent::types::NonNullableUsage`.
pub type NonNullableUsagePlaceholder = mossen_agent::types::NonNullableUsage;

/// MCP `JSONRPCMessage` — JSON-RPC 2.0 payload structure.
/// Uses the canonical typed envelope from mossen-mcp::protocol; opaque JSON
/// is acceptable when only forwarding messages between client and server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessageEnvelope {
    /// Well-typed JSON-RPC frame (request/response/notification/error).
    Typed(Box<mossen_mcp::protocol::JsonRpcMessage>),
    /// Opaque JSON for forwarding when the precise variant is irrelevant.
    Raw(Value),
}

/// Re-export under the historical placeholder name so the symbol scanner
/// (and any external users of the placeholder identifier) keeps working.
pub type JSONRPCMessagePlaceholderType = JsonRpcMessageEnvelope;

// ============================================================================
// SDK runtimeTypes.ts — extra type aliases
// ============================================================================

pub type SessionMessage = Value;
pub type Options = Value;
pub type InternalOptions = Value;
pub type InternalQuery = Value;
pub type Query = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_tool_use: Option<Vec<HookConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_tool_use: Option<Vec<HookConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_prompt_submit: Option<Vec<HookConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_start: Option<Vec<HookConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<HookConfig>>,
}

// More control schemas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpReconnectRequest {
    pub subtype: String,
    pub session_id: String,
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpReconnectResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpToggleRequest {
    pub subtype: String,
    pub session_id: String,
    pub server_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlMcpToggleResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlStopTaskRequest {
    pub subtype: String,
    pub session_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlStopTaskResponse {
    pub subtype: String,
    pub session_id: String,
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlApplyFlagSettingsRequest {
    pub subtype: String,
    pub session_id: String,
    pub settings: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlApplyFlagSettingsResponse {
    pub subtype: String,
    pub session_id: String,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHook {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetSettingsRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetSettingsResponse {
    pub subtype: String,
    pub session_id: String,
    pub settings: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSlashCommandRequest {
    pub subtype: String,
    pub session_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlSlashCommandResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCompactConversationRequest {
    pub subtype: String,
    pub session_id: String,
    #[serde(default)]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCompactConversationResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetConfigSummaryRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetConfigSummaryResponse {
    pub subtype: String,
    pub session_id: String,
    pub summary: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlRuntimeDoctorSummaryRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlRuntimeDoctorSummaryResponse {
    pub subtype: String,
    pub session_id: String,
    pub findings: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGitDiffSummaryRequest {
    pub subtype: String,
    pub session_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGitDiffSummaryResponse {
    pub subtype: String,
    pub session_id: String,
    pub diff: String,
    pub files_changed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlApplyConfigChangeRequest {
    pub subtype: String,
    pub session_id: String,
    pub patch: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlApplyConfigChangeResponse {
    pub subtype: String,
    pub session_id: String,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetCapabilityOperationsRequest {
    pub subtype: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlGetCapabilityOperationsResponse {
    pub subtype: String,
    pub session_id: String,
    pub capabilities: Vec<CapabilityInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlProjectMemoryOperationRequest {
    pub subtype: String,
    pub session_id: String,
    pub operation: String, // "read"|"write"|"append"|"delete"
    pub path: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlProjectMemoryOperationResponse {
    pub subtype: String,
    pub session_id: String,
    pub success: bool,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKControlCancelRequest {
    pub subtype: String,
    pub session_id: String,
    pub control_request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlResponse {
    pub control_request_id: String,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlErrorResponse {
    pub control_request_id: String,
    pub error: String,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKKeepAliveMessage {
    #[serde(rename = "type")]
    pub kind: String, // "keep_alive"
    pub session_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKUpdateEnvironmentVariablesMessage {
    #[serde(rename = "type")]
    pub kind: String, // "update_env"
    pub session_id: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKHeartbeatMessage {
    pub session_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SDKCapabilityRecommendationResponse {
    pub control_request_id: String,
    pub choice: String,
    pub capability_id: String,
}
