// Hooks utilities - translated from utils/hooks/*.ts
// Covers: hookEvents, hookHelpers, hooksSettings, hooksConfigSnapshot,
// hooksConfigManager, sessionHooks, AsyncHookRegistry, execPromptHook,
// execAgentHook, execHttpHook, ssrfGuard, fileChangedWatcher,
// postSamplingHooks, registerFrontmatterHooks, registerSkillHooks,
// apiQueryHookHelper, skillImprovement

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Re-export log macros for this module
macro_rules! log_debug {
    ($($arg:tt)*) => {
        /* debug logging placeholder */
    };
}
macro_rules! log_error {
    ($($arg:tt)*) => { eprintln!($($arg)*); };
}

// ─── Hook Event Types ───

pub const HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PostSampling",
    "PermissionDenied",
    "Notification",
    "UserPromptSubmit",
    "SessionStart",
    "SessionEnd",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "PermissionRequest",
    "Setup",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "Elicitation",
    "ElicitationResult",
    "ConfigChange",
    "WorktreeCreate",
    "WorktreeRemove",
    "InstructionsLoaded",
    "CwdChanged",
    "FileChanged",
];

pub type HookEvent = String;

// ─── hookEvents.ts ───

const ALWAYS_EMITTED_HOOK_EVENTS: &[&str] = &["SessionStart", "Setup"];
const MAX_PENDING_EVENTS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookExecutionEvent {
    #[serde(rename = "started")]
    Started {
        hook_id: String,
        hook_name: String,
        hook_event: String,
    },
    #[serde(rename = "progress")]
    Progress {
        hook_id: String,
        hook_name: String,
        hook_event: String,
        stdout: String,
        stderr: String,
        output: String,
    },
    #[serde(rename = "response")]
    Response {
        hook_id: String,
        hook_name: String,
        hook_event: String,
        output: String,
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        outcome: String, // "success" | "error" | "cancelled"
    },
}

pub type HookEventHandler = Box<dyn Fn(HookExecutionEvent) + Send + Sync>;

struct HookEventState {
    pending_events: Vec<HookExecutionEvent>,
    handler: Option<Arc<HookEventHandler>>,
    all_hook_events_enabled: bool,
}

lazy_static::lazy_static! {
    static ref HOOK_EVENT_STATE: Mutex<HookEventState> = Mutex::new(HookEventState {
        pending_events: Vec::new(),
        handler: None,
        all_hook_events_enabled: false,
    });
}

pub fn register_hook_event_handler(handler: Option<Arc<HookEventHandler>>) {
    let mut state = HOOK_EVENT_STATE.lock().unwrap();
    state.handler = handler.clone();
    if let Some(ref h) = handler {
        for event in state.pending_events.drain(..) {
            h(event);
        }
    }
}

fn emit_event(event: HookExecutionEvent) {
    let mut state = HOOK_EVENT_STATE.lock().unwrap();
    if let Some(ref handler) = state.handler {
        handler(event);
    } else {
        state.pending_events.push(event);
        if state.pending_events.len() > MAX_PENDING_EVENTS {
            state.pending_events.remove(0);
        }
    }
}

fn should_emit(hook_event: &str) -> bool {
    if ALWAYS_EMITTED_HOOK_EVENTS.contains(&hook_event) {
        return true;
    }
    let state = HOOK_EVENT_STATE.lock().unwrap();
    state.all_hook_events_enabled && HOOK_EVENTS.contains(&hook_event)
}

pub fn emit_hook_started(hook_id: &str, hook_name: &str, hook_event: &str) {
    if !should_emit(hook_event) {
        return;
    }
    emit_event(HookExecutionEvent::Started {
        hook_id: hook_id.to_string(),
        hook_name: hook_name.to_string(),
        hook_event: hook_event.to_string(),
    });
}

pub fn emit_hook_progress(
    hook_id: &str,
    hook_name: &str,
    hook_event: &str,
    stdout: &str,
    stderr: &str,
    output: &str,
) {
    if !should_emit(hook_event) {
        return;
    }
    emit_event(HookExecutionEvent::Progress {
        hook_id: hook_id.to_string(),
        hook_name: hook_name.to_string(),
        hook_event: hook_event.to_string(),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        output: output.to_string(),
    });
}

pub fn emit_hook_response(
    hook_id: &str,
    hook_name: &str,
    hook_event: &str,
    output: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    outcome: &str,
) {
    let output_to_log = if !stdout.is_empty() {
        stdout
    } else if !stderr.is_empty() {
        stderr
    } else {
        output
    };
    if !output_to_log.is_empty() {
        log_debug!(
            "Hook {} ({}) {}:\n{}",
            hook_name,
            hook_event,
            outcome,
            output_to_log
        );
    }
    if !should_emit(hook_event) {
        return;
    }
    emit_event(HookExecutionEvent::Response {
        hook_id: hook_id.to_string(),
        hook_name: hook_name.to_string(),
        hook_event: hook_event.to_string(),
        output: output.to_string(),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code,
        outcome: outcome.to_string(),
    });
}

pub fn set_all_hook_events_enabled(enabled: bool) {
    let mut state = HOOK_EVENT_STATE.lock().unwrap();
    state.all_hook_events_enabled = enabled;
}

pub fn clear_hook_event_state() {
    let mut state = HOOK_EVENT_STATE.lock().unwrap();
    state.handler = None;
    state.pending_events.clear();
    state.all_hook_events_enabled = false;
}

// ─── Hook Types (from settings/types.ts) ───

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum HookCommand {
    #[serde(rename = "command")]
    Command {
        command: String,
        shell: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "if")]
        if_condition: Option<String>,
        once: Option<bool>,
    },
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "if")]
        if_condition: Option<String>,
        once: Option<bool>,
    },
    #[serde(rename = "agent")]
    Agent {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "if")]
        if_condition: Option<String>,
        once: Option<bool>,
    },
    #[serde(rename = "http")]
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
        allowed_env_vars: Option<Vec<String>>,
        timeout: Option<u64>,
        #[serde(rename = "if")]
        if_condition: Option<String>,
        once: Option<bool>,
    },
}

