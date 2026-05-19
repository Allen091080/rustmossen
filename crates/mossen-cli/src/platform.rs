//! 平台检测与运行时 — 对应 TS 的 platform/ 目录。
//!
//! 检测操作系统、架构、shell 类型和各种运行时能力。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// 平台信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub shell: String,
    pub home_dir: String,
    pub temp_dir: String,
    pub is_wsl: bool,
    pub is_docker: bool,
    pub is_ci: bool,
    pub is_ssh: bool,
    pub terminal: Option<String>,
    pub locale: Option<String>,
}

/// 获取平台信息。
pub fn get_platform_info() -> PlatformInfo {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let shell = std::env::var("SHELL")
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });

    let home_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();

    let is_wsl = std::env::var("WSL_DISTRO_NAME").is_ok()
        || std::env::var("WSLENV").is_ok();

    let is_docker = std::path::Path::new("/.dockerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup")
            .map(|s| s.contains("docker"))
            .unwrap_or(false);

    let is_ci = std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("JENKINS_URL").is_ok()
        || std::env::var("GITLAB_CI").is_ok();

    let is_ssh = std::env::var("SSH_CONNECTION").is_ok()
        || std::env::var("SSH_CLIENT").is_ok();

    let terminal = std::env::var("TERM_PROGRAM")
        .ok()
        .or_else(|| std::env::var("TERMINAL_EMULATOR").ok());

    let locale = std::env::var("LANG")
        .ok()
        .or_else(|| std::env::var("LC_ALL").ok());

    PlatformInfo {
        os,
        arch,
        shell,
        home_dir,
        temp_dir,
        is_wsl,
        is_docker,
        is_ci,
        is_ssh,
        terminal,
        locale,
    }
}

