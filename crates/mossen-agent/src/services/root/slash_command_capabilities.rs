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
    Subcommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    None,
    ClearsConversation,
    SwitchesSessionModel,
    UpdatesGoal,
    ReadOnly,
    WritesConfig,
    WritesFiles,
    InstallsPackage,
    StartsProcess,
    Network,
    AuthState,
    SwitchesPermissionMode,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    Help,
    Capabilities,
    Status,
    Context,
    Config,
    Doctor,
    Ide,
    Init,
    Auth,
    Profile,
    Model,
    Goal,
    Clear,
    Cost,
    Skills,
    Mcp,
    Plugin,
    Agents,
    Approvals,
    Permissions,
    Plan,
    Hooks,
    Memory,
    Compact,
    Diff,
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
        cap(
            "slash.help",
            "help",
            "List slash commands",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::None,
            SideEffect::None,
            ResultKind::Help,
            &["commands", "streamJsonCapabilities"],
            "cli/print.ts:slash_command/help",
            "List stream-json slash command capabilities.",
            None,
        ),
        cap(
            "slash.capabilities",
            "capabilities",
            "Capability manifest",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::None,
            SideEffect::None,
            ResultKind::Capabilities,
            &["capabilities"],
            "cli/print.ts:slash_command/capabilities",
            "Return the machine-readable stream-json slash capability manifest.",
            None,
        ),
        cap(
            "slash.status",
            "status",
            "Runtime status",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::None,
            SideEffect::None,
            ResultKind::Status,
            &["runtime"],
            "cli/print.ts:slash_command/status",
            "Return runtime status for the current stream-json session.",
            None,
        ),
        {
            let mut capability = cap(
                "slash.goal",
                "goal",
                "Thread goal",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::Subcommand,
                SideEffect::UpdatesGoal,
                ResultKind::Goal,
                &["goal", "message"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/goal",
                "Set, inspect, pause, resume, edit, or clear the persistent thread goal.",
                None,
            );
            capability.accepted_args = vec![
                "edit".to_string(),
                "pause".to_string(),
                "resume".to_string(),
                "clear".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.model",
                "model",
                "Model / profile",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::ProfileName,
                SideEffect::SwitchesSessionModel,
                ResultKind::Model,
                &["model"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/model",
                "Return current model/profile state or switch to a named session profile.",
                None,
            );
            capability.accepted_args = vec![
                "status".to_string(),
                "current".to_string(),
                "show".to_string(),
                "list".to_string(),
                "options".to_string(),
                "set".to_string(),
                "use".to_string(),
                "reset".to_string(),
                "default".to_string(),
                "clear".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.clear",
                "clear",
                "Clear conversation",
                CommandStatus::Available,
                false,
                true,
                ArgsMode::ConfirmOnly,
                SideEffect::ClearsConversation,
                ResultKind::Clear,
                &["clear"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/clear",
                "Preview or queue a clear request through the stream-json control bridge; --confirm clears at the dialogue safe point.",
                None,
            );
            capability.accepted_args = vec![
                "preview".to_string(),
                "plan".to_string(),
                "status".to_string(),
                "run".to_string(),
                "--confirm".to_string(),
                "confirm".to_string(),
            ];
            capability
        },
        cap(
            "slash.cost",
            "cost",
            "Session cost / usage",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Cost,
            &["cost"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/cost",
            "Return current session cost and usage totals.",
            None,
        ),
        cap(
            "slash.skills",
            "skills",
            "Skills inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Skills,
            &["skills"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/skills",
            "Return available model-facing skills without skill content.",
            None,
        ),
        cap(
            "slash.mcp",
            "mcp",
            "MCP server inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Mcp,
            &["mcp"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/mcp",
            "Return MCP server and tool/prompt/resource counts without raw server config.",
            None,
        ),
        cap(
            "slash.plugin",
            "plugin",
            "Plugin inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Plugin,
            &["plugins"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/plugin",
            "Return plugin inventory without installing or changing config.",
            None,
        ),
        cap(
            "slash.agents",
            "agents",
            "Agent inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Agents,
            &["agents"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/agents",
            "Return active agent definitions without prompts or local paths.",
            None,
        ),
        {
            let mut capability = cap(
                "slash.approvals",
                "approvals",
                "Approval status",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::None,
                ResultKind::Approvals,
                &["approvals"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/approvals",
                "Return redacted pending approval state and aggregate approval history without resolving decisions.",
                None,
            );
            capability.aliases = vec!["approval-history".to_string(), "approval-log".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "pending".to_string(),
                "history".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.permissions",
                "permissions",
                "Permission mode / rules",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::Subcommand,
                SideEffect::SwitchesPermissionMode,
                ResultKind::Permissions,
                &["permissions"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/permissions",
                "Return or change the current session permission mode and session-scoped allow/deny rules without exposing rule patterns.",
                None,
            );
            capability.aliases = vec![
                "permission-mode".to_string(),
                "approval-mode".to_string(),
                "allowed-tools".to_string(),
            ];
            capability.accepted_args = vec![
                "mode".to_string(),
                "choose".to_string(),
                "select".to_string(),
                "picker".to_string(),
                "set".to_string(),
                "status".to_string(),
                "summary".to_string(),
                "list".to_string(),
                "show".to_string(),
                "rules".to_string(),
                "allow".to_string(),
                "deny".to_string(),
                "reset".to_string(),
                "clear".to_string(),
                "ask".to_string(),
                "supervised".to_string(),
                "suggest".to_string(),
                "read-only".to_string(),
                "readonly".to_string(),
                "auto-edit".to_string(),
                "full-auto".to_string(),
                "default".to_string(),
                "plan".to_string(),
                "acceptEdits".to_string(),
                "accept-edits".to_string(),
                "bypassPermissions".to_string(),
                "bypass".to_string(),
                "dontAsk".to_string(),
                "dont-ask".to_string(),
                "never-ask".to_string(),
                "auto".to_string(),
                "yolo".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.plan",
                "plan",
                "Plan mode",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::Subcommand,
                SideEffect::SwitchesPermissionMode,
                ResultKind::Plan,
                &["plan"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/plan",
                "Return plan-mode status or switch the current stream-json session into or out of plan mode.",
                None,
            );
            capability.aliases = vec!["plan-mode".to_string(), "planning".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "show".to_string(),
                "enter".to_string(),
                "on".to_string(),
                "exit".to_string(),
                "off".to_string(),
            ];
            capability
        },
        cap(
            "slash.hooks",
            "hooks",
            "Hooks inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Hooks,
            &["hooks"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/hooks",
            "Return hook event/source/type counts without command/url/prompt bodies.",
            None,
        ),
        cap(
            "slash.memory",
            "memory",
            "Memory file inventory",
            CommandStatus::Available,
            true,
            false,
            ArgsMode::ReadOnlyNoArgs,
            SideEffect::None,
            ResultKind::Memory,
            &["memory"],
            "crates/mossen-cli/src/structured_io.rs:slash_command/memory",
            "Return memory file paths, types, and sizes without file content.",
            None,
        ),
        {
            let mut capability = cap(
                "slash.compact",
                "compact",
                "Compact conversation",
                CommandStatus::Available,
                false,
                true,
                ArgsMode::Subcommand,
                SideEffect::ClearsConversation,
                ResultKind::Compact,
                &["compact"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/compact",
                "Queue a stream-json compact preview/status/run request through the compact control bridge.",
                None,
            );
            capability.accepted_args = vec![
                "plan".to_string(),
                "preview".to_string(),
                "dry-run".to_string(),
                "dryrun".to_string(),
                "status".to_string(),
                "cancel".to_string(),
                "stop".to_string(),
                "run".to_string(),
                "--confirm".to_string(),
                "confirm".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.context",
                "context",
                "Context usage breakdown",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::None,
                ResultKind::Context,
                &["context"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/context",
                "Return a read-only token usage and context-window snapshot for terminal context panels.",
                None,
            );
            capability.aliases = vec!["ctx".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "usage".to_string(),
                "tokens".to_string(),
                "breakdown".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.config",
                "config",
                "Settings inspector",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::None,
                ResultKind::Config,
                &["config"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/config",
                "Return a redacted read-only session/config source snapshot for terminal settings panels.",
                None,
            );
            capability.aliases = vec!["settings".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "sources".to_string(),
                "runtime".to_string(),
                "security".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.profile",
                "profile",
                "Profile inventory",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::Subcommand,
                SideEffect::SwitchesSessionModel,
                ResultKind::Profile,
                &["profile"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/profile",
                "Return redacted model profile inventory or switch the active profile for the current session.",
                None,
            );
            capability.aliases = vec!["profiles".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "current".to_string(),
                "show".to_string(),
                "summary".to_string(),
                "list".to_string(),
                "profiles".to_string(),
                "options".to_string(),
                "set".to_string(),
                "use".to_string(),
                "reset".to_string(),
                "clear".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.doctor",
                "doctor",
                "Runtime health diagnostics",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::None,
                ResultKind::Doctor,
                &["doctor"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/doctor",
                "Return a redacted read-only stream-json runtime health snapshot without external checks.",
                None,
            );
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "runtime".to_string(),
                "render".to_string(),
                "slash".to_string(),
                "mcp".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.diff",
                "diff",
                "Uncommitted diff viewer",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::StartsProcess,
                ResultKind::Diff,
                &["diff"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/diff",
                "Return a bounded read-only git diff summary without raw patch content.",
                None,
            );
            capability.aliases = vec!["changes".to_string()];
            capability.accepted_args = vec![
                "summary".to_string(),
                "status".to_string(),
                "show".to_string(),
                "files".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.ide",
                "ide",
                "IDE integration status",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::None,
                ResultKind::Ide,
                &["ide"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/ide",
                "Return a redacted read-only IDE/MCP connection snapshot without scanning or opening editors.",
                None,
            );
            capability.aliases = vec!["editor".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "mcp".to_string(),
                "diagnostics".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.init",
                "init",
                "MOSSEN.md initializer",
                CommandStatus::Available,
                false,
                false,
                ArgsMode::Subcommand,
                SideEffect::WritesFiles,
                ResultKind::Init,
                &["init"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/init",
                "Return a prompt handoff for agent-driven MOSSEN.md initialization through normal tool permissions.",
                None,
            );
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "preview".to_string(),
                "prompt".to_string(),
                "run".to_string(),
                "start".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.login",
                "login",
                "Backend credentials",
                CommandStatus::Available,
                true,
                false,
                ArgsMode::Subcommand,
                SideEffect::ReadOnly,
                ResultKind::Auth,
                &["auth"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/auth",
                "Return redacted backend credential status plus setup guidance for personal custom backends.",
                None,
            );
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "preview".to_string(),
                "prompt".to_string(),
                "run".to_string(),
                "start".to_string(),
            ];
            capability
        },
        {
            let mut capability = cap(
                "slash.logout",
                "logout",
                "Credential logout status",
                CommandStatus::Available,
                true,
                true,
                ArgsMode::ConfirmOnly,
                SideEffect::ReadOnly,
                ResultKind::Auth,
                &["auth"],
                "crates/mossen-cli/src/structured_io.rs:slash_command/auth",
                "Return redacted local credential status without mutating personal backend configuration.",
                None,
            );
            capability.aliases = vec!["signout".to_string()];
            capability.accepted_args = vec![
                "status".to_string(),
                "summary".to_string(),
                "preview".to_string(),
                "confirm".to_string(),
                "--confirm".to_string(),
            ];
            capability
        },
    ]
});

fn cap(
    id: &str,
    command: &str,
    title: &str,
    status: CommandStatus,
    read_only: bool,
    requires_confirmation: bool,
    args_mode: ArgsMode,
    side_effect: SideEffect,
    result_kind: ResultKind,
    payload_keys: &[&str],
    source: &str,
    summary: &str,
    reason: Option<&str>,
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
    let found = CAPABILITIES
        .iter()
        .find(|cap| cap.command == normalized || cap.aliases.iter().any(|a| a == &normalized));
    found.map(|c| c.command.clone()).unwrap_or(normalized)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_auth_capabilities_are_status_only_backend_credentials() {
        let login = get_stream_json_slash_command_capability("login").expect("login capability");
        assert_eq!(login.title, "Backend credentials");
        assert!(login.read_only);
        assert_eq!(login.side_effect, SideEffect::ReadOnly);
        assert!(login.summary.contains("backend credential status"));
        let obsolete_handoff_label = ["external CLI", "handoff"].join(" ");
        assert!(!login.summary.contains(&obsolete_handoff_label));

        let logout = get_stream_json_slash_command_capability("logout").expect("logout capability");
        assert_eq!(logout.title, "Credential logout status");
        assert!(logout.read_only);
        assert_eq!(logout.side_effect, SideEffect::ReadOnly);
        assert!(logout.summary.contains("without mutating"));
        assert!(!logout.summary.contains(&obsolete_handoff_label));
    }
}