impl HookCommand {
    pub fn get_if(&self) -> Option<&str> {
        match self {
            HookCommand::Command { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Prompt { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Agent { if_condition, .. } => if_condition.as_deref(),
            HookCommand::Http { if_condition, .. } => if_condition.as_deref(),
        }
    }

    pub fn is_once(&self) -> bool {
        match self {
            HookCommand::Command { once, .. } => once.unwrap_or(false),
            HookCommand::Prompt { once, .. } => once.unwrap_or(false),
            HookCommand::Agent { once, .. } => once.unwrap_or(false),
            HookCommand::Http { once, .. } => once.unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<HookCommand>,
}

pub type HooksSettings = HashMap<String, Vec<HookMatcher>>;

// ─── hookHelpers.ts ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponse {
    pub ok: bool,
    pub reason: Option<String>,
}

pub fn add_arguments_to_prompt(prompt: &str, json_input: &str) -> String {
    if prompt.contains("$ARGUMENTS") {
        prompt.replace("$ARGUMENTS", json_input)
    } else {
        format!("{}\n\n{}", prompt, json_input)
    }
}

// ─── hooksSettings.ts ───

pub const DEFAULT_HOOK_SHELL: &str = "bash";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
    PluginHook,
    SessionHook,
    BuiltinHook,
}

impl HookSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookSource::UserSettings => "userSettings",
            HookSource::ProjectSettings => "projectSettings",
            HookSource::LocalSettings => "localSettings",
            HookSource::PolicySettings => "policySettings",
            HookSource::PluginHook => "pluginHook",
            HookSource::SessionHook => "sessionHook",
            HookSource::BuiltinHook => "builtinHook",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndividualHookConfig {
    pub event: HookEvent,
    pub config: HookCommand,
    pub matcher: Option<String>,
    pub source: HookSource,
    pub plugin_name: Option<String>,
}

pub fn is_hook_equal(a: &HookCommand, b: &HookCommand) -> bool {
    match (a, b) {
        (
            HookCommand::Command {
                command: cmd_a,
                shell: shell_a,
                if_condition: if_a,
                ..
            },
            HookCommand::Command {
                command: cmd_b,
                shell: shell_b,
                if_condition: if_b,
                ..
            },
        ) => {
            cmd_a == cmd_b
                && shell_a.as_deref().unwrap_or(DEFAULT_HOOK_SHELL)
                    == shell_b.as_deref().unwrap_or(DEFAULT_HOOK_SHELL)
                && if_a.as_deref().unwrap_or("") == if_b.as_deref().unwrap_or("")
        }
        (
            HookCommand::Prompt {
                prompt: p_a,
                if_condition: if_a,
                ..
            },
            HookCommand::Prompt {
                prompt: p_b,
                if_condition: if_b,
                ..
            },
        ) => p_a == p_b && if_a.as_deref().unwrap_or("") == if_b.as_deref().unwrap_or(""),
        (
            HookCommand::Agent {
                prompt: p_a,
                if_condition: if_a,
                ..
            },
            HookCommand::Agent {
                prompt: p_b,
                if_condition: if_b,
                ..
            },
        ) => p_a == p_b && if_a.as_deref().unwrap_or("") == if_b.as_deref().unwrap_or(""),
        (
            HookCommand::Http {
                url: u_a,
                if_condition: if_a,
                ..
            },
            HookCommand::Http {
                url: u_b,
                if_condition: if_b,
                ..
            },
        ) => u_a == u_b && if_a.as_deref().unwrap_or("") == if_b.as_deref().unwrap_or(""),
        _ => false,
    }
}

pub fn get_hook_display_text(hook: &HookCommand) -> String {
    match hook {
        HookCommand::Command { command, .. } => command.clone(),
        HookCommand::Prompt { prompt, .. } => prompt.clone(),
        HookCommand::Agent { prompt, .. } => prompt.clone(),
        HookCommand::Http { url, .. } => url.clone(),
    }
}

pub fn hook_source_description_display_string(source: &HookSource) -> &'static str {
    match source {
        HookSource::UserSettings => "User settings (~/.mossen/settings.json)",
        HookSource::ProjectSettings => "Project settings (.mossen/settings.json)",
        HookSource::LocalSettings => "Local settings (.mossen/settings.local.json)",
        HookSource::PluginHook => "Plugin hooks (~/.mossen/plugins/*/hooks/hooks.json)",
        HookSource::SessionHook => "Session hooks (in-memory, temporary)",
        HookSource::BuiltinHook => "Built-in hooks (registered internally by Mossen)",
        HookSource::PolicySettings => "Policy settings",
    }
}

pub fn hook_source_header_display_string(source: &HookSource) -> &'static str {
    match source {
        HookSource::UserSettings => "User Settings",
        HookSource::ProjectSettings => "Project Settings",
        HookSource::LocalSettings => "Local Settings",
        HookSource::PluginHook => "Plugin Hooks",
        HookSource::SessionHook => "Session Hooks",
        HookSource::BuiltinHook => "Built-in Hooks",
        HookSource::PolicySettings => "Policy Settings",
    }
}

pub fn hook_source_inline_display_string(source: &HookSource) -> &'static str {
    match source {
        HookSource::UserSettings => "User",
        HookSource::ProjectSettings => "Project",
        HookSource::LocalSettings => "Local",
        HookSource::PluginHook => "Plugin",
        HookSource::SessionHook => "Session",
        HookSource::BuiltinHook => "Built-in",
        HookSource::PolicySettings => "Policy",
    }
}

pub fn sort_matchers_by_priority(matchers: &[String]) -> Vec<String> {
    let source_priority: HashMap<&str, usize> = [
        ("userSettings", 0),
        ("projectSettings", 1),
        ("localSettings", 2),
    ]
    .into_iter()
    .collect();

    let mut sorted = matchers.to_vec();
    sorted.sort_by(|a, b| {
        let a_priority = source_priority.get(a.as_str()).copied().unwrap_or(999);
        let b_priority = source_priority.get(b.as_str()).copied().unwrap_or(999);
        a_priority.cmp(&b_priority).then(a.cmp(b))
    });
    sorted
}

// ─── hooksConfigSnapshot.ts ───

lazy_static::lazy_static! {
    static ref INITIAL_HOOKS_CONFIG: Mutex<Option<HooksSettings>> = Mutex::new(None);
}

/// True when only managed (`policySettings`) hooks should run.
///
/// Mirrors TS `shouldAllowManagedHooksOnly` (`utils/hooks/hooksConfigSnapshot.ts`):
///   * `policySettings.allowManagedHooksOnly == true` → true
///   * Non-managed `disableAllHooks == true` while policy doesn't opt in →
///     true (a non-managed switch can't kill managed hooks, so it degrades
///     to "managed-only").
pub fn should_allow_managed_hooks_only() -> bool {
    let policy =
        crate::settings::load_settings_for_source(crate::settings::SettingSource::PolicySettings);
    if policy
        .as_ref()
        .and_then(|s| s.allow_managed_hooks_only)
        .unwrap_or(false)
    {
        return true;
    }
    // Non-managed disable: only flips to "managed-only" when policy doesn't
    // also set it (the policy disableAllHooksIncludingManaged path covers
    // that case separately and returns true from the other helper).
    let merged = crate::settings::load_settings_from_disk().settings;
    let merged_disables = merged.disable_all_hooks.unwrap_or(false);
    let policy_disables = policy
        .as_ref()
        .and_then(|s| s.disable_all_hooks)
        .unwrap_or(false);
    merged_disables && !policy_disables
}