/// 平台运行时快照 — 对应 TS 的 platform/runtime.ts。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformRuntimeSnapshot {
    pub provider: ProviderSnapshot,
    pub direct_connect: DirectConnectSnapshot,
    pub ssh_remote: SshRemoteSnapshot,
    pub system_prompt: SystemPromptSnapshot,
    pub memory: MemorySnapshot,
    pub compression: CompressionSnapshot,
    pub skills: SkillsSnapshot,
    pub plugins: PluginsSnapshot,
    pub mcp: McpSnapshot,
    pub security: SecuritySnapshot,
    pub local_git: LocalGitSnapshot,
    pub remote: RemoteSnapshot,
    pub assistant: AssistantSnapshot,
    pub chrome: ChromeSnapshot,
    pub voice: VoiceSnapshot,
    pub team_memory: TeamMemorySnapshot,
    pub agents: AgentsSnapshot,
    pub sessions: SessionsSnapshot,
    pub swarm: SwarmSnapshot,
    pub feature_gates: FeatureGates,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub protocol: Option<String>,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tool_use: bool,
    pub structured_output: bool,
    pub auth: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirectConnectSnapshot {
    pub feature_enabled: bool,
    pub server_runtime_available: bool,
    pub open_runtime_available: bool,
    pub recoverable_from_local_cache: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SshRemoteSnapshot {
    pub feature_enabled: bool,
    pub local_test_available: bool,
    pub remote_session_available: bool,
    pub recoverable_from_local_cache: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPromptSnapshot {
    pub default_assembly: Vec<String>,
    pub effective_assembly: Option<EffectiveAssembly>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectiveAssembly {
    pub item_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompressionSnapshot {
    pub available: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillsSnapshot {
    pub bundled_registered: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginsSnapshot {
    pub enabled: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpSnapshot {
    pub enterprise_servers: usize,
    pub user_servers: usize,
    pub project_servers: usize,
    pub local_servers: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecuritySnapshot {
    pub default_permission_mode: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalGitSnapshot {
    pub local_git_ready: bool,
    pub local_pr_ready: bool,
    pub gh_installed: bool,
    pub gh_authenticated: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteSnapshot {
    pub bridge_available: bool,
    pub policy_allowed: bool,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssistantSnapshot {
    pub command_exposed: bool,
    pub attach_available: bool,
    pub discovery_available: bool,
    pub discovered_sessions: usize,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChromeSnapshot {
    pub extension_installed: bool,
    pub native_host_installed: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceSnapshot {
    pub visible: bool,
    pub stream_available: bool,
    pub recording_available: bool,
    pub user_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamMemorySnapshot {
    pub enabled: bool,
    pub sync_available: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentsSnapshot {
    pub active: usize,
    pub entrypoint: Option<String>,
    pub includes_code_guide: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionsSnapshot {
    pub project_sessions: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwarmSnapshot {
    pub teammate: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureGates {
    pub direct_connect: bool,
    pub ssh_remote: bool,
    pub kairos: bool,
    pub transcript_classifier: bool,
    pub chicago_mcp: bool,
    pub voice_mode: bool,
    pub daemon: bool,
}

/// 获取平台运行时快照 — 对应 TS 的 getPlatformRuntimeSnapshot()。
pub async fn get_platform_runtime_snapshot(prime: bool) -> PlatformRuntimeSnapshot {
    let mut snapshot = PlatformRuntimeSnapshot::default();

    // Provider capabilities
    snapshot.provider.capabilities.streaming = true;
    snapshot.provider.capabilities.tool_use = true;
    snapshot.provider.capabilities.structured_output = true;
    snapshot.provider.capabilities.auth = true;

    // Git detection
    snapshot.local_git.local_git_ready = which::which("git").is_ok();
    snapshot.local_git.gh_installed = which::which("gh").is_ok();

    if snapshot.local_git.gh_installed {
        // Check if gh is authenticated
        let output = tokio::process::Command::new("gh")
            .args(["auth", "status"])
            .output()
            .await;
        snapshot.local_git.gh_authenticated = output
            .map(|o| o.status.success())
            .unwrap_or(false);
        snapshot.local_git.local_pr_ready =
            snapshot.local_git.gh_installed && snapshot.local_git.gh_authenticated;
    }

    // Memory
    snapshot.memory.enabled = true;

    // Compression
    snapshot.compression.available = true;

    info!(
        git = snapshot.local_git.local_git_ready,
        gh = snapshot.local_git.gh_installed,
        "platform runtime snapshot collected"
    );

    snapshot
}

/// Shell 类型检测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Cmd,
    Unknown(String),
}

/// 检测当前 shell 类型。
pub fn detect_shell() -> ShellType {
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.contains("zsh") {
        ShellType::Zsh
    } else if shell.contains("bash") {
        ShellType::Bash
    } else if shell.contains("fish") {
        ShellType::Fish
    } else if shell.contains("pwsh") || shell.contains("powershell") {
        ShellType::PowerShell
    } else if cfg!(windows) {
        ShellType::Cmd
    } else {
        ShellType::Unknown(shell)
    }
}

/// 获取 shell 的 RC 文件路径。
pub fn get_shell_rc_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    match detect_shell() {
        ShellType::Zsh => Some(home.join(".zshrc")),
        ShellType::Bash => {
            let bashrc = home.join(".bashrc");
            if bashrc.exists() {
                Some(bashrc)
            } else {
                Some(home.join(".bash_profile"))
            }
        }
        ShellType::Fish => Some(home.join(".config/fish/config.fish")),
        _ => None,
    }
}

/// 检测终端是否支持颜色。
pub fn supports_color() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    if std::env::var("FORCE_COLOR").is_ok() {
        return true;
    }
    // Check TERM
    std::env::var("TERM")
        .map(|t| t != "dumb")
        .unwrap_or(false)
}

/// 检测终端是否支持 Unicode。
pub fn supports_unicode() -> bool {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .map(|l| l.to_lowercase().contains("utf"))
        .unwrap_or(false)
}

/// 获取终端列数。
pub fn terminal_columns() -> usize {
    crossterm::terminal::size()
        .map(|(cols, _)| cols as usize)
        .unwrap_or(80)
}

/// 获取终端行数。
pub fn terminal_rows() -> usize {
    crossterm::terminal::size()
        .map(|(_, rows)| rows as usize)
        .unwrap_or(24)
}

// ============================================================================
// platform/runtimeTypes.ts — 完整运行时快照类型
// ============================================================================

/// 能力状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformCapabilityStatus {
    Wired,
    Degraded,
    Disabled,
    SnapshotMissing,
}

/// 能力域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformCapabilityDomain {
    Provider,
    LocalGit,
    DirectConnect,
    SshRemote,
    SystemPrompt,
    Memory,
    Compression,
    Skills,
    Security,
    Plugins,
    Mcp,
    Remote,
    Assistant,
    Chrome,
    Voice,
    TeamMemory,
    Agents,
    Sessions,
    Swarm,
}

/// Provider 协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderProtocol {
    MossenCompatible,
    OpenaiCompatible,
    Private,
}

/// 模型分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Local,
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRuntimeCapabilities {
    pub streaming: bool,
    #[serde(rename = "toolUse")]
    pub tool_use: bool,
    #[serde(rename = "structuredOutput")]
    pub structured_output: bool,
    pub auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRuntimeSnapshot {
    pub kind: String, // 'custom-backend'|'first-party'|'bedrock'|'vertex'|'foundry'
    pub name: String,
    pub tier: ModelTier,
    pub protocol: Option<ProviderProtocol>,
    #[serde(rename = "baseUrl")]
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub capabilities: ProviderRuntimeCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectConnectRuntimeSnapshot {
    pub feature_enabled: bool,
    pub server_command_exposed: bool,
    pub open_command_exposed: bool,
    pub server_runtime_available: bool,
    pub open_runtime_available: bool,
    pub client_session_create_available: bool,
    pub client_session_manager_available: bool,
    pub repl_hook_available: bool,
    pub missing_server_modules: Vec<String>,
    pub missing_open_modules: Vec<String>,
    pub cache_paths_checked: Vec<String>,
    pub cache_paths_present: Vec<String>,
    pub recoverable_source_hits: Vec<String>,
    pub recoverable_from_local_cache: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGitRuntimeSnapshot {
    pub git_installed: bool,
    pub git_path: Option<String>,
    pub gh_installed: bool,
    pub gh_path: Option<String>,
    pub gh_authenticated: bool,
    pub commit_push_pr_command_exposed: bool,
    pub local_git_ready: bool,
    pub local_pr_ready: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshRuntimeSnapshot {
    pub feature_enabled: bool,
    pub command_exposed: bool,
    pub local_test_available: bool,
    pub remote_session_available: bool,
    pub repl_hook_available: bool,
    pub session_factory_available: bool,
    pub session_manager_available: bool,
    pub missing_modules: Vec<String>,
    pub missing_adjacent_modules: Vec<String>,
    pub cache_paths_checked: Vec<String>,
    pub cache_paths_present: Vec<String>,
    pub recoverable_source_hits: Vec<String>,
    pub recoverable_from_local_cache: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptLayerSnapshot {
    pub layer: String,
    pub label: String,
    pub section_names: Vec<String>,
    pub item_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveSystemPromptAssemblySnapshot {
    pub base_source: String, // 'default'|'custom'|...
    pub overlay_sources: Vec<String>,
    pub item_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptRuntimeSnapshot {
    pub default_assembly: Vec<SystemPromptLayerSnapshot>,
    pub effective_assembly: Option<EffectiveSystemPromptAssemblySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRuntimeSnapshot {
    pub enabled: bool,
    pub auto_memory_path: Option<String>,
    pub prompt_loaded: bool,
    pub entrypoint: String,
    pub daily_log_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionRuntimeSnapshot {
    pub available: bool,
    pub post_compact_token_budget: u64,
    pub post_compact_max_files_to_restore: u64,
    pub post_compact_max_tokens_per_file: u64,
    pub invoked_skill_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsRuntimeSnapshot {
    pub bundled_registered: u64,
    pub dynamic_discovered: u64,
    pub conditional_pending: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityRuntimeSnapshot {
    pub default_permission_mode: Option<String>,
    pub available_permission_modes: Vec<String>,
    pub session_trust_accepted: bool,
    pub sandbox_enabled: bool,
    pub unsandboxed_commands_allowed: bool,
    pub bypass_permissions_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginsRuntimeSnapshot {
    pub enabled: u64,
    pub disabled: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeSnapshot {
    pub enterprise_servers: u64,
    pub user_servers: u64,
    pub project_servers: u64,
    pub local_servers: u64,
    pub total_errors: u64,
    pub plugin_only: bool,
    pub managed_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeSnapshot {
    pub policy_allowed: bool,
    pub bridge_available: bool,
    pub disabled_reason: Option<String>,
    pub running_in_remote_session: bool,
    pub remote_environment_type: Option<String>,
    pub teleported_session: bool,
    pub teleported_session_id: Option<String>,
    pub unix_socket_auth_proxy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRuntimeSnapshot {
    pub feature_enabled: bool,
    pub command_exposed: bool,
    pub discovery_available: bool,
    pub discovered_sessions: u64,
    pub attach_available: bool,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeRuntimeSnapshot {
    pub cli_override: Option<bool>,
    pub should_enable: bool,
    pub auto_enable: bool,
    pub extension_installed: bool,
    pub native_host_installed: bool,
    pub native_host_wrapper_exists: bool,
    pub native_host_manifest_count: u64,
    pub install_url: Option<String>,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRuntimeSnapshot {
    pub visible: bool,
    pub growthbook_enabled: bool,
    pub auth_available: bool,
    pub stream_available: bool,
    pub recording_available: bool,
    pub recording_reason: Option<String>,
    pub user_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemoryRuntimeSnapshot {
    pub build_enabled: bool,
    pub enabled: bool,
    pub sync_available: bool,
    pub auto_memory_enabled: bool,
    pub rollout_enabled: bool,
    pub path: Option<String>,
    pub entrypoint: Option<String>,
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsRuntimeSnapshot {
    pub entrypoint: Option<String>,
    pub active: u64,
    pub total: u64,
    pub parse_errors: u64,
    pub includes_code_guide: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsRuntimeSnapshot {
    pub current_transcript_path: String,
    pub project_sessions: u64,
    pub projects_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwarmRuntimeSnapshot {
    pub teammate: bool,
    pub team_name: Option<String>,
    pub agent_name: Option<String>,
    pub session_created_teams: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureGatesRuntimeSnapshot {
    pub direct_connect: bool,
    pub ssh_remote: bool,
    pub kairos: bool,
    pub kairos_brief: bool,
    pub transcript_classifier: bool,
    pub chicago_mcp: bool,
    pub voice_mode: bool,
    pub daemon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCapabilityManifestEntry {
    pub id: PlatformCapabilityDomain,
    pub title: String,
    pub status: PlatformCapabilityStatus,
    pub modules: Vec<String>,
    pub validation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformRuntimeSnapshotFull {
    pub provider: ProviderRuntimeSnapshot,
    pub local_git: LocalGitRuntimeSnapshot,
    pub direct_connect: DirectConnectRuntimeSnapshot,
    pub ssh_remote: SshRuntimeSnapshot,
    pub system_prompt: SystemPromptRuntimeSnapshot,
    pub memory: MemoryRuntimeSnapshot,
    pub compression: CompressionRuntimeSnapshot,
    pub skills: SkillsRuntimeSnapshot,
    pub security: SecurityRuntimeSnapshot,
    pub plugins: PluginsRuntimeSnapshot,
    pub mcp: McpRuntimeSnapshot,
    pub remote: RemoteRuntimeSnapshot,
    pub assistant: AssistantRuntimeSnapshot,
    pub chrome: ChromeRuntimeSnapshot,
    pub voice: VoiceRuntimeSnapshot,
    pub team_memory: TeamMemoryRuntimeSnapshot,
    pub agents: AgentsRuntimeSnapshot,
    pub sessions: SessionsRuntimeSnapshot,
    pub swarm: SwarmRuntimeSnapshot,
    pub feature_gates: FeatureGatesRuntimeSnapshot,
    pub manifest: Vec<PlatformCapabilityManifestEntry>,
}
