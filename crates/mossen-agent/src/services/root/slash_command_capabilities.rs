/// Slash command capabilities manifest for stream-json protocol.

use serde::{Deserialize, Serialize};

pub const STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArgsMode {
    None,
    ConfirmOnly,
    ReadOnlyNoArgs,
    ProfileName,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    None,
    ClearsConversation,
    SwitchesSessionModel,
    ReadOnly,
    WritesConfig,
    WritesFiles,
    InstallsPackage,
    StartsProcess,
    Network,
    AuthState,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    Help,
    Capabilities,
    Status,
    Model,
    Clear,
    Cost,
    Skills,
    Mcp,
    Plugin,
    Agents,
    Permissions,
    Hooks,
    Memory,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Available,
    Blocked,
    Unavailable,
    Preview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandCapability {
    pub id: String,
    pub command: String,
    pub title: String,
    pub kind: String,
    pub protocol: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub status: CommandStatus,
    pub read_only: bool,
    pub requires_confirmation: bool,
    pub args_mode: ArgsMode,
    #[serde(default)]
    pub accepted_args: Vec<String>,
    pub side_effect: SideEffect,
    pub result_kind: ResultKind,
    #[serde(default)]
    pub payload_keys: Vec<String>,
    #[serde(default)]
    pub error_tags: Vec<String>,
    pub source: String,
    pub last_verified_by_smoke: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

use once_cell::sync::Lazy;

static CAPABILITIES: Lazy<Vec<SlashCommandCapability>> = Lazy::new(|| {
    vec![
        cap("slash.help", "help", "List slash commands", CommandStatus::Available, true, false, ArgsMode::None, SideEffect::None, ResultKind::Help, &["commands", "streamJsonCapabilities"], "cli/print.ts:slash_command/help", "List stream-json slash command capabilities.", None),
        cap("slash.capabilities", "capabilities", "Capability manifest", CommandStatus::Available, true, false, ArgsMode::None, SideEffect::None, ResultKind::Capabilities, &["capabilities"], "cli/print.ts:slash_command/capabilities", "Return the machine-readable stream-json slash capability manifest.", None),
        cap("slash.status", "status", "Runtime status", CommandStatus::Available, true, false, ArgsMode::None, SideEffect::None, ResultKind::Status, &["runtime"], "cli/print.ts:slash_command/status", "Return runtime status for the current stream-json session.", None),
        cap("slash.model", "model", "Model / profile", CommandStatus::Available, false, false, ArgsMode::ProfileName, SideEffect::SwitchesSessionModel, ResultKind::Model, &["model"], "cli/print.ts:slash_command/model", "Return current model/profile state or switch to a named session profile.", None),
        cap("slash.clear", "clear", "Clear conversation", CommandStatus::Available, false, true, ArgsMode::ConfirmOnly, SideEffect::ClearsConversation, ResultKind::Clear, &["clear"], "cli/print.ts:slash_command/clear", "Clear the current conversation when --confirm is provided.", None),
        cap("slash.cost", "cost", "Session cost / usage", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Cost, &["cost"], "cli/print.ts:slash_command/cost", "Return current session cost and usage totals.", None),
        cap("slash.skills", "skills", "Skills inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Skills, &["skills"], "cli/print.ts:slash_command/skills", "Return available model-facing skills without skill content.", None),
        cap("slash.mcp", "mcp", "MCP server inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Mcp, &["mcp"], "cli/print.ts:slash_command/mcp", "Return MCP server and tool status without raw server config.", None),
        cap("slash.plugin", "plugin", "Plugin inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Plugin, &["plugins"], "cli/print.ts:slash_command/plugin", "Return plugin inventory without installing or changing config.", None),
        cap("slash.agents", "agents", "Agent inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Agents, &["agents"], "cli/print.ts:slash_command/agents", "Return active agent definitions without prompts or local paths.", None),
        cap("slash.permissions", "permissions", "Permission rules summary", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Permissions, &["permissions"], "cli/print.ts:slash_command/permissions", "Return current permission mode and per-source rule counts (no rule patterns).", None),
        cap("slash.hooks", "hooks", "Hooks inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Hooks, &["hooks"], "cli/print.ts:slash_command/hooks", "Return hook event/source/type counts without command/url/prompt bodies.", None),
        cap("slash.memory", "memory", "Memory file inventory", CommandStatus::Available, true, false, ArgsMode::ReadOnlyNoArgs, SideEffect::None, ResultKind::Memory, &["memory"], "cli/print.ts:slash_command/memory", "Return memory file paths, types, and sizes without file content.", None),
        cap("slash.compact", "compact", "Compact conversation", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::None, ResultKind::Error, &[], "cli/print.ts:slash_command/compact", "Context compaction is not exposed through stream-json slash_command.", Some("use control_request subtype \"compact_conversation\"")),
        cap("slash.context", "context", "Context usage breakdown", CommandStatus::Blocked, true, false, ArgsMode::None, SideEffect::None, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Context usage is exposed through the dedicated control_request subtype.", Some("use control_request subtype \"get_context_usage\"")),
        cap("slash.config", "config", "Settings inspector", CommandStatus::Blocked, true, false, ArgsMode::None, SideEffect::None, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Settings inspection is exposed through dedicated control_request subtypes.", Some("use control_request subtype \"get_config_summary\"")),
        cap("slash.profile", "profile", "Profile inventory", CommandStatus::Blocked, true, false, ArgsMode::None, SideEffect::None, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Profile state is already covered by /model.", Some("duplicate of /model")),
        cap("slash.doctor", "doctor", "Installation diagnostics", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::Network, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Doctor diagnostics are exposed through the dedicated control_request subtype.", Some("use control_request subtype \"runtime_doctor_summary\"")),
        cap("slash.diff", "diff", "Uncommitted diff viewer", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::StartsProcess, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Git diff summary is exposed through the dedicated control_request subtype.", Some("use control_request subtype \"git_diff_summary\"")),
        cap("slash.ide", "ide", "IDE integration status", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::StartsProcess, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "IDE state depends on async MCP IDE handshake.", Some("use mcp_status control_request")),
        cap("slash.init", "init", "CLAUDE.md initializer", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::WritesFiles, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Init would write project memory files; no safe stream-json gate exists.", Some("init writes CLAUDE.md and project metadata")),
        cap("slash.login", "login", "Backend login", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::AuthState, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Login mutates auth state and runs an interactive flow.", Some("auth flow requires interactive UI")),
        cap("slash.logout", "logout", "Backend logout", CommandStatus::Blocked, false, false, ArgsMode::None, SideEffect::AuthState, ResultKind::Error, &[], "cli/print.ts:slash_command/blocked", "Logout mutates locally cached auth state.", Some("auth state mutation")),
    ]
});

fn cap(
    id: &str, command: &str, title: &str, status: CommandStatus,
    read_only: bool, requires_confirmation: bool, args_mode: ArgsMode,
    side_effect: SideEffect, result_kind: ResultKind,
    payload_keys: &[&str], source: &str, summary: &str, reason: Option<&str>,
) -> SlashCommandCapability {
    SlashCommandCapability {
        id: id.to_string(),
        command: command.to_string(),
        title: title.to_string(),
        kind: "slash_command".to_string(),
        protocol: "stream_json".to_string(),
        aliases: Vec::new(),
        status,
        read_only,
        requires_confirmation,
        args_mode,
        accepted_args: Vec::new(),
        side_effect,
        result_kind,
        payload_keys: payload_keys.iter().map(|s| s.to_string()).collect(),
        error_tags: Vec::new(),
        source: source.to_string(),
        last_verified_by_smoke: String::new(),
        summary: summary.to_string(),
        reason: reason.map(|s| s.to_string()),
    }
}

pub fn get_stream_json_slash_command_capabilities() -> &'static [SlashCommandCapability] {
    &CAPABILITIES
}

pub fn get_stream_json_slash_command_capability_manifest() -> Vec<SlashCommandCapability> {
    CAPABILITIES.clone()
}

pub fn normalize_stream_json_slash_command(command: &str) -> String {
    let normalized = command.trim().trim_start_matches('/').to_lowercase();
    let found = CAPABILITIES.iter().find(|cap| {
        cap.command == normalized || cap.aliases.iter().any(|a| a == &normalized)
    });
    found
        .map(|c| c.command.clone())
        .unwrap_or(normalized)
}

pub fn get_stream_json_slash_command_capability(
    command: &str,
) -> Option<&'static SlashCommandCapability> {
    let normalized = normalize_stream_json_slash_command(command);
    CAPABILITIES.iter().find(|cap| cap.command == normalized)
}

pub fn is_stream_json_slash_command_available(command: &str) -> bool {
    get_stream_json_slash_command_capability(command)
        .map(|c| c.status == CommandStatus::Available)
        .unwrap_or(false)
}

pub fn is_stream_json_slash_command_blocked(command: &str) -> bool {
    get_stream_json_slash_command_capability(command)
        .map(|c| c.status == CommandStatus::Blocked)
        .unwrap_or(false)
}

pub fn format_available_stream_json_slash_commands() -> String {
    CAPABILITIES
        .iter()
        .filter(|cap| cap.status == CommandStatus::Available)
        .map(|cap| cap.command.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// TS-mirror — `slashCommandCapabilities.ts` exports (type aliases + serde
// payload helpers).
// ---------------------------------------------------------------------------

/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandArgsMode`.
pub type StreamJsonSlashCommandArgsMode = ArgsMode;
/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandSideEffect`.
pub type StreamJsonSlashCommandSideEffect = SideEffect;
/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandResultKind`.
pub type StreamJsonSlashCommandResultKind = ResultKind;
/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandStatus`.
pub type StreamJsonSlashCommandStatus = CommandStatus;
/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandCapability`.
pub type StreamJsonSlashCommandCapability = SlashCommandCapability;

/// `slashCommandCapabilities.ts` `STREAM_JSON_SLASH_COMMAND_CAPABILITIES` —
/// exposes the static capability table at the top-level name.
pub fn stream_json_slash_command_capabilities() -> &'static [SlashCommandCapability] {
    &CAPABILITIES
}

/// `slashCommandCapabilities.ts` `StreamJsonSlashCommandCapabilityPayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamJsonSlashCommandCapabilityPayload {
    pub id: String,
    pub command: String,
    pub title: String,
    pub status: CommandStatus,
    pub read_only: bool,
    pub requires_confirmation: bool,
    pub args_mode: ArgsMode,
    pub side_effect: SideEffect,
    pub result_kind: ResultKind,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `slashCommandCapabilities.ts` `serializeStreamJsonSlashCommandCapability`.
pub fn serialize_stream_json_slash_command_capability(
    capability: &SlashCommandCapability,
) -> StreamJsonSlashCommandCapabilityPayload {
    StreamJsonSlashCommandCapabilityPayload {
        id: capability.id.clone(),
        command: capability.command.clone(),
        title: capability.title.clone(),
        status: capability.status.clone(),
        read_only: capability.read_only,
        requires_confirmation: capability.requires_confirmation,
        args_mode: capability.args_mode.clone(),
        side_effect: capability.side_effect.clone(),
        result_kind: capability.result_kind.clone(),
        aliases: capability.aliases.clone(),
        summary: capability.summary.clone(),
        reason: capability.reason.clone(),
    }
}