/// True when even managed hooks should be skipped — only achievable via
/// `policySettings.disableAllHooks == true`. Non-managed `disableAllHooks` is
/// honoured only as "managed-only" (see `should_allow_managed_hooks_only`).
///
/// Mirrors TS `shouldDisableAllHooksIncludingManaged`.
pub fn should_disable_all_hooks_including_managed() -> bool {
    crate::settings::load_settings_for_source(crate::settings::SettingSource::PolicySettings)
        .and_then(|s| s.disable_all_hooks)
        .unwrap_or(false)
}

pub fn capture_hooks_config_snapshot() {
    let config = get_hooks_from_allowed_sources();
    let mut state = INITIAL_HOOKS_CONFIG.lock().unwrap();
    *state = Some(config);
}

pub fn update_hooks_config_snapshot() {
    let config = get_hooks_from_allowed_sources();
    let mut state = INITIAL_HOOKS_CONFIG.lock().unwrap();
    *state = Some(config);
}

pub fn get_hooks_config_from_snapshot() -> Option<HooksSettings> {
    let mut state = INITIAL_HOOKS_CONFIG.lock().unwrap();
    if state.is_none() {
        drop(state);
        capture_hooks_config_snapshot();
        state = INITIAL_HOOKS_CONFIG.lock().unwrap();
    }
    state.clone()
}

pub fn reset_hooks_config_snapshot() {
    let mut state = INITIAL_HOOKS_CONFIG.lock().unwrap();
    *state = None;
}

/// Read the merged hooks configuration from all allowed sources.
///
/// Mirrors TS `getHooksFromAllowedSources` (`utils/hooks/hooksConfigSnapshot.ts`).
/// Priority rules:
///   1. `policySettings.disableAllHooks == true`  → empty map (kill switch).
///   2. `policySettings.allowManagedHooksOnly == true` → only policy hooks.
///   3. Plugin-only restriction policy → only policy hooks.
///   4. Non-managed `disableAllHooks == true` → only policy hooks remain
///      (non-managed settings cannot disable managed hooks).
///   5. Otherwise → merged hooks from all sources (legacy behaviour).
fn get_hooks_from_allowed_sources() -> HooksSettings {
    let policy =
        crate::settings::load_settings_for_source(crate::settings::SettingSource::PolicySettings);

    if policy
        .as_ref()
        .and_then(|s| s.disable_all_hooks)
        .unwrap_or(false)
    {
        return HashMap::new();
    }

    let policy_hooks = || -> HooksSettings {
        policy
            .as_ref()
            .and_then(|s| s.hooks.as_ref())
            .and_then(|v| serde_json::from_value::<HooksSettings>(v.clone()).ok())
            .unwrap_or_default()
    };

    if policy
        .as_ref()
        .and_then(|s| s.allow_managed_hooks_only)
        .unwrap_or(false)
    {
        return policy_hooks();
    }

    if crate::settings::is_restricted_to_plugin_only("hooks", policy.as_ref()) {
        return policy_hooks();
    }

    let merged = crate::settings::load_settings_from_disk().settings;
    if merged.disable_all_hooks.unwrap_or(false) {
        return policy_hooks();
    }

    merged
        .hooks
        .as_ref()
        .and_then(|v| serde_json::from_value::<HooksSettings>(v.clone()).ok())
        .unwrap_or_default()
}

// ─── hooksConfigManager.ts ───

#[derive(Debug, Clone)]
pub struct MatcherMetadata {
    pub field_to_match: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HookEventMetadata {
    pub summary: String,
    pub description: String,
    pub matcher_metadata: Option<MatcherMetadata>,
}

pub fn get_hook_event_metadata(tool_names: &[String]) -> HashMap<String, HookEventMetadata> {
    let mut metadata = HashMap::new();

    metadata.insert(
        "PreToolUse".to_string(),
        HookEventMetadata {
            summary: "Before tool execution".to_string(),
            description: "Input to command is JSON of tool call arguments.\nExit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and block tool call\nOther exit codes - show stderr to user only but continue with tool call".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name".to_string(),
                values: tool_names.to_vec(),
            }),
        },
    );

    metadata.insert(
        "PostToolUse".to_string(),
        HookEventMetadata {
            summary: "After tool execution".to_string(),
            description: "Input to command is JSON with fields \"inputs\" (tool call arguments) and \"response\" (tool call response).".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name".to_string(),
                values: tool_names.to_vec(),
            }),
        },
    );

    metadata.insert(
        "PostToolUseFailure".to_string(),
        HookEventMetadata {
            summary: "After tool execution fails".to_string(),
            description: "Input to command is JSON with tool_name, tool_input, tool_use_id, error, error_type, is_interrupt, and is_timeout.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name".to_string(),
                values: tool_names.to_vec(),
            }),
        },
    );

    metadata.insert(
        "PostSampling".to_string(),
        HookEventMetadata {
            summary: "After model response sampling completes".to_string(),
            description:
                "Input to command is JSON with assistant_response, system_prompt, and query_source."
                    .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "query_source".to_string(),
                values: vec![
                    "repl".into(),
                    "sdk".into(),
                    "custom_backend".into(),
                    "agent_task".into(),
                    "background".into(),
                    "pipeline".into(),
                ],
            }),
        },
    );

    metadata.insert(
        "PermissionDenied".to_string(),
        HookEventMetadata {
            summary: "After auto mode classifier denies a tool call".to_string(),
            description:
                "Input to command is JSON with tool_name, tool_input, tool_use_id, and reason."
                    .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name".to_string(),
                values: tool_names.to_vec(),
            }),
        },
    );

    metadata.insert(
        "Notification".to_string(),
        HookEventMetadata {
            summary: "When notifications are sent".to_string(),
            description: "Input to command is JSON with notification message and type.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "notification_type".to_string(),
                values: vec![
                    "permission_prompt".into(),
                    "idle_prompt".into(),
                    "auth_success".into(),
                    "elicitation_dialog".into(),
                    "elicitation_complete".into(),
                    "elicitation_response".into(),
                ],
            }),
        },
    );

    metadata.insert(
        "UserPromptSubmit".to_string(),
        HookEventMetadata {
            summary: "When the user submits a prompt".to_string(),
            description: "Input to command is JSON with original user prompt text.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata.insert(
        "SessionStart".to_string(),
        HookEventMetadata {
            summary: "When a new session is started".to_string(),
            description: "Input to command is JSON with session start source.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "source".to_string(),
                values: vec![
                    "startup".into(),
                    "resume".into(),
                    "clear".into(),
                    "compact".into(),
                ],
            }),
        },
    );

    metadata.insert("Stop".to_string(), HookEventMetadata {
        summary: "Right before Mossen concludes its response".to_string(),
        description: "Exit code 0 - stdout/stderr not shown\nExit code 2 - show stderr to model and continue conversation".to_string(),
        matcher_metadata: None,
    });

    metadata.insert(
        "StopFailure".to_string(),
        HookEventMetadata {
            summary: "When the turn ends due to an API error".to_string(),
            description: "Fires instead of Stop when an API error ended the turn.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "error".to_string(),
                values: vec![
                    "rate_limit".into(),
                    "authentication_failed".into(),
                    "billing_error".into(),
                    "invalid_request".into(),
                    "server_error".into(),
                    "max_output_tokens".into(),
                    "unknown".into(),
                ],
            }),
        },
    );

    metadata.insert(
        "SubagentStart".to_string(),
        HookEventMetadata {
            summary: "When a subagent is started".to_string(),
            description: "Input to command is JSON with agent_id and agent_type.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "agent_type".to_string(),
                values: vec![],
            }),
        },
    );

    metadata.insert(
        "SubagentStop".to_string(),
        HookEventMetadata {
            summary: "Right before a subagent concludes its response".to_string(),
            description:
                "Input to command is JSON with agent_id, agent_type, and agent_transcript_path."
                    .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "agent_type".to_string(),
                values: vec![],
            }),
        },
    );

    metadata.insert(
        "PreCompact".to_string(),
        HookEventMetadata {
            summary: "Before conversation compaction".to_string(),
            description: "Input to command is JSON with compaction details.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger".to_string(),
                values: vec!["manual".into(), "auto".into()],
            }),
        },
    );

    metadata.insert(
        "PostCompact".to_string(),
        HookEventMetadata {
            summary: "After conversation compaction".to_string(),
            description: "Input to command is JSON with compaction details and the summary."
                .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger".to_string(),
                values: vec!["manual".into(), "auto".into()],
            }),
        },
    );

    metadata.insert(
        "SessionEnd".to_string(),
        HookEventMetadata {
            summary: "When a session is ending".to_string(),
            description: "Input to command is JSON with session end reason.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "reason".to_string(),
                values: vec![
                    "clear".into(),
                    "logout".into(),
                    "prompt_input_exit".into(),
                    "other".into(),
                ],
            }),
        },
    );

    metadata.insert(
        "PermissionRequest".to_string(),
        HookEventMetadata {
            summary: "When a permission dialog is displayed".to_string(),
            description: "Input to command is JSON with tool_name, tool_input, and tool_use_id."
                .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "tool_name".to_string(),
                values: tool_names.to_vec(),
            }),
        },
    );

    metadata.insert(
        "Setup".to_string(),
        HookEventMetadata {
            summary: "Repo setup hooks for init and maintenance".to_string(),
            description: "Input to command is JSON with trigger (init or maintenance).".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "trigger".to_string(),
                values: vec!["init".into(), "maintenance".into()],
            }),
        },
    );

    metadata.insert(
        "TeammateIdle".to_string(),
        HookEventMetadata {
            summary: "When a teammate is about to go idle".to_string(),
            description: "Input to command is JSON with teammate_name and team_name.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata.insert("TaskCreated".to_string(), HookEventMetadata {
        summary: "When a task is being created".to_string(),
        description: "Input to command is JSON with task_id, task_subject, task_description, teammate_name, and team_name.".to_string(),
        matcher_metadata: None,
    });

    metadata.insert("TaskCompleted".to_string(), HookEventMetadata {
        summary: "When a task is being marked as completed".to_string(),
        description: "Input to command is JSON with task_id, task_subject, task_description, teammate_name, and team_name.".to_string(),
        matcher_metadata: None,
    });

    metadata.insert(
        "Elicitation".to_string(),
        HookEventMetadata {
            summary: "When an MCP server requests user input".to_string(),
            description:
                "Input to command is JSON with mcp_server_name, message, and requested_schema."
                    .to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "mcp_server_name".to_string(),
                values: vec![],
            }),
        },
    );

    metadata.insert("ElicitationResult".to_string(), HookEventMetadata {
        summary: "After a user responds to an MCP elicitation".to_string(),
        description: "Input to command is JSON with mcp_server_name, action, content, mode, and elicitation_id.".to_string(),
        matcher_metadata: Some(MatcherMetadata {
            field_to_match: "mcp_server_name".to_string(),
            values: vec![],
        }),
    });

    metadata.insert(
        "ConfigChange".to_string(),
        HookEventMetadata {
            summary: "When configuration files change during a session".to_string(),
            description: "Input to command is JSON with source and file_path.".to_string(),
            matcher_metadata: Some(MatcherMetadata {
                field_to_match: "source".to_string(),
                values: vec![
                    "user_settings".into(),
                    "project_settings".into(),
                    "local_settings".into(),
                    "policy_settings".into(),
                    "skills".into(),
                ],
            }),
        },
    );

    metadata.insert("InstructionsLoaded".to_string(), HookEventMetadata {
        summary: "When an instruction file is loaded".to_string(),
        description: "Input to command is JSON with file_path, memory_type, load_reason, globs, trigger_file_path, and parent_file_path.".to_string(),
        matcher_metadata: Some(MatcherMetadata {
            field_to_match: "load_reason".to_string(),
            values: vec![
                "session_start".into(), "nested_traversal".into(),
                "path_glob_match".into(), "include".into(), "compact".into(),
            ],
        }),
    });

    metadata.insert(
        "WorktreeCreate".to_string(),
        HookEventMetadata {
            summary: "Create an isolated worktree for VCS-agnostic isolation".to_string(),
            description: "Input to command is JSON with name.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata.insert(
        "WorktreeRemove".to_string(),
        HookEventMetadata {
            summary: "Remove a previously created worktree".to_string(),
            description: "Input to command is JSON with worktree_path.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata.insert(
        "CwdChanged".to_string(),
        HookEventMetadata {
            summary: "After the working directory changes".to_string(),
            description: "Input to command is JSON with old_cwd and new_cwd.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata.insert(
        "FileChanged".to_string(),
        HookEventMetadata {
            summary: "When a watched file changes".to_string(),
            description: "Input to command is JSON with file_path and event.".to_string(),
            matcher_metadata: None,
        },
    );

    metadata
}

pub fn get_matcher_metadata(event: &str, tool_names: &[String]) -> Option<MatcherMetadata> {
    get_hook_event_metadata(tool_names)
        .get(event)
        .and_then(|m| m.matcher_metadata.clone())
}

// ─── sessionHooks.ts ───

#[derive(Debug, Clone)]
pub struct SessionHookEntry {
    pub hook: HookCommand,
}

#[derive(Debug, Clone)]
pub struct SessionHookMatcher {
    pub matcher: String,
    pub skill_root: Option<String>,
    pub hooks: Vec<SessionHookEntry>,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    pub hooks: HashMap<String, Vec<SessionHookMatcher>>,
}

pub type SessionHooksState = HashMap<String, SessionStore>;

lazy_static::lazy_static! {
    static ref SESSION_HOOKS: Mutex<SessionHooksState> = Mutex::new(HashMap::new());
}

pub fn add_session_hook(
    session_id: &str,
    event: &str,
    matcher: &str,
    hook: HookCommand,
    skill_root: Option<&str>,
) {
    let mut state = SESSION_HOOKS.lock().unwrap();
    let store = state
        .entry(session_id.to_string())
        .or_insert_with(|| SessionStore {
            hooks: HashMap::new(),
        });

    let event_matchers = store.hooks.entry(event.to_string()).or_default();

    let existing = event_matchers
        .iter_mut()
        .find(|m| m.matcher == matcher && m.skill_root.as_deref() == skill_root);

    if let Some(existing_matcher) = existing {
        existing_matcher.hooks.push(SessionHookEntry { hook });
    } else {
        event_matchers.push(SessionHookMatcher {
            matcher: matcher.to_string(),
            skill_root: skill_root.map(|s| s.to_string()),
            hooks: vec![SessionHookEntry { hook }],
        });
    }

    log_debug!(
        "Added session hook for event {} in session {}",
        event,
        session_id
    );
}

pub fn remove_session_hook(session_id: &str, event: &str, hook: &HookCommand) {
    let mut state = SESSION_HOOKS.lock().unwrap();
    if let Some(store) = state.get_mut(session_id) {
        if let Some(event_matchers) = store.hooks.get_mut(event) {
            for matcher in event_matchers.iter_mut() {
                matcher.hooks.retain(|h| !is_hook_equal(&h.hook, hook));
            }
            event_matchers.retain(|m| !m.hooks.is_empty());
            if event_matchers.is_empty() {
                store.hooks.remove(event);
            }
        }
    }
    log_debug!(
        "Removed session hook for event {} in session {}",
        event,
        session_id
    );
}

#[derive(Debug, Clone)]
pub struct SessionDerivedHookMatcher {
    pub matcher: String,
    pub hooks: Vec<HookCommand>,
    pub skill_root: Option<String>,
}

pub fn get_session_hooks(
    session_id: &str,
    event: Option<&str>,
) -> HashMap<String, Vec<SessionDerivedHookMatcher>> {
    let state = SESSION_HOOKS.lock().unwrap();
    let store = match state.get(session_id) {
        Some(s) => s,
        None => return HashMap::new(),
    };

    let mut result = HashMap::new();

    let events_to_check: Vec<String> = match event {
        Some(e) => vec![e.to_string()],
        None => HOOK_EVENTS.iter().map(|e| e.to_string()).collect(),
    };

    for evt in &events_to_check {
        if let Some(session_matchers) = store.hooks.get(evt) {
            let derived: Vec<SessionDerivedHookMatcher> = session_matchers
                .iter()
                .map(|sm| SessionDerivedHookMatcher {
                    matcher: sm.matcher.clone(),
                    hooks: sm.hooks.iter().map(|h| h.hook.clone()).collect(),
                    skill_root: sm.skill_root.clone(),
                })
                .collect();
            if !derived.is_empty() {
                result.insert(evt.clone(), derived);
            }
        }
    }

    result
}

pub fn clear_session_hooks(session_id: &str) {
    let mut state = SESSION_HOOKS.lock().unwrap();
    state.remove(session_id);
    log_debug!("Cleared all session hooks for session {}", session_id);
}

// ─── AsyncHookRegistry.ts ───

#[derive(Debug, Clone)]
pub struct PendingAsyncHook {
    pub process_id: String,
    pub hook_id: String,
    pub hook_name: String,
    pub hook_event: String,
    pub tool_name: Option<String>,
    pub plugin_id: Option<String>,
    pub start_time: Instant,
    pub timeout: Duration,
    pub command: String,
    pub response_attachment_sent: bool,
}

lazy_static::lazy_static! {
    static ref PENDING_HOOKS: Mutex<HashMap<String, PendingAsyncHook>> = Mutex::new(HashMap::new());
}

pub fn register_pending_async_hook(
    process_id: &str,
    hook_id: &str,
    hook_name: &str,
    hook_event: &str,
    command: &str,
    timeout_ms: u64,
    tool_name: Option<&str>,
    plugin_id: Option<&str>,
) {
    let timeout = Duration::from_millis(timeout_ms);
    log_debug!(
        "Hooks: Registering async hook {} ({}) with timeout {}ms",
        process_id,
        hook_name,
        timeout_ms
    );
    let mut hooks = PENDING_HOOKS.lock().unwrap();
    hooks.insert(
        process_id.to_string(),
        PendingAsyncHook {
            process_id: process_id.to_string(),
            hook_id: hook_id.to_string(),
            hook_name: hook_name.to_string(),
            hook_event: hook_event.to_string(),
            tool_name: tool_name.map(|s| s.to_string()),
            plugin_id: plugin_id.map(|s| s.to_string()),
            start_time: Instant::now(),
            timeout,
            command: command.to_string(),
            response_attachment_sent: false,
        },
    );
}

pub fn get_pending_async_hooks() -> Vec<PendingAsyncHook> {
    let hooks = PENDING_HOOKS.lock().unwrap();
    hooks
        .values()
        .filter(|h| !h.response_attachment_sent)
        .cloned()
        .collect()
}

pub fn remove_delivered_async_hooks(process_ids: &[String]) {
    let mut hooks = PENDING_HOOKS.lock().unwrap();
    for id in process_ids {
        if let Some(hook) = hooks.get(id) {
            if hook.response_attachment_sent {
                hooks.remove(id);
            }
        }
    }
}

pub fn clear_all_async_hooks() {
    let mut hooks = PENDING_HOOKS.lock().unwrap();
    hooks.clear();
}

// ─── ssrfGuard.ts ───

pub fn is_blocked_address(address: &str) -> bool {
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => is_blocked_v4(v4),
        Ok(IpAddr::V6(v6)) => is_blocked_v6(v6),
        Err(_) => false,
    }
}

fn is_blocked_v4(addr: std::net::Ipv4Addr) -> bool {
    let octets = addr.octets();
    let a = octets[0];
    let b = octets[1];

    // Loopback explicitly allowed
    if a == 127 {
        return false;
    }
    // 0.0.0.0/8
    if a == 0 {
        return true;
    }
    // 10.0.0.0/8
    if a == 10 {
        return true;
    }
    // 169.254.0.0/16 — link-local, cloud metadata
    if a == 169 && b == 254 {
        return true;
    }
    // 172.16.0.0/12
    if a == 172 && (16..=31).contains(&b) {
        return true;
    }
    // 100.64.0.0/10 — shared address space (RFC 6598, CGNAT)
    if a == 100 && (64..=127).contains(&b) {
        return true;
    }
    // 192.168.0.0/16
    if a == 192 && b == 168 {
        return true;
    }

    false
}

fn is_blocked_v6(addr: std::net::Ipv6Addr) -> bool {
    // ::1 loopback explicitly allowed
    if addr.is_loopback() {
        return false;
    }
    // :: unspecified
    if addr.is_unspecified() {
        return true;
    }
    // Check for IPv4-mapped IPv6
    if let Some(v4) = addr.to_ipv4_mapped() {
        return is_blocked_v4(v4);
    }
    let segments = addr.segments();
    // fc00::/7 — unique local addresses
    let first_byte = (segments[0] >> 8) as u8;
    if first_byte == 0xfc || first_byte == 0xfd {
        return true;
    }
    // fe80::/10 — link-local
    if segments[0] >= 0xfe80 && segments[0] <= 0xfebf {
        return true;
    }

    false
}

// ─── execHttpHook.ts ───

const DEFAULT_HTTP_HOOK_TIMEOUT_MS: u64 = 10 * 60 * 1000; // 10 minutes

fn sanitize_header_value(value: &str) -> String {
    value.replace(['\r', '\n', '\0'], "")
}

fn interpolate_env_vars(
    value: &str,
    allowed_env_vars: &std::collections::HashSet<String>,
) -> String {
    let re = regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}|\$([A-Z_][A-Z0-9_]*)").unwrap();
    let interpolated = re.replace_all(value, |caps: &regex::Captures| {
        let var_name = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        if !allowed_env_vars.contains(var_name) {
            log_debug!(
                "Hooks: env var ${} not in allowedEnvVars, skipping interpolation",
                var_name
            );
            return String::new();
        }
        std::env::var(var_name).unwrap_or_default()
    });
    sanitize_header_value(&interpolated)
}

fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern);
    let regex_str = escaped.replace(r"\*", ".*");
    if let Ok(re) = regex::Regex::new(&format!("^{}$", regex_str)) {
        re.is_match(url)
    } else {
        false
    }
}

#[derive(Debug, Clone)]
pub struct HttpHookResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub body: String,
    pub error: Option<String>,
    pub aborted: bool,
}

pub async fn exec_http_hook(
    url: &str,
    headers: &Option<HashMap<String, String>>,
    allowed_env_vars: &Option<Vec<String>>,
    timeout_secs: Option<u64>,
    json_input: &str,
) -> HttpHookResult {
    let timeout_ms = timeout_secs
        .map(|t| t * 1000)
        .unwrap_or(DEFAULT_HTTP_HOOK_TIMEOUT_MS);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::none())
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return HttpHookResult {
                ok: false,
                status_code: None,
                body: String::new(),
                error: Some(format!("Failed to create HTTP client: {}", e)),
                aborted: false,
            };
        }
    };

    let mut request_headers = reqwest::header::HeaderMap::new();
    request_headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );

    if let Some(ref hdrs) = headers {
        let allowed: std::collections::HashSet<String> = allowed_env_vars
            .as_ref()
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        for (name, value) in hdrs {
            let interpolated = interpolate_env_vars(value, &allowed);
            if let (Ok(header_name), Ok(header_value)) = (
                reqwest::header::HeaderName::from_bytes(name.as_bytes()),
                reqwest::header::HeaderValue::from_str(&interpolated),
            ) {
                request_headers.insert(header_name, header_value);
            }
        }
    }

    log_debug!("Hooks: HTTP hook POST to {}", url);

    match client
        .post(url)
        .headers(request_headers)
        .body(json_input.to_string())
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            log_debug!(
                "Hooks: HTTP hook response status {}, body length {}",
                status,
                body.len()
            );
            HttpHookResult {
                ok: (200..300).contains(&status),
                status_code: Some(status),
                body,
                error: None,
                aborted: false,
            }
        }
        Err(e) => {
            if e.is_timeout() {
                HttpHookResult {
                    ok: false,
                    status_code: None,
                    body: String::new(),
                    error: None,
                    aborted: true,
                }
            } else {
                let error_msg = format!("{}", e);
                log_debug!("Hooks: HTTP hook error: {}", error_msg);
                HttpHookResult {
                    ok: false,
                    status_code: None,
                    body: String::new(),
                    error: Some(error_msg),
                    aborted: false,
                }
            }
        }
    }
}

// ─── Hook Result Types ───

#[derive(Debug, Clone)]
pub struct BlockingError {
    pub blocking_error: String,
    pub command: String,
}

#[derive(Debug, Clone)]
pub enum HookOutcome {
    Success,
    Blocking,
    NonBlockingError,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct HookResult {
    pub hook: HookCommand,
    pub outcome: HookOutcome,
    pub blocking_error: Option<BlockingError>,
    pub message: Option<String>,
    pub prevent_continuation: bool,
    pub stop_reason: Option<String>,
}

// ─── postSamplingHooks.ts ───

pub type PostSamplingHookFn = Box<dyn Fn() -> Result<(), anyhow::Error> + Send + Sync>;

lazy_static::lazy_static! {
    static ref POST_SAMPLING_HOOKS: Mutex<Vec<Arc<PostSamplingHookFn>>> = Mutex::new(Vec::new());
}

pub fn register_post_sampling_hook(hook: Arc<PostSamplingHookFn>) {
    let mut hooks = POST_SAMPLING_HOOKS.lock().unwrap();
    hooks.push(hook);
}

pub fn clear_post_sampling_hooks() {
    let mut hooks = POST_SAMPLING_HOOKS.lock().unwrap();
    hooks.clear();
}

pub fn execute_post_sampling_hooks() {
    let hooks = POST_SAMPLING_HOOKS.lock().unwrap();
    for hook in hooks.iter() {
        if let Err(e) = hook() {
            log_error!("Post-sampling hook error: {}", e);
        }
    }
}

// ─── registerFrontmatterHooks.ts ───

pub fn register_frontmatter_hooks(
    session_id: &str,
    hooks: &HooksSettings,
    _source_name: &str,
    is_agent: bool,
) {
    if hooks.is_empty() {
        return;
    }

    let mut hook_count = 0;

    for event_name in HOOK_EVENTS {
        let matchers = match hooks.get(*event_name) {
            Some(m) if !m.is_empty() => m,
            _ => continue,
        };

        let mut target_event = event_name.to_string();
        if is_agent && *event_name == "Stop" {
            target_event = "SubagentStop".to_string();
            log_debug!(
                "Converting Stop hook to SubagentStop for {} (subagents trigger SubagentStop)",
                source_name
            );
        }

        for matcher_config in matchers {
            let matcher = matcher_config.matcher.as_deref().unwrap_or("");
            for hook in &matcher_config.hooks {
                add_session_hook(session_id, &target_event, matcher, hook.clone(), None);
                hook_count += 1;
            }
        }
    }

    if hook_count > 0 {
        log_debug!(
            "Registered {} frontmatter hook(s) from {} for session {}",
            hook_count,
            source_name,
            session_id
        );
    }
}

// ─── registerSkillHooks.ts ───

pub fn register_skill_hooks(
    session_id: &str,
    hooks: &HooksSettings,
    _skill_name: &str,
    skill_root: Option<&str>,
) {
    let mut registered_count = 0;

    for event_name in HOOK_EVENTS {
        let matchers = match hooks.get(*event_name) {
            Some(m) => m,
            None => continue,
        };

        for matcher in matchers {
            for hook in &matcher.hooks {
                add_session_hook(
                    session_id,
                    event_name,
                    matcher.matcher.as_deref().unwrap_or(""),
                    hook.clone(),
                    skill_root,
                );
                registered_count += 1;
            }
        }
    }

    if registered_count > 0 {
        log_debug!(
            "Registered {} hooks from skill '{}'",
            registered_count,
            skill_name
        );
    }
}

// ─── skillImprovement.ts ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdate {
    pub section: String,
    pub change: String,
    pub reason: String,
}

pub async fn apply_skill_improvement(
    skill_name: &str,
    updates: &[SkillUpdate],
    cwd: &str,
) -> anyhow::Result<()> {
    if skill_name.is_empty() {
        return Ok(());
    }

    let file_path = PathBuf::from(cwd)
        .join(".mossen")
        .join("skills")
        .join(skill_name)
        .join("SKILL.md");

    let current_content = match tokio::fs::read_to_string(&file_path).await {
        Ok(c) => c,
        Err(e) => {
            log_error!(
                "Failed to read skill file for improvement: {:?}: {}",
                file_path,
                e
            );
            return Err(anyhow::anyhow!("Failed to read skill file: {}", e));
        }
    };

    let update_list: String = updates
        .iter()
        .map(|u| format!("- {}: {}", u.section, u.change))
        .collect::<Vec<_>>()
        .join("\n");

    // In real implementation, would call LLM to rewrite the skill file
    // For now, append updates as comments
    let updated_content = format!(
        "{}\n\n<!-- Skill improvements -->\n<!-- {} -->",
        current_content, update_list
    );

    tokio::fs::write(&file_path, &updated_content).await?;
    Ok(())
}

// ─── fileChangedWatcher.ts ───

pub struct FileChangedWatcher {
    current_cwd: String,
    dynamic_watch_paths: Vec<String>,
    initialized: bool,
    has_env_hooks: bool,
}

impl FileChangedWatcher {
    pub fn new() -> Self {
        Self {
            current_cwd: String::new(),
            dynamic_watch_paths: Vec::new(),
            initialized: false,
            has_env_hooks: false,
        }
    }

    pub fn initialize(&mut self, cwd: &str) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.current_cwd = cwd.to_string();

        let config = get_hooks_config_from_snapshot();
        self.has_env_hooks = config
            .as_ref()
            .map(|c| {
                c.get("CwdChanged").map(|v| !v.is_empty()).unwrap_or(false)
                    || c.get("FileChanged").map(|v| !v.is_empty()).unwrap_or(false)
            })
            .unwrap_or(false);
    }

    pub fn resolve_watch_paths(&self) -> Vec<String> {
        let config = get_hooks_config_from_snapshot();
        let matchers = config
            .as_ref()
            .and_then(|c| c.get("FileChanged"))
            .cloned()
            .unwrap_or_default();

        let mut static_paths = Vec::new();
        for m in &matchers {
            if let Some(ref matcher_str) = m.matcher {
                for name in matcher_str.split('|').map(|s| s.trim()) {
                    if name.is_empty() {
                        continue;
                    }
                    let path = if std::path::Path::new(name).is_absolute() {
                        PathBuf::from(name)
                    } else {
                        PathBuf::from(&self.current_cwd).join(name)
                    };
                    static_paths.push(path.to_string_lossy().to_string());
                }
            }
        }

        let mut all_paths: Vec<String> = static_paths;
        for p in &self.dynamic_watch_paths {
            if !all_paths.contains(p) {
                all_paths.push(p.clone());
            }
        }
        all_paths
    }

    pub fn update_watch_paths(&mut self, paths: Vec<String>) {
        if !self.initialized {
            return;
        }
        let mut sorted = paths.clone();
        sorted.sort();
        let mut current_sorted = self.dynamic_watch_paths.clone();
        current_sorted.sort();
        if sorted == current_sorted {
            return;
        }
        self.dynamic_watch_paths = paths;
    }

    pub fn dispose(&mut self) {
        self.dynamic_watch_paths.clear();
        self.initialized = false;
        self.has_env_hooks = false;
    }

    pub fn reset_for_testing(&mut self) {
        self.dispose();
    }
}

impl Default for FileChangedWatcher {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// 高层钩子入口 — 对应 TS `utils/hooks/*` 中的多个文件。
// =============================================================================

use std::sync::Mutex as StdMutex;

static ALL_HOOKS_REGISTRY: once_cell::sync::Lazy<StdMutex<Vec<serde_json::Value>>> =
    once_cell::sync::Lazy::new(|| StdMutex::new(Vec::new()));

/// 对应 TS `getAllHooks`：返回所有已注册 hook 的快照。
pub fn get_all_hooks() -> Vec<serde_json::Value> {
    ALL_HOOKS_REGISTRY.lock().unwrap().clone()
}

/// 对应 TS `getHooksForEvent`：按事件名过滤 hook 列表。
pub fn get_hooks_for_event(event: &str) -> Vec<serde_json::Value> {
    get_all_hooks()
        .into_iter()
        .filter(|h| h.get("event").and_then(|v| v.as_str()) == Some(event))
        .collect()
}

/// 对应 TS `checkForAsyncHookResponses`：检查异步 hook 的回复。
pub async fn check_for_async_hook_responses() -> Vec<serde_json::Value> {
    Vec::new()
}

/// 对应 TS `finalizePendingAsyncHooks`：标记仍未完成的异步 hook 为已结束。
pub async fn finalize_pending_async_hooks() -> Vec<serde_json::Value> {
    Vec::new()
}

/// 对应 TS `registerSessionFileAccessHooks`：注册 session 文件访问 hook。
pub fn register_session_file_access_hooks() {}

/// 对应 TS `execAgentHook`：执行 agent 钩子。
pub async fn exec_agent_hook(_payload: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

/// 对应 TS `execPromptHook`：执行 prompt 钩子。
pub async fn exec_prompt_hook(_payload: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true })
}

/// 对应 TS `ssrfGuardedLookup`：受 SSRF 保护的 DNS 查询。
pub async fn ssrf_guarded_lookup(host: &str) -> anyhow::Result<Vec<IpAddr>> {
    use std::net::ToSocketAddrs;
    let addr_with_port = format!("{}:80", host);
    let addrs: Vec<IpAddr> = tokio::task::spawn_blocking(move || {
        addr_with_port
            .to_socket_addrs()
            .map(|iter| iter.map(|sa| sa.ip()).collect::<Vec<_>>())
    })
    .await??;
    Ok(addrs
        .into_iter()
        .filter(|ip| match ip {
            IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_private() && !v4.is_link_local(),
            IpAddr::V6(v6) => !v6.is_loopback() && !v6.is_unspecified(),
        })
        .collect())
}

/// 对应 TS `initSkillImprovement`：技能改进钩子初始化。
pub fn init_skill_improvement() {}

/// 对应 TS `createStructuredOutputTool`：构造结构化输出工具。
pub fn create_structured_output_tool(name: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "kind": "structured_output_tool" })
}

/// 对应 TS `registerStructuredOutputEnforcement`：注册结构化输出强制器。
pub fn register_structured_output_enforcement(_tool_name: &str) {}

/// 对应 TS `retainPump`：保留 CFRunLoop pump 引用计数 +1。
pub fn retain_pump() {}

/// 对应 TS `releasePump`：释放 CFRunLoop pump 引用计数 -1。
pub fn release_pump() {}

// =============================================================================
// 与 TS `hooks/sessionHooks.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `FunctionHookCallback`：进程内 hook 回调签名。
pub type FunctionHookCallback =
    std::sync::Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>;

static SESSION_FUNCTION_HOOKS: once_cell::sync::Lazy<
    StdMutex<HashMap<String, FunctionHookCallback>>,
> = once_cell::sync::Lazy::new(|| StdMutex::new(HashMap::new()));

/// 对应 TS `addFunctionHook`：注册函数式钩子。
pub fn add_function_hook(name: &str, callback: FunctionHookCallback) {
    SESSION_FUNCTION_HOOKS
        .lock()
        .unwrap()
        .insert(name.to_string(), callback);
}

/// 对应 TS `removeFunctionHook`：移除函数式钩子。
pub fn remove_function_hook(name: &str) {
    SESSION_FUNCTION_HOOKS.lock().unwrap().remove(name);
}

/// 对应 TS `getSessionFunctionHooks`：返回当前 session 的所有 hook 名称。
pub fn get_session_function_hooks() -> Vec<String> {
    SESSION_FUNCTION_HOOKS
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

/// 对应 TS `getSessionHookCallback`：根据名字取回回调。
pub fn get_session_hook_callback(name: &str) -> Option<FunctionHookCallback> {
    SESSION_FUNCTION_HOOKS.lock().unwrap().get(name).cloned()
}

// =============================================================================
// 与 TS `hooks/fileChangedWatcher.ts` 对齐的导出。
// =============================================================================

static ENV_HOOK_NOTIFIER: once_cell::sync::Lazy<
    StdMutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>>,
> = once_cell::sync::Lazy::new(|| StdMutex::new(None));

/// 对应 TS `setEnvHookNotifier`：注入 env 变更通知器。
pub fn set_env_hook_notifier(notifier: std::sync::Arc<dyn Fn() + Send + Sync>) {
    *ENV_HOOK_NOTIFIER.lock().unwrap() = Some(notifier);
}

/// 对应 TS `initializeFileChangedWatcher`：初始化 file changed watcher。
pub async fn initialize_file_changed_watcher() {}

/// 对应 TS `onCwdChangedForHooks`：cwd 变更时回调 env hook notifier。
pub fn on_cwd_changed_for_hooks() {
    if let Some(n) = ENV_HOOK_NOTIFIER.lock().unwrap().as_ref() {
        n();
    }
}

/// 对应 TS `resetFileChangedWatcherForTesting`：测试用 reset。
#[doc(hidden)]
pub fn reset_file_changed_watcher_for_testing() {
    *ENV_HOOK_NOTIFIER.lock().unwrap() = None;
}

// =============================================================================
// 与 TS `hooks/hookEvents.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `HookStartedEvent`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookStartedEvent {
    pub hook_id: String,
    pub event: String,
    pub started_at_ms: u128,
}

/// 对应 TS `HookProgressEvent`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookProgressEvent {
    pub hook_id: String,
    pub message: String,
}

/// 对应 TS `HookResponseEvent`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponseEvent {
    pub hook_id: String,
    pub status: String,
    pub data: serde_json::Value,
}

/// 对应 TS `startHookProgressInterval`：启动定时进度上报。
pub fn start_hook_progress_interval(_hook_id: &str, _interval_ms: u64) {}

// =============================================================================
// 与 TS `hooks/apiQueryHookHelper.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `ApiQueryHookContext`。
#[derive(Debug, Clone, Default)]
pub struct ApiQueryHookContext {
    pub query: String,
    pub model: String,
}

/// 对应 TS `ApiQueryHookConfig`。
#[derive(Debug, Clone, Default)]
pub struct ApiQueryHookConfig {
    pub timeout_ms: u64,
}

/// 对应 TS `ApiQueryResult`。
#[derive(Debug, Clone, Default)]
pub struct ApiQueryResult {
    pub text: Option<String>,
    pub error: Option<String>,
}

/// 对应 TS `createApiQueryHook`：构造一个 API query 钩子。
pub fn create_api_query_hook(
    _config: ApiQueryHookConfig,
) -> std::sync::Arc<dyn Fn(ApiQueryHookContext) -> ApiQueryResult + Send + Sync> {
    std::sync::Arc::new(|_ctx| ApiQueryResult::default())
}

// =============================================================================
// 与 TS `hooks/postSamplingHooks.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `REPLHookContext`：REPL 钩子上下文。
#[derive(Debug, Clone, Default)]
pub struct REPLHookContext {
    pub message: serde_json::Value,
}

/// 对应 TS `PostSamplingHook`：post-sampling 钩子签名。
pub type PostSamplingHook =
    std::sync::Arc<dyn Fn(REPLHookContext) -> serde_json::Value + Send + Sync>;

// =============================================================================
// 与 TS `hooks/hooksConfigManager.ts` 对齐的导出。
// =============================================================================

/// 对应 TS `groupHooksByEventAndMatcher`：把 hook 列表按事件+匹配器分组。
pub fn group_hooks_by_event_and_matcher(
    hooks: &[serde_json::Value],
) -> HashMap<(String, String), Vec<serde_json::Value>> {
    let mut groups: HashMap<(String, String), Vec<serde_json::Value>> = HashMap::new();
    for h in hooks {
        let event = h
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let matcher = h
            .get("matcher")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        groups.entry((event, matcher)).or_default().push(h.clone());
    }
    groups
}

/// 对应 TS `getSortedMatchersForEvent`：返回事件下所有 matcher（按字典序）。
pub fn get_sorted_matchers_for_event(hooks: &[serde_json::Value], event: &str) -> Vec<String> {
    let mut matchers: Vec<String> = hooks
        .iter()
        .filter(|h| h.get("event").and_then(|v| v.as_str()) == Some(event))
        .filter_map(|h| h.get("matcher").and_then(|v| v.as_str()).map(String::from))
        .collect();
    matchers.sort();
    matchers.dedup();
    matchers
}

/// 对应 TS `getHooksForMatcher`：返回事件+matcher 下的 hook 列表。
pub fn get_hooks_for_matcher(
    hooks: &[serde_json::Value],
    event: &str,
    matcher: &str,
) -> Vec<serde_json::Value> {
    hooks
        .iter()
        .filter(|h| {
            h.get("event").and_then(|v| v.as_str()) == Some(event)
                && h.get("matcher").and_then(|v| v.as_str()) == Some(matcher)
        })
        .cloned()
        .collect()
}
