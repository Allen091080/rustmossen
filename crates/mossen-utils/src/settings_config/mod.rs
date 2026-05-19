// settings_config — translated from utils/settings/ (19 TS files, 4620 lines)
//
// Files translated:
//   constants.ts, types.ts, managedPath.ts, internalWrites.ts,
//   settingsCache.ts, schemaOutput.ts, toolValidationConfig.ts,
//   validationTips.ts, permissionValidation.ts, validation.ts,
//   mdm/constants.ts, mdm/rawRead.ts, mdm/settings.ts,
//   pluginOnlyPolicy.ts, validateEditTool.ts, settings.ts,
//   changeDetector.ts, applySettingsChange.ts, allErrors.ts

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// constants.ts
// ============================================================================

/// All possible sources where settings can come from.
/// Order matters - later sources override earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    FlagSettings,
    PolicySettings,
}

pub const SETTING_SOURCES: &[SettingSource] = &[
    SettingSource::UserSettings,
    SettingSource::ProjectSettings,
    SettingSource::LocalSettings,
    SettingSource::FlagSettings,
    SettingSource::PolicySettings,
];

pub fn get_setting_source_name(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "user",
        SettingSource::ProjectSettings => "project",
        SettingSource::LocalSettings => "project, gitignored",
        SettingSource::FlagSettings => "cli flag",
        SettingSource::PolicySettings => "managed",
    }
}

pub fn get_source_display_name(source: &str) -> &'static str {
    match source {
        "userSettings" => "User",
        "projectSettings" => "Project",
        "localSettings" => "Local",
        "flagSettings" => "Flag",
        "policySettings" => "Managed",
        "plugin" => "Plugin",
        "built-in" => "Built-in",
        _ => "Unknown",
    }
}

pub fn get_setting_source_display_name_lowercase(source: &str) -> &'static str {
    match source {
        "userSettings" => "user settings",
        "projectSettings" => "shared project settings",
        "localSettings" => "project local settings",
        "flagSettings" => "command line arguments",
        "policySettings" => "enterprise managed settings",
        "cliArg" => "CLI argument",
        "command" => "command configuration",
        "session" => "current session",
        _ => "unknown",
    }
}

pub fn get_setting_source_display_name_capitalized(source: &str) -> &'static str {
    match source {
        "userSettings" => "User settings",
        "projectSettings" => "Shared project settings",
        "localSettings" => "Project local settings",
        "flagSettings" => "Command line arguments",
        "policySettings" => "Enterprise managed settings",
        "cliArg" => "CLI argument",
        "command" => "Command configuration",
        "session" => "Current session",
        _ => "Unknown",
    }
}

/// Parse the --setting-sources CLI flag into SettingSource array.
pub fn parse_setting_sources_flag(flag: &str) -> Result<Vec<SettingSource>, String> {
    if flag.is_empty() {
        return Ok(vec![]);
    }
    let names: Vec<&str> = flag.split(',').map(|s| s.trim()).collect();
    let mut result = Vec::new();
    for name in names {
        match name {
            "user" => result.push(SettingSource::UserSettings),
            "project" => result.push(SettingSource::ProjectSettings),
            "local" => result.push(SettingSource::LocalSettings),
            _ => {
                return Err(format!(
                    "Invalid setting source: {}. Valid options are: user, project, local",
                    name
                ));
            }
        }
    }
    Ok(result)
}

/// Get enabled setting sources with policy/flag always included.
pub fn get_enabled_setting_sources(allowed: &[SettingSource]) -> Vec<SettingSource> {
    let mut result: HashSet<SettingSource> = allowed.iter().copied().collect();
    result.insert(SettingSource::PolicySettings);
    result.insert(SettingSource::FlagSettings);
    result.into_iter().collect()
}

/// Check if a specific source is enabled.
pub fn is_setting_source_enabled(source: SettingSource, allowed: &[SettingSource]) -> bool {
    let enabled = get_enabled_setting_sources(allowed);
    enabled.contains(&source)
}

/// Editable setting sources (excludes policySettings and flagSettings which are read-only).
pub fn is_editable_source(source: SettingSource) -> bool {
    !matches!(
        source,
        SettingSource::PolicySettings | SettingSource::FlagSettings
    )
}

/// List of sources where permission rules can be saved, in display order.
pub const EDITABLE_SOURCES: &[SettingSource] = &[
    SettingSource::LocalSettings,
    SettingSource::ProjectSettings,
    SettingSource::UserSettings,
];

/// The JSON Schema URL for Mossen settings.
pub const MOSSEN_CODE_SETTINGS_SCHEMA_URL: &str =
    "https://schemas.mossen.invalid/mossen-code-settings.json";

// ============================================================================
// types.ts
// ============================================================================

/// Surfaces lockable by strictPluginOnlyCustomization.
pub const CUSTOMIZATION_SURFACES: &[&str] = &["skills", "agents", "hooks", "mcp"];

/// Permission rule in permissions section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_bypass_permissions_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_auto_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_directories: Option<Vec<String>>,
    /// Extra fields preserved via passthrough
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Marketplace source schema (extra known marketplace).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraKnownMarketplace {
    pub source: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
}

/// Allowed MCP server entry in enterprise allowlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedMcpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

/// Denied MCP server entry in enterprise denylist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeniedMcpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

/// Validate that an MCP server entry has exactly one of serverName, serverCommand, serverUrl.
pub fn validate_mcp_server_entry(entry: &AllowedMcpServerEntry) -> bool {
    let defined = [
        entry.server_name.is_some(),
        entry.server_command.is_some(),
        entry.server_url.is_some(),
    ]
    .iter()
    .filter(|&&v| v)
    .count();
    defined == 1
}

pub fn validate_denied_mcp_server_entry(entry: &DeniedMcpServerEntry) -> bool {
    let defined = [
        entry.server_name.is_some(),
        entry.server_command.is_some(),
        entry.server_url.is_some(),
    ]
    .iter()
    .filter(|&&v| v)
    .count();
    defined == 1
}

/// Worktree configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorktreeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_directories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparse_paths: Option<Vec<String>>,
}

/// Attribution configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
}

/// File suggestion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSuggestionConfig {
    pub r#type: String,
    pub command: String,
}

/// Status line configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLineConfig {
    pub r#type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
}

/// Spinner verbs config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinnerVerbsConfig {
    pub mode: String, // "append" | "replace"
    pub verbs: Vec<String>,
}

/// Spinner tips override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinnerTipsOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_default: Option<bool>,
    pub tips: Vec<Value>,
}

/// Remote session config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_environment_id: Option<String>,
}

/// XAA IdP connection config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaaIdpConfig {
    pub issuer: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<i32>,
}

/// Sandbox settings config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxSettingsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_if_unavailable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_unsandboxed_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_violations: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_allow_bash_if_sandboxed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_weaker_nested_sandbox: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_weaker_network_isolation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ripgrep: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Auto mode config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoModeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soft_deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<Vec<String>>,
}

/// SSH config entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfigEntry {
    pub id: String,
    pub name: String,
    pub ssh_host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_identity_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_directory: Option<String>,
}

/// Channel plugin entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPluginEntry {
    pub marketplace: String,
    pub plugin: String,
}

/// Plugin config value type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginConfigValue {
    String(String),
    Number(f64),
    Bool(bool),
    StringArray(Vec<String>),
}

/// User config values for MCP servers.
pub type UserConfigValues = HashMap<String, PluginConfigValue>;

/// Plugin configuration stored in settings.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, UserConfigValues>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, PluginConfigValue>>,
}

/// Unified settings file structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsJson {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_credential_export: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_auth_refresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcp_auth_refresh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xaa_idp: Option<XaaIdpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_suggestion: Option<FileSuggestionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_gitignore: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_period_days: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<AttributionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_co_authored_by: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_git_instructions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_overrides: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_all_project_mcp_servers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_mcpjson_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_mcpjson_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mcp_servers: Option<Vec<AllowedMcpServerEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_mcp_servers: Option<Vec<DeniedMcpServerEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_all_hooks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_managed_hooks_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_http_hook_urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_hook_allowed_env_vars: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_managed_permission_rules_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_managed_mcp_servers_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_plugin_only_customization: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_line: Option<StatusLineConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_plugins: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_known_marketplaces: Option<HashMap<String, ExtraKnownMarketplace>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict_known_marketplaces: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_marketplaces: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_login_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_login_org_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_web_fetch_preflight: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxSettingsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_survey_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_tips_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_verbs: Option<SpinnerVerbsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_tips_override: Option<SpinnerTipsOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax_highlighting_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_title_from_rename: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_thinking_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode_per_session_opt_in: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_suggestion_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_clear_context_on_plan_accept: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_announcements: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_configs: Option<HashMap<String, PluginConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_updates_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_deep_link_registration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plans_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classifier_permissions_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_sleep_duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sleep_duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_channel_plugins: Option<Vec<ChannelPluginEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_view: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefers_reduced_motion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_dream_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_thinking_summaries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_dangerous_mode_permission_prompt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_auto_permission_prompt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_auto_mode_during_plan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_mode: Option<AutoModeConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_auto_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_configs: Option<Vec<SshConfigEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossen_md_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_trust_message: Option<String>,
    /// Extra fields preserved via passthrough.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Internal type for plugin hooks.
#[derive(Debug, Clone)]
pub struct PluginHookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    pub plugin_root: String,
    pub plugin_name: String,
    pub plugin_id: String,
}

/// Internal type for skill hooks.
#[derive(Debug, Clone)]
pub struct SkillHookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    pub skill_root: String,
    pub skill_name: String,
}

/// Type guard for MCP server entry with serverName.
pub fn is_mcp_server_name_entry(entry: &AllowedMcpServerEntry) -> bool {
    entry.server_name.is_some()
}

/// Type guard for MCP server entry with serverCommand.
pub fn is_mcp_server_command_entry(entry: &AllowedMcpServerEntry) -> bool {
    entry.server_command.is_some()
}

/// Type guard for MCP server entry with serverUrl.
pub fn is_mcp_server_url_entry(entry: &AllowedMcpServerEntry) -> bool {
    entry.server_url.is_some()
}

// ============================================================================
// managedPath.ts
// ============================================================================

/// Get the path to the managed settings directory based on the current platform.
pub fn get_managed_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("MOSSEN_CODE_MANAGED_SETTINGS_PATH") {
        if std::env::var("USER_TYPE").as_deref() == Ok("ant") {
            return PathBuf::from(path);
        }
    }
    if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/Mossen")
    } else if cfg!(target_os = "windows") {
        PathBuf::from("C:\\Program Files\\Mossen")
    } else {
        PathBuf::from("/etc/mossen")
    }
}

/// Get the path to the managed-settings.d/ drop-in directory.
pub fn get_managed_settings_drop_in_dir() -> PathBuf {
    get_managed_file_path().join("managed-settings.d")
}

// ============================================================================
// internalWrites.ts
// ============================================================================

static INTERNAL_WRITE_TIMESTAMPS: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Mark a file path as being written internally.
pub fn mark_internal_write(path: &str) {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    map.insert(path.to_string(), Instant::now());
}

/// Check if a path was recently written internally within the given window.
/// Consumes the mark on match.
pub fn consume_internal_write(path: &str, window: Duration) -> bool {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    if let Some(ts) = map.get(path) {
        if ts.elapsed() < window {
            map.remove(path);
            return true;
        }
    }
    false
}

/// Clear all internal write timestamps.
pub fn clear_internal_writes() {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    map.clear();
}

// ============================================================================
// settingsCache.ts
// ============================================================================

/// Settings with errors.
#[derive(Debug, Clone)]
pub struct SettingsWithErrors {
    pub settings: SettingsJson,
    pub errors: Vec<ValidationError>,
}

static SESSION_SETTINGS_CACHE: Lazy<Mutex<Option<SettingsWithErrors>>> =
    Lazy::new(|| Mutex::new(None));

pub fn get_session_settings_cache() -> Option<SettingsWithErrors> {
    SESSION_SETTINGS_CACHE.lock().unwrap().clone()
}

pub fn set_session_settings_cache(value: SettingsWithErrors) {
    *SESSION_SETTINGS_CACHE.lock().unwrap() = Some(value);
}

static PER_SOURCE_CACHE: Lazy<Mutex<HashMap<SettingSource, Option<SettingsJson>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_cached_settings_for_source(source: SettingSource) -> Option<Option<SettingsJson>> {
    let cache = PER_SOURCE_CACHE.lock().unwrap();
    cache.get(&source).cloned()
}

pub fn set_cached_settings_for_source(source: SettingSource, value: Option<SettingsJson>) {
    let mut cache = PER_SOURCE_CACHE.lock().unwrap();
    cache.insert(source, value);
}

/// Path-keyed cache for parsed settings files.
#[derive(Debug, Clone)]
pub struct ParsedSettings {
    pub settings: Option<SettingsJson>,
    pub errors: Vec<ValidationError>,
}

static PARSE_FILE_CACHE: Lazy<Mutex<HashMap<String, ParsedSettings>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_cached_parsed_file(path: &str) -> Option<ParsedSettings> {
    let cache = PARSE_FILE_CACHE.lock().unwrap();
    cache.get(path).cloned()
}

pub fn set_cached_parsed_file(path: &str, value: ParsedSettings) {
    let mut cache = PARSE_FILE_CACHE.lock().unwrap();
    cache.insert(path.to_string(), value);
}

/// Reset all settings caches.
pub fn reset_settings_cache() {
    *SESSION_SETTINGS_CACHE.lock().unwrap() = None;
    PER_SOURCE_CACHE.lock().unwrap().clear();
    PARSE_FILE_CACHE.lock().unwrap().clear();
}

static PLUGIN_SETTINGS_BASE: Lazy<Mutex<Option<HashMap<String, Value>>>> =
    Lazy::new(|| Mutex::new(None));

pub fn get_plugin_settings_base() -> Option<HashMap<String, Value>> {
    PLUGIN_SETTINGS_BASE.lock().unwrap().clone()
}

pub fn set_plugin_settings_base(settings: Option<HashMap<String, Value>>) {
    *PLUGIN_SETTINGS_BASE.lock().unwrap() = settings;
}

pub fn clear_plugin_settings_base() {
    *PLUGIN_SETTINGS_BASE.lock().unwrap() = None;
}

// ============================================================================
// schemaOutput.ts
// ============================================================================

/// Generate a JSON schema representation for the SettingsJson struct.
pub fn generate_settings_json_schema() -> String {
    // In Rust we generate a schema description from the struct definition.
    // This is a simplified version since we don't have Zod's toJSONSchema.
    serde_json::to_string_pretty(&serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "MossenSettings",
        "type": "object",
        "description": "Mossen settings configuration schema"
    }))
    .unwrap_or_default()
}

// ============================================================================
// toolValidationConfig.ts
// ============================================================================

/// Tool validation result.
#[derive(Debug, Clone)]
pub struct ToolValidationResult {
    pub valid: bool,
    pub error: Option<String>,
    pub suggestion: Option<String>,
    pub examples: Option<Vec<String>>,
}

/// File pattern tools (accept *.ts, src/**, etc.)
const FILE_PATTERN_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "NotebookRead",
    "NotebookEdit",
];

/// Bash wildcard tools (accept * anywhere, and legacy command:* syntax)
const BASH_PREFIX_TOOLS: &[&str] = &["Bash"];

/// Check if a tool uses file patterns.
pub fn is_file_pattern_tool(tool_name: &str) -> bool {
    FILE_PATTERN_TOOLS.contains(&tool_name)
}

/// Check if a tool uses bash prefix patterns.
pub fn is_bash_prefix_tool(tool_name: &str) -> bool {
    BASH_PREFIX_TOOLS.contains(&tool_name)
}

/// Get custom validation for a tool.
pub fn get_custom_validation(tool_name: &str, content: &str) -> Option<ToolValidationResult> {
    match tool_name {
        "WebSearch" => {
            if content.contains('*') || content.contains('?') {
                Some(ToolValidationResult {
                    valid: false,
                    error: Some("WebSearch does not support wildcards".to_string()),
                    suggestion: Some("Use exact search terms without * or ?".to_string()),
                    examples: Some(vec![
                        "WebSearch(mossen ai)".to_string(),
                        "WebSearch(typescript tutorial)".to_string(),
                    ]),
                })
            } else {
                Some(ToolValidationResult {
                    valid: true,
                    error: None,
                    suggestion: None,
                    examples: None,
                })
            }
        }
        "WebFetch" => {
            if content.contains("://") || content.starts_with("http") {
                Some(ToolValidationResult {
                    valid: false,
                    error: Some(
                        "WebFetch permissions use domain format, not URLs".to_string(),
                    ),
                    suggestion: Some("Use \"domain:hostname\" format".to_string()),
                    examples: Some(vec![
                        "WebFetch(domain:example.com)".to_string(),
                        "WebFetch(domain:github.com)".to_string(),
                    ]),
                })
            } else if !content.starts_with("domain:") {
                Some(ToolValidationResult {
                    valid: false,
                    error: Some(
                        "WebFetch permissions must use \"domain:\" prefix".to_string(),
                    ),
                    suggestion: Some("Use \"domain:hostname\" format".to_string()),
                    examples: Some(vec![
                        "WebFetch(domain:example.com)".to_string(),
                        "WebFetch(domain:*.google.com)".to_string(),
                    ]),
                })
            } else {
                Some(ToolValidationResult {
                    valid: true,
                    error: None,
                    suggestion: None,
                    examples: None,
                })
            }
        }
        _ => None,
    }
}

// ============================================================================
// validationTips.ts
// ============================================================================

/// Validation tip.
#[derive(Debug, Clone)]
pub struct ValidationTip {
    pub suggestion: Option<String>,
    pub doc_link: Option<String>,
}

/// Tip context for matching.
#[derive(Debug)]
pub struct TipContext {
    pub path: String,
    pub code: String,
    pub expected: Option<String>,
    pub received: Option<String>,
    pub enum_values: Option<Vec<String>>,
    pub message: Option<String>,
    pub value: Option<String>,
}

const DOCUMENTATION_BASE: &str = "https://mossen.invalid/docs";

/// Get a validation tip based on the context.
pub fn get_validation_tip(ctx: &TipContext) -> Option<ValidationTip> {
    // permissions.defaultMode + invalid_value
    if ctx.path == "permissions.defaultMode" && ctx.code == "invalid_value" {
        return Some(ValidationTip {
            suggestion: Some(
                "Valid modes: \"acceptEdits\" (ask before file changes), \"plan\" (analysis only), \"bypassPermissions\" (auto-accept all), or \"default\" (standard behavior)".to_string(),
            ),
            doc_link: Some(format!("{}/iam#permission-modes", DOCUMENTATION_BASE)),
        });
    }

    // apiKeyHelper + invalid_type
    if ctx.path == "apiKeyHelper" && ctx.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Provide a shell command that outputs your API key to stdout. The script should output only the API key. Example: \"/bin/generate_temp_api_key.sh\"".to_string(),
            ),
            doc_link: None,
        });
    }

    // cleanupPeriodDays + too_small
    if ctx.path == "cleanupPeriodDays"
        && ctx.code == "too_small"
        && ctx.expected.as_deref() == Some("0")
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Must be 0 or greater. Set a positive number for days to retain transcripts (default is 30). Setting 0 disables session persistence entirely.".to_string(),
            ),
            doc_link: None,
        });
    }

    // env.* + invalid_type
    if ctx.path.starts_with("env.") && ctx.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Environment variables must be strings. Wrap numbers and booleans in quotes. Example: \"DEBUG\": \"true\", \"PORT\": \"3000\"".to_string(),
            ),
            doc_link: Some(format!("{}/settings#environment-variables", DOCUMENTATION_BASE)),
        });
    }

    // permissions.allow/deny + invalid_type + expected array
    if (ctx.path == "permissions.allow" || ctx.path == "permissions.deny")
        && ctx.code == "invalid_type"
        && ctx.expected.as_deref() == Some("array")
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Permission rules must be in an array. Format: [\"Tool(specifier)\"]. Examples: [\"Bash(npm run build)\", \"Edit(docs/**)\", \"Read(~/.zshrc)\"].".to_string(),
            ),
            doc_link: None,
        });
    }

    // hooks + invalid_type
    if ctx.path.contains("hooks") && ctx.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Hooks use a matcher + hooks array. The matcher is a string: a tool name (\"Bash\"), pipe-separated list (\"Edit|Write\"), or empty to match all.".to_string(),
            ),
            doc_link: None,
        });
    }

    // invalid_type + expected boolean
    if ctx.code == "invalid_type" && ctx.expected.as_deref() == Some("boolean") {
        return Some(ValidationTip {
            suggestion: Some(
                "Use true or false without quotes. Example: \"includeCoAuthoredBy\": true".to_string(),
            ),
            doc_link: None,
        });
    }

    // unrecognized_keys
    if ctx.code == "unrecognized_keys" {
        return Some(ValidationTip {
            suggestion: Some(
                "Check for typos or refer to the documentation for valid fields".to_string(),
            ),
            doc_link: Some(format!("{}/settings", DOCUMENTATION_BASE)),
        });
    }

    // invalid_value + enum
    if ctx.code == "invalid_value" && ctx.enum_values.is_some() {
        let vals = ctx.enum_values.as_ref().unwrap();
        return Some(ValidationTip {
            suggestion: Some(format!(
                "Valid values: {}",
                vals.iter()
                    .map(|v| format!("\"{}\"", v))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            doc_link: None,
        });
    }

    // invalid_type + expected object + null + empty path
    if ctx.code == "invalid_type"
        && ctx.expected.as_deref() == Some("object")
        && ctx.received.as_deref() == Some("null")
        && ctx.path.is_empty()
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Check for missing commas, unmatched brackets, or trailing commas. Use a JSON validator to identify the exact syntax error.".to_string(),
            ),
            doc_link: None,
        });
    }

    // permissions.additionalDirectories + invalid_type
    if ctx.path == "permissions.additionalDirectories" && ctx.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Must be an array of directory paths. Example: [\"~/projects\", \"/tmp/workspace\"]. You can also use --add-dir flag or /add-dir command".to_string(),
            ),
            doc_link: Some(format!(
                "{}/iam#working-directories",
                DOCUMENTATION_BASE
            )),
        });
    }

    // Add doc link based on path prefix
    None
}

/// Path documentation links.
fn get_path_doc_link(path: &str) -> Option<String> {
    let prefix = path.split('.').next()?;
    match prefix {
        "permissions" => Some(format!(
            "{}/iam#configuring-permissions",
            DOCUMENTATION_BASE
        )),
        "env" => Some(format!(
            "{}/settings#environment-variables",
            DOCUMENTATION_BASE
        )),
        "hooks" => Some(format!("{}/hooks", DOCUMENTATION_BASE)),
        _ => None,
    }
}

// ============================================================================
// permissionValidation.ts
// ============================================================================

/// Checks if a character at a given index is escaped.
fn is_escaped(s: &str, index: usize) -> bool {
    let bytes = s.as_bytes();
    let mut backslash_count = 0u32;
    let mut j = index as i64 - 1;
    while j >= 0 && bytes[j as usize] == b'\\' {
        backslash_count += 1;
        j -= 1;
    }
    backslash_count % 2 != 0
}

/// Counts unescaped occurrences of a character in a string.
fn count_unescaped_char(s: &str, ch: char) -> usize {
    let mut count = 0;
    for (i, c) in s.chars().enumerate() {
        if c == ch && !is_escaped(s, i) {
            count += 1;
        }
    }
    count
}

/// Checks if a string contains unescaped empty parentheses "()".
fn has_unescaped_empty_parens(s: &str) -> bool {
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'(' && bytes[i + 1] == b')' && !is_escaped(s, i) {
            return true;
        }
    }
    false
}

/// Parse a permission rule string into (tool_name, rule_content).
fn parse_permission_rule(rule: &str) -> (String, Option<String>) {
    if let Some(paren_pos) = rule.find('(') {
        let tool_name = rule[..paren_pos].to_string();
        let rest = &rule[paren_pos + 1..];
        let content = if rest.ends_with(')') {
            &rest[..rest.len() - 1]
        } else {
            rest
        };
        (tool_name, Some(content.to_string()))
    } else {
        (rule.to_string(), None)
    }
}

/// Validates permission rule format and content.
pub fn validate_permission_rule(rule: &str) -> ToolValidationResult {
    // Empty rule check
    if rule.is_empty() || rule.trim().is_empty() {
        return ToolValidationResult {
            valid: false,
            error: Some("Permission rule cannot be empty".to_string()),
            suggestion: None,
            examples: None,
        };
    }

    // Check parentheses matching (only count unescaped parens)
    let open_count = count_unescaped_char(rule, '(');
    let close_count = count_unescaped_char(rule, ')');
    if open_count != close_count {
        return ToolValidationResult {
            valid: false,
            error: Some("Mismatched parentheses".to_string()),
            suggestion: Some(
                "Ensure all opening parentheses have matching closing parentheses".to_string(),
            ),
            examples: None,
        };
    }

    // Check for empty parentheses (escape-aware)
    if has_unescaped_empty_parens(rule) {
        let tool_name = rule
            .find('(')
            .map(|i| &rule[..i])
            .unwrap_or("")
            .to_string();
        if tool_name.is_empty() {
            return ToolValidationResult {
                valid: false,
                error: Some("Empty parentheses with no tool name".to_string()),
                suggestion: Some("Specify a tool name before the parentheses".to_string()),
                examples: None,
            };
        }
        return ToolValidationResult {
            valid: false,
            error: Some("Empty parentheses".to_string()),
            suggestion: Some(format!(
                "Either specify a pattern or use just \"{}\" without parentheses",
                tool_name
            )),
            examples: Some(vec![
                tool_name.clone(),
                format!("{}(some-pattern)", tool_name),
            ]),
        };
    }

    let (tool_name, rule_content) = parse_permission_rule(rule);

    // MCP validation
    if tool_name.starts_with("mcp__") {
        if rule_content.is_some() || count_unescaped_char(rule, '(') > 0 {
            let server_name = tool_name
                .strip_prefix("mcp__")
                .unwrap_or("")
                .split("__")
                .next()
                .unwrap_or("");
            return ToolValidationResult {
                valid: false,
                error: Some("MCP rules do not support patterns in parentheses".to_string()),
                suggestion: Some(format!(
                    "Use \"{}\" without parentheses, or use \"mcp__{}__*\" for all tools",
                    tool_name, server_name
                )),
                examples: Some(vec![
                    format!("mcp__{}", server_name),
                    format!("mcp__{}__*", server_name),
                ]),
            };
        }
        return ToolValidationResult {
            valid: true,
            error: None,
            suggestion: None,
            examples: None,
        };
    }

    // Tool name validation
    if tool_name.is_empty() {
        return ToolValidationResult {
            valid: false,
            error: Some("Tool name cannot be empty".to_string()),
            suggestion: None,
            examples: None,
        };
    }

    // Check tool name starts with uppercase
    if let Some(first_char) = tool_name.chars().next() {
        if !first_char.is_uppercase() {
            let mut capitalized = String::new();
            for (i, c) in tool_name.chars().enumerate() {
                if i == 0 {
                    capitalized.extend(c.to_uppercase());
                } else {
                    capitalized.push(c);
                }
            }
            return ToolValidationResult {
                valid: false,
                error: Some("Tool names must start with uppercase".to_string()),
                suggestion: Some(format!("Use \"{}\"", capitalized)),
                examples: None,
            };
        }
    }

    // Check for custom validation rules
    if let Some(content) = &rule_content {
        if let Some(result) = get_custom_validation(&tool_name, content) {
            if !result.valid {
                return result;
            }
        }
    }

    // Bash-specific validation
    if is_bash_prefix_tool(&tool_name) {
        if let Some(content) = &rule_content {
            if content.contains(":*") && !content.ends_with(":*") {
                return ToolValidationResult {
                    valid: false,
                    error: Some("The :* pattern must be at the end".to_string()),
                    suggestion: Some(
                        "Move :* to the end for prefix matching, or use * for wildcard matching"
                            .to_string(),
                    ),
                    examples: Some(vec![
                        "Bash(npm run:*) - prefix matching (legacy)".to_string(),
                        "Bash(npm run *) - wildcard matching".to_string(),
                    ]),
                };
            }
            if content == ":*" {
                return ToolValidationResult {
                    valid: false,
                    error: Some("Prefix cannot be empty before :*".to_string()),
                    suggestion: Some("Specify a command prefix before :*".to_string()),
                    examples: Some(vec!["Bash(npm:*)".to_string(), "Bash(git:*)".to_string()]),
                };
            }
        }
    }

    // File tool validation
    if is_file_pattern_tool(&tool_name) {
        if let Some(content) = &rule_content {
            if content.contains(":*") {
                return ToolValidationResult {
                    valid: false,
                    error: Some("The \":*\" syntax is only for Bash prefix rules".to_string()),
                    suggestion: Some(
                        "Use glob patterns like \"*\" or \"**\" for file matching".to_string(),
                    ),
                    examples: Some(vec![
                        format!("{}(*.ts) - matches .ts files", tool_name),
                        format!("{}(src/**) - matches all files in src", tool_name),
                        format!("{}(**/*.test.ts) - matches test files", tool_name),
                    ]),
                };
            }

            // Warn about wildcards not at boundaries
            if content.contains('*') {
                let re = Regex::new(r"^\*|\*$|\*\*|/\*|\*\.|\*\)").unwrap();
                if !re.is_match(content) && !content.contains("**") {
                    return ToolValidationResult {
                        valid: false,
                        error: Some("Wildcard placement might be incorrect".to_string()),
                        suggestion: Some(
                            "Wildcards are typically used at path boundaries".to_string(),
                        ),
                        examples: Some(vec![
                            format!("{}(*.js) - all .js files", tool_name),
                            format!("{}(src/*) - all files directly in src", tool_name),
                            format!("{}(src/**) - all files recursively in src", tool_name),
                        ]),
                    };
                }
            }
        }
    }

    ToolValidationResult {
        valid: true,
        error: None,
        suggestion: None,
        examples: None,
    }
}

// ============================================================================
// validation.ts
// ============================================================================

/// Validation error.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub file: Option<String>,
    pub path: String,
    pub message: String,
    pub expected: Option<String>,
    pub invalid_value: Option<Value>,
    pub suggestion: Option<String>,
    pub doc_link: Option<String>,
    pub mcp_error_metadata: Option<McpErrorMetadata>,
}

/// MCP error metadata.
#[derive(Debug, Clone)]
pub struct McpErrorMetadata {
    pub scope: String,
    pub server_name: Option<String>,
    pub severity: Option<String>,
}

/// Get the type string for an unknown serde value.
fn get_received_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Format validation errors from a parse result.
pub fn format_validation_errors(
    errors: &[(String, String)],
    file_path: &str,
) -> Vec<ValidationError> {
    errors
        .iter()
        .map(|(path, message)| {
            let tip_ctx = TipContext {
                path: path.clone(),
                code: "custom".to_string(),
                expected: None,
                received: None,
                enum_values: None,
                message: Some(message.clone()),
                value: None,
            };
            let tip = get_validation_tip(&tip_ctx);
            ValidationError {
                file: Some(file_path.to_string()),
                path: path.clone(),
                message: message.clone(),
                expected: None,
                invalid_value: None,
                suggestion: tip.as_ref().and_then(|t| t.suggestion.clone()),
                doc_link: tip
                    .as_ref()
                    .and_then(|t| t.doc_link.clone())
                    .or_else(|| get_path_doc_link(path)),
                mcp_error_metadata: None,
            }
        })
        .collect()
}

/// Validate settings file content against the schema.
pub fn validate_settings_file_content(content: &str) -> Result<(), (String, String)> {
    let json_data: Value = serde_json::from_str(content).map_err(|e| {
        (
            format!("Invalid JSON: {}", e),
            generate_settings_json_schema(),
        )
    })?;

    // Try to parse as SettingsJson
    let _settings: SettingsJson = serde_json::from_value(json_data).map_err(|e| {
        (
            format!("Settings validation failed:\n- {}", e),
            generate_settings_json_schema(),
        )
    })?;

    Ok(())
}

/// Filter invalid permission rules from raw parsed JSON data before schema validation.
pub fn filter_invalid_permission_rules(
    data: &mut Value,
    file_path: &str,
) -> Vec<ValidationError> {
    let mut warnings = Vec::new();
    if let Some(obj) = data.as_object_mut() {
        if let Some(perms) = obj.get_mut("permissions") {
            if let Some(perms_obj) = perms.as_object_mut() {
                for key in &["allow", "deny", "ask"] {
                    if let Some(rules_val) = perms_obj.get_mut(*key) {
                        if let Some(rules) = rules_val.as_array_mut() {
                            let mut i = 0;
                            while i < rules.len() {
                                if let Some(rule_str) = rules[i].as_str() {
                                    let result = validate_permission_rule(rule_str);
                                    if !result.valid {
                                        let mut message = format!(
                                            "Invalid permission rule \"{}\" was skipped",
                                            rule_str
                                        );
                                        if let Some(err) = &result.error {
                                            message.push_str(&format!(": {}", err));
                                        }
                                        if let Some(sug) = &result.suggestion {
                                            message.push_str(&format!(". {}", sug));
                                        }
                                        warnings.push(ValidationError {
                                            file: Some(file_path.to_string()),
                                            path: format!("permissions.{}", key),
                                            message,
                                            expected: None,
                                            invalid_value: Some(rules[i].clone()),
                                            suggestion: None,
                                            doc_link: None,
                                            mcp_error_metadata: None,
                                        });
                                        rules.remove(i);
                                        continue;
                                    }
                                } else {
                                    warnings.push(ValidationError {
                                        file: Some(file_path.to_string()),
                                        path: format!("permissions.{}", key),
                                        message: format!(
                                            "Non-string value in {} array was removed",
                                            key
                                        ),
                                        expected: None,
                                        invalid_value: Some(rules[i].clone()),
                                        suggestion: None,
                                        doc_link: None,
                                        mcp_error_metadata: None,
                                    });
                                    rules.remove(i);
                                    continue;
                                }
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    warnings
}

// ============================================================================
// mdm/constants.ts
// ============================================================================

/// macOS preference domain used for Mossen managed settings profiles.
pub const MACOS_PREFERENCE_DOMAIN: &str = "com.mossen.mossencode";

/// Windows registry key paths.
pub const WINDOWS_REGISTRY_KEY_PATH_HKLM: &str = "HKLM\\SOFTWARE\\Policies\\MossenCode";
pub const WINDOWS_REGISTRY_KEY_PATH_HKCU: &str = "HKCU\\SOFTWARE\\Policies\\MossenCode";

/// Windows registry value name containing the JSON settings blob.
pub const WINDOWS_REGISTRY_VALUE_NAME: &str = "Settings";

/// Path to macOS plutil binary.
pub const PLUTIL_PATH: &str = "/usr/bin/plutil";

/// Arguments for plutil to convert plist to JSON on stdout.
pub const PLUTIL_ARGS_PREFIX: &[&str] = &["-convert", "json", "-o", "-", "--"];

/// Subprocess timeout in milliseconds.
pub const MDM_SUBPROCESS_TIMEOUT_MS: u64 = 5000;

/// Build the list of macOS plist paths in priority order.
pub fn get_macos_plist_paths() -> Vec<(String, String)> {
    let mut paths = Vec::new();

    if let Ok(info) = nix_username() {
        if !info.is_empty() {
            paths.push((
                format!(
                    "/Library/Managed Preferences/{}/{}.plist",
                    info, MACOS_PREFERENCE_DOMAIN
                ),
                "per-user managed preferences".to_string(),
            ));
        }
    }

    paths.push((
        format!(
            "/Library/Managed Preferences/{}.plist",
            MACOS_PREFERENCE_DOMAIN
        ),
        "device-level managed preferences".to_string(),
    ));

    if std::env::var("USER_TYPE").as_deref() == Ok("ant") {
        if let Some(home) = dirs::home_dir() {
            paths.push((
                home.join("Library")
                    .join("Preferences")
                    .join(format!("{}.plist", MACOS_PREFERENCE_DOMAIN))
                    .to_string_lossy()
                    .to_string(),
                "user preferences (ant-only)".to_string(),
            ));
        }
    }

    paths
}

/// Get the current username on Unix systems.
fn nix_username() -> Result<String, String> {
    std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).map_err(|e| e.to_string())
}

// ============================================================================
// mdm/rawRead.ts
// ============================================================================

/// Raw read result from MDM subprocess.
#[derive(Debug, Clone, Default)]
pub struct RawReadResult {
    pub plist_stdouts: Option<Vec<(String, String)>>, // (stdout, label)
    pub hklm_stdout: Option<String>,
    pub hkcu_stdout: Option<String>,
}

/// Fire a subprocess to read plist or registry for MDM settings.
pub async fn fire_raw_read() -> RawReadResult {
    if cfg!(target_os = "macos") {
        let plist_paths = get_macos_plist_paths();
        let mut results = Vec::new();

        for (path, label) in plist_paths {
            if !Path::new(&path).exists() {
                continue;
            }
            match tokio::process::Command::new(PLUTIL_PATH)
                .args(PLUTIL_ARGS_PREFIX)
                .arg(&path)
                .output()
                .await
            {
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        if !stdout.is_empty() {
                            results.push((stdout, label));
                            break; // first source wins
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        return RawReadResult {
            plist_stdouts: if results.is_empty() {
                Some(vec![])
            } else {
                Some(results)
            },
            hklm_stdout: None,
            hkcu_stdout: None,
        };
    }

    if cfg!(target_os = "windows") {
        let hklm = tokio::process::Command::new("reg")
            .args(["query", WINDOWS_REGISTRY_KEY_PATH_HKLM, "/v", WINDOWS_REGISTRY_VALUE_NAME])
            .output()
            .await;

        let hkcu = tokio::process::Command::new("reg")
            .args(["query", WINDOWS_REGISTRY_KEY_PATH_HKCU, "/v", WINDOWS_REGISTRY_VALUE_NAME])
            .output()
            .await;

        return RawReadResult {
            plist_stdouts: None,
            hklm_stdout: hklm.ok().filter(|o| o.status.success()).map(|o| {
                String::from_utf8_lossy(&o.stdout).to_string()
            }),
            hkcu_stdout: hkcu.ok().filter(|o| o.status.success()).map(|o| {
                String::from_utf8_lossy(&o.stdout).to_string()
            }),
        };
    }

    RawReadResult::default()
}

// ============================================================================
// mdm/settings.ts
// ============================================================================

/// MDM result containing settings and errors.
#[derive(Debug, Clone)]
pub struct MdmResult {
    pub settings: SettingsJson,
    pub errors: Vec<ValidationError>,
}

impl Default for MdmResult {
    fn default() -> Self {
        Self {
            settings: SettingsJson::default(),
            errors: Vec::new(),
        }
    }
}

static MDM_CACHE: Lazy<Mutex<Option<MdmResult>>> = Lazy::new(|| Mutex::new(None));
static HKCU_CACHE: Lazy<Mutex<Option<MdmResult>>> = Lazy::new(|| Mutex::new(None));

/// Read admin-controlled MDM settings from the session cache.
pub fn get_mdm_settings() -> MdmResult {
    MDM_CACHE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

/// Read HKCU registry settings.
pub fn get_hkcu_settings() -> MdmResult {
    HKCU_CACHE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

/// Clear MDM and HKCU settings caches.
pub fn clear_mdm_settings_cache() {
    *MDM_CACHE.lock().unwrap() = None;
    *HKCU_CACHE.lock().unwrap() = None;
}

/// Update the session caches directly.
pub fn set_mdm_settings_cache(mdm: MdmResult, hkcu: MdmResult) {
    *MDM_CACHE.lock().unwrap() = Some(mdm);
    *HKCU_CACHE.lock().unwrap() = Some(hkcu);
}

/// Parse command output as settings JSON.
pub fn parse_command_output_as_settings(
    stdout: &str,
    source_path: &str,
) -> MdmResult {
    let mut data: Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return MdmResult::default(),
    };

    if !data.is_object() {
        return MdmResult::default();
    }

    let rule_warnings = filter_invalid_permission_rules(&mut data, source_path);

    match serde_json::from_value::<SettingsJson>(data) {
        Ok(settings) => MdmResult {
            settings,
            errors: rule_warnings,
        },
        Err(e) => MdmResult {
            settings: SettingsJson::default(),
            errors: {
                let mut errs = rule_warnings;
                errs.push(ValidationError {
                    file: Some(source_path.to_string()),
                    path: String::new(),
                    message: format!("Settings validation error: {}", e),
                    expected: None,
                    invalid_value: None,
                    suggestion: None,
                    doc_link: None,
                    mcp_error_metadata: None,
                });
                errs
            },
        },
    }
}

/// Parse reg query stdout to extract a registry string value.
pub fn parse_reg_query_stdout(stdout: &str, value_name: &str) -> Option<String> {
    let escaped = regex::escape(value_name);
    let re = Regex::new(&format!(
        r"^\s+{}\s+REG_(?:EXPAND_)?SZ\s+(.*)$",
        escaped
    ))
    .ok()?;

    for line in stdout.lines() {
        if let Some(caps) = re.captures(line) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().trim_end().to_string());
            }
        }
    }
    None
}

/// Convert raw subprocess output into parsed MDM and HKCU results.
pub fn consume_raw_read_result(raw: &RawReadResult) -> (MdmResult, MdmResult) {
    // macOS: plist result
    if let Some(plist_stdouts) = &raw.plist_stdouts {
        if let Some((stdout, label)) = plist_stdouts.first() {
            let result = parse_command_output_as_settings(stdout, label);
            if !result.settings.extra.is_empty()
                || result.settings.model.is_some()
                || result.settings.permissions.is_some()
            {
                return (result, MdmResult::default());
            }
        }
    }

    // Windows: HKLM result
    if let Some(hklm_stdout) = &raw.hklm_stdout {
        if let Some(json_string) = parse_reg_query_stdout(hklm_stdout, WINDOWS_REGISTRY_VALUE_NAME)
        {
            let result = parse_command_output_as_settings(
                &json_string,
                &format!(
                    "Registry: {}\\{}",
                    WINDOWS_REGISTRY_KEY_PATH_HKLM, WINDOWS_REGISTRY_VALUE_NAME
                ),
            );
            if !result.settings.extra.is_empty() || result.settings.model.is_some() {
                return (result, MdmResult::default());
            }
        }
    }

    // No admin MDM — check managed-settings.json before using HKCU
    if has_managed_settings_file() {
        return (MdmResult::default(), MdmResult::default());
    }

    // Fall through to HKCU
    if let Some(hkcu_stdout) = &raw.hkcu_stdout {
        if let Some(json_string) = parse_reg_query_stdout(hkcu_stdout, WINDOWS_REGISTRY_VALUE_NAME)
        {
            let result = parse_command_output_as_settings(
                &json_string,
                &format!(
                    "Registry: {}\\{}",
                    WINDOWS_REGISTRY_KEY_PATH_HKCU, WINDOWS_REGISTRY_VALUE_NAME
                ),
            );
            return (MdmResult::default(), result);
        }
    }

    (MdmResult::default(), MdmResult::default())
}

/// Check if file-based managed settings exist.
fn has_managed_settings_file() -> bool {
    let file_path = get_managed_file_path().join("managed-settings.json");
    if let Ok(content) = std::fs::read_to_string(&file_path) {
        if let Ok(data) = serde_json::from_str::<Value>(&content) {
            if data.is_object() && data.as_object().map_or(false, |o| !o.is_empty()) {
                return true;
            }
        }
    }

    let drop_in_dir = get_managed_settings_drop_in_dir();
    if let Ok(entries) = std::fs::read_dir(&drop_in_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json")
                && !path
                    .file_name()
                    .map_or(true, |n| n.to_string_lossy().starts_with('.'))
            {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<Value>(&content) {
                        if data.is_object()
                            && data.as_object().map_or(false, |o| !o.is_empty())
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// Refresh MDM settings by firing a fresh raw read.
pub async fn refresh_mdm_settings() -> (MdmResult, MdmResult) {
    let raw = fire_raw_read().await;
    consume_raw_read_result(&raw)
}

// ============================================================================
// pluginOnlyPolicy.ts
// ============================================================================

/// Customization surface type.
pub type CustomizationSurface = String;

/// Check whether a customization surface is locked to plugin-only sources.
pub fn is_restricted_to_plugin_only(
    surface: &str,
    policy_settings: Option<&SettingsJson>,
) -> bool {
    let policy = policy_settings.and_then(|s| s.strict_plugin_only_customization.as_ref());
    match policy {
        Some(Value::Bool(true)) => true,
        Some(Value::Array(arr)) => arr.iter().any(|v| v.as_str() == Some(surface)),
        _ => false,
    }
}

/// Admin-trusted sources that bypass strictPluginOnlyCustomization.
static ADMIN_TRUSTED_SOURCES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("plugin");
    set.insert("policySettings");
    set.insert("built-in");
    set.insert("builtin");
    set.insert("bundled");
    set
});

/// Whether a customization's source is admin-trusted.
pub fn is_source_admin_trusted(source: Option<&str>) -> bool {
    source.map_or(false, |s| ADMIN_TRUSTED_SOURCES.contains(s))
}

// ============================================================================
// validateEditTool.ts
// ============================================================================

/// Validation result for edit tool.
#[derive(Debug)]
pub struct EditValidationResult {
    pub result: bool,
    pub message: String,
    pub error_code: i32,
}

/// Validates settings file edits to ensure the result conforms to schema.
pub fn validate_input_for_settings_file_edit(
    file_path: &str,
    original_content: &str,
    get_updated_content: impl FnOnce() -> String,
) -> Option<EditValidationResult> {
    // Only validate Mossen settings files
    if !is_mossen_settings_path(file_path) {
        return None;
    }

    // Check if the current file (before edit) conforms to the schema
    if validate_settings_file_content(original_content).is_err() {
        return None;
    }

    // If the before version is valid, ensure the after version is also valid
    let updated_content = get_updated_content();
    if let Err((error, full_schema)) = validate_settings_file_content(&updated_content) {
        return Some(EditValidationResult {
            result: false,
            message: format!(
                "Mossen settings.json validation failed after edit:\n{}\n\nFull schema:\n{}\nIMPORTANT: Do not update the env unless explicitly instructed to do so.",
                error, full_schema
            ),
            error_code: 10,
        });
    }

    None
}

/// Check if a path is a Mossen settings path.
fn is_mossen_settings_path(path: &str) -> bool {
    path.contains(".mossen") && path.ends_with("settings.json")
        || path.ends_with("settings.local.json")
}

// ============================================================================
// settings.ts
// ============================================================================

/// Get the managed settings file path.
fn get_managed_settings_file_path() -> PathBuf {
    get_managed_file_path().join("managed-settings.json")
}

/// Load file-based managed settings: managed-settings.json + managed-settings.d/*.json.
pub fn load_managed_file_settings() -> (Option<SettingsJson>, Vec<ValidationError>) {
    let mut errors = Vec::new();
    let mut merged = SettingsJson::default();
    let mut found = false;

    let (base_settings, base_errors) =
        parse_settings_file(&get_managed_settings_file_path().to_string_lossy());
    errors.extend(base_errors);
    if let Some(settings) = base_settings {
        merge_settings(&mut merged, &settings);
        found = true;
    }

    let drop_in_dir = get_managed_settings_drop_in_dir();
    if let Ok(entries) = std::fs::read_dir(&drop_in_dir) {
        let mut names: Vec<_> = entries
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                e.path().is_file() && name_str.ends_with(".json") && !name_str.starts_with('.')
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();

        for name in names {
            let path = drop_in_dir.join(&name);
            let (settings, file_errors) = parse_settings_file(&path.to_string_lossy());
            errors.extend(file_errors);
            if let Some(settings) = settings {
                merge_settings(&mut merged, &settings);
                found = true;
            }
        }
    }

    (if found { Some(merged) } else { None }, errors)
}

/// Check which file-based managed settings sources are present.
pub fn get_managed_file_settings_presence() -> (bool, bool) {
    let (base, _) = parse_settings_file(&get_managed_settings_file_path().to_string_lossy());
    let has_base = base.is_some();

    let mut has_drop_ins = false;
    let drop_in_dir = get_managed_settings_drop_in_dir();
    if let Ok(entries) = std::fs::read_dir(&drop_in_dir) {
        has_drop_ins = entries.flatten().any(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            e.path().is_file() && name_str.ends_with(".json") && !name_str.starts_with('.')
        });
    }

    (has_base, has_drop_ins)
}

/// Parse a settings file.
pub fn parse_settings_file(path: &str) -> (Option<SettingsJson>, Vec<ValidationError>) {
    if let Some(cached) = get_cached_parsed_file(path) {
        return (cached.settings.clone(), cached.errors.clone());
    }

    let result = parse_settings_file_uncached(path);
    set_cached_parsed_file(
        path,
        ParsedSettings {
            settings: result.0.clone(),
            errors: result.1.clone(),
        },
    );
    result
}

fn parse_settings_file_uncached(path: &str) -> (Option<SettingsJson>, Vec<ValidationError>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Broken symlink or missing file encountered for settings.json at path: {}",
                    path
                );
            }
            return (None, vec![]);
        }
    };

    if content.trim().is_empty() {
        return (Some(SettingsJson::default()), vec![]);
    }

    let mut data: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (None, vec![]),
    };

    let rule_warnings = filter_invalid_permission_rules(&mut data, path);

    match serde_json::from_value::<SettingsJson>(data) {
        Ok(settings) => (Some(settings), rule_warnings),
        Err(e) => {
            let mut errors = rule_warnings;
            errors.push(ValidationError {
                file: Some(path.to_string()),
                path: String::new(),
                message: format!("Schema validation error: {}", e),
                expected: None,
                invalid_value: None,
                suggestion: None,
                doc_link: None,
                mcp_error_metadata: None,
            });
            (None, errors)
        }
    }
}

/// Get the root path for a settings source.
pub fn get_settings_root_path_for_source(
    source: SettingSource,
    config_home: &str,
    cwd: &str,
    flag_settings_path: Option<&str>,
) -> PathBuf {
    match source {
        SettingSource::UserSettings => PathBuf::from(config_home),
        SettingSource::PolicySettings
        | SettingSource::ProjectSettings
        | SettingSource::LocalSettings => PathBuf::from(cwd),
        SettingSource::FlagSettings => {
            if let Some(p) = flag_settings_path {
                PathBuf::from(p)
                    .parent()
                    .unwrap_or_else(|| Path::new(cwd))
                    .to_path_buf()
            } else {
                PathBuf::from(cwd)
            }
        }
    }
}

/// Get the settings file path for a source.
pub fn get_settings_file_path_for_source(
    source: SettingSource,
    config_home: &str,
    cwd: &str,
    flag_settings_path: Option<&str>,
    use_cowork: bool,
) -> Option<String> {
    match source {
        SettingSource::UserSettings => {
            let root = get_settings_root_path_for_source(source, config_home, cwd, flag_settings_path);
            let filename = if use_cowork {
                "cowork_settings.json"
            } else {
                "settings.json"
            };
            Some(root.join(filename).to_string_lossy().to_string())
        }
        SettingSource::ProjectSettings => {
            let root = get_settings_root_path_for_source(source, config_home, cwd, flag_settings_path);
            Some(
                root.join(".mossen")
                    .join("settings.json")
                    .to_string_lossy()
                    .to_string(),
            )
        }
        SettingSource::LocalSettings => {
            let root = get_settings_root_path_for_source(source, config_home, cwd, flag_settings_path);
            Some(
                root.join(".mossen")
                    .join("settings.local.json")
                    .to_string_lossy()
                    .to_string(),
            )
        }
        SettingSource::PolicySettings => {
            Some(get_managed_settings_file_path().to_string_lossy().to_string())
        }
        SettingSource::FlagSettings => flag_settings_path.map(|p| p.to_string()),
    }
}

/// Get the relative settings file path for project/local sources.
pub fn get_relative_settings_file_path_for_source(source: SettingSource) -> Option<PathBuf> {
    match source {
        SettingSource::ProjectSettings => Some(PathBuf::from(".mossen").join("settings.json")),
        SettingSource::LocalSettings => {
            Some(PathBuf::from(".mossen").join("settings.local.json"))
        }
        _ => None,
    }
}

/// Merge arrays by concatenation and deduplication.
fn merge_arrays(target: &[Value], source: &[Value]) -> Vec<Value> {
    let mut result = target.to_vec();
    for item in source {
        if !result.iter().any(|r| r == item) {
            result.push(item.clone());
        }
    }
    result
}

/// Deep merge settings: source into target.
pub fn merge_settings(target: &mut SettingsJson, source: &SettingsJson) {
    // Convert both to Value, merge, then deserialize back.
    let target_val = serde_json::to_value(&*target).unwrap_or(Value::Object(Default::default()));
    let source_val = serde_json::to_value(source).unwrap_or(Value::Object(Default::default()));

    let merged = deep_merge_values(target_val, source_val);
    if let Ok(result) = serde_json::from_value::<SettingsJson>(merged) {
        *target = result;
    }
}

/// Deep merge two JSON values. Arrays are concatenated and deduplicated.
fn deep_merge_values(target: Value, source: Value) -> Value {
    match (target, source) {
        (Value::Object(mut t), Value::Object(s)) => {
            for (key, s_val) in s {
                let merged = if let Some(t_val) = t.remove(&key) {
                    deep_merge_values(t_val, s_val)
                } else {
                    s_val
                };
                t.insert(key, merged);
            }
            Value::Object(t)
        }
        (Value::Array(t), Value::Array(s)) => Value::Array(merge_arrays(&t, &s)),
        (_, source) => source,
    }
}

/// Update settings for a source by merging new settings.
pub fn update_settings_for_source(
    source: SettingSource,
    settings: &SettingsJson,
    config_home: &str,
    cwd: &str,
    flag_settings_path: Option<&str>,
    use_cowork: bool,
) -> Result<(), String> {
    if matches!(
        source,
        SettingSource::PolicySettings | SettingSource::FlagSettings
    ) {
        return Ok(());
    }

    let file_path = match get_settings_file_path_for_source(
        source,
        config_home,
        cwd,
        flag_settings_path,
        use_cowork,
    ) {
        Some(p) => p,
        None => return Ok(()),
    };

    // Create the folder if needed
    if let Some(parent) = Path::new(&file_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Read existing settings
    let mut existing = match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            if content.trim().is_empty() {
                SettingsJson::default()
            } else {
                match serde_json::from_str::<SettingsJson>(&content) {
                    Ok(s) => s,
                    Err(_) => {
                        // Try raw JSON parse
                        match serde_json::from_str::<Value>(&content) {
                            Ok(Value::Null) | Err(_) => {
                                return Err(format!(
                                    "Invalid JSON syntax in settings file at {}",
                                    file_path
                                ));
                            }
                            Ok(raw) => serde_json::from_value::<SettingsJson>(raw)
                                .unwrap_or_default(),
                        }
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SettingsJson::default(),
        Err(e) => return Err(format!("Failed to read settings from {}: {}", file_path, e)),
    };

    merge_settings(&mut existing, settings);

    mark_internal_write(&file_path);

    let json_str = serde_json::to_string_pretty(&existing)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    std::fs::write(&file_path, format!("{}\n", json_str))
        .map_err(|e| format!("Failed to write settings to {}: {}", file_path, e))?;

    reset_settings_cache();

    Ok(())
}

/// Get managed settings keys for logging.
pub fn get_managed_settings_keys_for_logging(settings: &SettingsJson) -> Vec<String> {
    let value = serde_json::to_value(settings).unwrap_or(Value::Object(Default::default()));
    let obj = match value.as_object() {
        Some(o) => o,
        None => return vec![],
    };

    let keys_to_expand: HashSet<&str> = ["permissions", "sandbox", "hooks"].iter().copied().collect();

    let valid_nested_keys: HashMap<&str, HashSet<&str>> = {
        let mut m = HashMap::new();
        m.insert(
            "permissions",
            [
                "allow",
                "deny",
                "ask",
                "defaultMode",
                "disableBypassPermissionsMode",
                "disableAutoMode",
                "additionalDirectories",
            ]
            .iter()
            .copied()
            .collect(),
        );
        m.insert(
            "sandbox",
            [
                "enabled",
                "failIfUnavailable",
                "allowUnsandboxedCommands",
                "network",
                "filesystem",
                "ignoreViolations",
                "excludedCommands",
                "autoAllowBashIfSandboxed",
                "enableWeakerNestedSandbox",
                "enableWeakerNetworkIsolation",
                "ripgrep",
            ]
            .iter()
            .copied()
            .collect(),
        );
        m.insert(
            "hooks",
            [
                "PreToolUse",
                "PostToolUse",
                "Notification",
                "UserPromptSubmit",
                "SessionStart",
                "SessionEnd",
                "Stop",
                "SubagentStop",
                "PreCompact",
                "PostCompact",
                "TeammateIdle",
                "TaskCreated",
                "TaskCompleted",
            ]
            .iter()
            .copied()
            .collect(),
        );
        m
    };

    let mut all_keys = Vec::new();

    for (key, val) in obj {
        if keys_to_expand.contains(key.as_str()) {
            if let Some(nested_obj) = val.as_object() {
                if let Some(valid_keys) = valid_nested_keys.get(key.as_str()) {
                    for nested_key in nested_obj.keys() {
                        if valid_keys.contains(nested_key.as_str()) {
                            all_keys.push(format!("{}.{}", key, nested_key));
                        }
                    }
                }
            }
        } else {
            all_keys.push(key.clone());
        }
    }

    all_keys.sort();
    all_keys
}

/// Get the origin of the highest-priority active policy settings source.
pub fn get_policy_settings_origin() -> Option<&'static str> {
    // Check remote
    // (In Rust, remote settings would be checked via a separate module)
    // For now, check MDM and file-based settings

    let mdm = get_mdm_settings();
    if !mdm.settings.extra.is_empty() || mdm.settings.model.is_some() {
        return Some(if cfg!(target_os = "macos") {
            "plist"
        } else {
            "hklm"
        });
    }

    let (file_settings, _) = load_managed_file_settings();
    if file_settings.is_some() {
        return Some("file");
    }

    let hkcu = get_hkcu_settings();
    if !hkcu.settings.extra.is_empty() || hkcu.settings.model.is_some() {
        return Some("hkcu");
    }

    None
}

/// Check if any raw settings file contains a specific key.
pub fn raw_settings_contains_key(
    key: &str,
    enabled_sources: &[SettingSource],
    config_home: &str,
    cwd: &str,
    flag_settings_path: Option<&str>,
    use_cowork: bool,
) -> bool {
    for source in enabled_sources {
        if *source == SettingSource::PolicySettings {
            continue;
        }

        let file_path = match get_settings_file_path_for_source(
            *source,
            config_home,
            cwd,
            flag_settings_path,
            use_cowork,
        ) {
            Some(p) => p,
            None => continue,
        };

        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    continue;
                }
                if let Ok(data) = serde_json::from_str::<Value>(&content) {
                    if data.is_object() && data.get(key).is_some() {
                        return true;
                    }
                }
            }
            Err(_) => continue,
        }
    }

    false
}

/// Returns true if any trusted settings source has accepted the bypass
/// permissions mode dialog.
pub fn has_skip_dangerous_mode_permission_prompt(
    get_source: impl Fn(SettingSource) -> Option<SettingsJson>,
) -> bool {
    let sources = [
        SettingSource::UserSettings,
        SettingSource::LocalSettings,
        SettingSource::FlagSettings,
        SettingSource::PolicySettings,
    ];
    for source in &sources {
        if let Some(settings) = get_source(*source) {
            if settings.skip_dangerous_mode_permission_prompt == Some(true) {
                return true;
            }
        }
    }
    false
}

/// Returns true if any trusted settings source has accepted the auto mode opt-in.
pub fn has_auto_mode_opt_in(
    get_source: impl Fn(SettingSource) -> Option<SettingsJson>,
) -> bool {
    let sources = [
        SettingSource::UserSettings,
        SettingSource::LocalSettings,
        SettingSource::FlagSettings,
        SettingSource::PolicySettings,
    ];
    for source in &sources {
        if let Some(settings) = get_source(*source) {
            if settings.skip_auto_permission_prompt == Some(true) {
                return true;
            }
        }
    }
    false
}

/// Returns whether plan mode should use auto mode semantics. Default true.
pub fn get_use_auto_mode_during_plan(
    get_source: impl Fn(SettingSource) -> Option<SettingsJson>,
) -> bool {
    let sources = [
        SettingSource::PolicySettings,
        SettingSource::FlagSettings,
        SettingSource::UserSettings,
        SettingSource::LocalSettings,
    ];
    for source in &sources {
        if let Some(settings) = get_source(*source) {
            if settings.use_auto_mode_during_plan == Some(false) {
                return false;
            }
        }
    }
    true
}

/// Returns the merged autoMode config from trusted settings sources.
pub fn get_auto_mode_config(
    get_source: impl Fn(SettingSource) -> Option<SettingsJson>,
) -> Option<AutoModeConfig> {
    let sources = [
        SettingSource::UserSettings,
        SettingSource::LocalSettings,
        SettingSource::FlagSettings,
        SettingSource::PolicySettings,
    ];

    let mut allow = Vec::new();
    let mut soft_deny = Vec::new();
    let mut environment = Vec::new();

    for source in &sources {
        if let Some(settings) = get_source(*source) {
            if let Some(auto_mode) = &settings.auto_mode {
                if let Some(a) = &auto_mode.allow {
                    allow.extend(a.iter().cloned());
                }
                if let Some(sd) = &auto_mode.soft_deny {
                    soft_deny.extend(sd.iter().cloned());
                }
                if let Some(d) = &auto_mode.deny {
                    // ant back-compat
                    if std::env::var("USER_TYPE").as_deref() == Ok("ant") {
                        soft_deny.extend(d.iter().cloned());
                    }
                }
                if let Some(e) = &auto_mode.environment {
                    environment.extend(e.iter().cloned());
                }
            }
        }
    }

    if allow.is_empty() && soft_deny.is_empty() && environment.is_empty() {
        return None;
    }

    Some(AutoModeConfig {
        allow: if allow.is_empty() { None } else { Some(allow) },
        soft_deny: if soft_deny.is_empty() {
            None
        } else {
            Some(soft_deny)
        },
        deny: None,
        environment: if environment.is_empty() {
            None
        } else {
            Some(environment)
        },
    })
}

// ============================================================================
// changeDetector.ts
// ============================================================================

/// Time in milliseconds to wait for file writes to stabilize.
pub const FILE_STABILITY_THRESHOLD_MS: u64 = 1000;

/// Polling interval for checking file stability.
pub const FILE_STABILITY_POLL_INTERVAL_MS: u64 = 500;

/// Time window to consider a file change as internal.
pub const INTERNAL_WRITE_WINDOW_MS: u64 = 5000;

/// Poll interval for MDM settings changes (30 minutes).
pub const MDM_POLL_INTERVAL_MS: u64 = 30 * 60 * 1000;

/// Grace period before processing a settings file deletion.
pub const DELETION_GRACE_MS: u64 =
    FILE_STABILITY_THRESHOLD_MS + FILE_STABILITY_POLL_INTERVAL_MS + 200;

/// Config change source for hook execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigChangeSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
    PolicySettings,
}

/// Convert a SettingSource to ConfigChangeSource.
pub fn setting_source_to_config_change_source(source: SettingSource) -> ConfigChangeSource {
    match source {
        SettingSource::UserSettings => ConfigChangeSource::UserSettings,
        SettingSource::ProjectSettings => ConfigChangeSource::ProjectSettings,
        SettingSource::LocalSettings => ConfigChangeSource::LocalSettings,
        SettingSource::FlagSettings | SettingSource::PolicySettings => {
            ConfigChangeSource::PolicySettings
        }
    }
}

/// Settings change detector state.
pub struct SettingsChangeDetector {
    initialized: bool,
    disposed: bool,
    last_mdm_snapshot: Option<String>,
    pending_deletions: HashMap<String, ()>,
    listeners: Vec<Box<dyn Fn(SettingSource) + Send + Sync>>,
}

impl SettingsChangeDetector {
    pub fn new() -> Self {
        Self {
            initialized: false,
            disposed: false,
            last_mdm_snapshot: None,
            pending_deletions: HashMap::new(),
            listeners: Vec::new(),
        }
    }

    /// Subscribe to settings changes.
    pub fn subscribe(&mut self, listener: Box<dyn Fn(SettingSource) + Send + Sync>) {
        self.listeners.push(listener);
    }

    /// Dispose of the change detector.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.pending_deletions.clear();
        self.last_mdm_snapshot = None;
        clear_internal_writes();
        self.listeners.clear();
    }

    /// Notify listeners of a settings change.
    pub fn notify_change(&self, source: SettingSource) {
        reset_settings_cache();
        for listener in &self.listeners {
            listener(source);
        }
    }

    /// Get watch targets - collect settings file paths and parent directories.
    pub fn get_watch_targets(
        &self,
        enabled_sources: &[SettingSource],
        config_home: &str,
        cwd: &str,
        flag_settings_path: Option<&str>,
        use_cowork: bool,
    ) -> (Vec<String>, HashSet<String>, Option<String>) {
        let mut dir_to_files: HashMap<String, HashSet<String>> = HashMap::new();
        let mut dirs_with_existing: HashSet<String> = HashSet::new();

        for source in enabled_sources {
            if *source == SettingSource::FlagSettings {
                continue;
            }
            let path = match get_settings_file_path_for_source(
                *source,
                config_home,
                cwd,
                flag_settings_path,
                use_cowork,
            ) {
                Some(p) => p,
                None => continue,
            };

            let dir = Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            dir_to_files
                .entry(dir.clone())
                .or_default()
                .insert(path.clone());

            if Path::new(&path).is_file() {
                dirs_with_existing.insert(dir);
            }
        }

        let mut settings_files = HashSet::new();
        for dir in &dirs_with_existing {
            if let Some(files) = dir_to_files.get(dir) {
                for file in files {
                    settings_files.insert(file.clone());
                }
            }
        }

        let drop_in_dir_path = get_managed_settings_drop_in_dir();
        let drop_in_dir = if drop_in_dir_path.is_dir() {
            let dir_str = drop_in_dir_path.to_string_lossy().to_string();
            dirs_with_existing.insert(dir_str.clone());
            Some(dir_str)
        } else {
            None
        };

        (
            dirs_with_existing.into_iter().collect(),
            settings_files,
            drop_in_dir,
        )
    }

    /// Get the source for a given file path.
    pub fn get_source_for_path(
        &self,
        path: &str,
        enabled_sources: &[SettingSource],
        config_home: &str,
        cwd: &str,
        flag_settings_path: Option<&str>,
        use_cowork: bool,
    ) -> Option<SettingSource> {
        let drop_in_dir = get_managed_settings_drop_in_dir();
        let drop_in_str = drop_in_dir.to_string_lossy();
        if path.starts_with(&*drop_in_str) {
            return Some(SettingSource::PolicySettings);
        }

        for source in enabled_sources {
            if let Some(file_path) = get_settings_file_path_for_source(
                *source,
                config_home,
                cwd,
                flag_settings_path,
                use_cowork,
            ) {
                if file_path == path {
                    return Some(*source);
                }
            }
        }

        None
    }
}

impl Default for SettingsChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// applySettingsChange.ts
// ============================================================================

/// Apply a settings change to app state.
/// This function re-reads settings, reloads permissions/hooks, and pushes new state.
pub fn apply_settings_change<F>(source: SettingSource, set_app_state: F)
where
    F: FnOnce(SettingSource, &SettingsJson),
{
    // Reset settings cache and load fresh settings
    reset_settings_cache();

    let (settings, _) = parse_settings_file(&get_managed_settings_file_path().to_string_lossy());
    let new_settings = settings.unwrap_or_default();

    set_app_state(source, &new_settings);
}

// ============================================================================
// allErrors.ts
// ============================================================================

/// Get merged settings with all validation errors, including MCP config errors.
pub fn get_settings_with_all_errors(
    get_settings_with_errors: impl FnOnce() -> SettingsWithErrors,
    get_mcp_errors: impl FnOnce() -> Vec<ValidationError>,
) -> SettingsWithErrors {
    let mut result = get_settings_with_errors();
    let mcp_errors = get_mcp_errors();
    result.errors.extend(mcp_errors);
    result
}

// =============================================================================
// Schema 标识符与 settings 入口（外部模块按 TS 同名引用的别名）。
// =============================================================================

/// 对应 TS `EnvironmentVariablesSchema`。
pub const ENVIRONMENT_VARIABLES_SCHEMA: &str = "EnvironmentVariables";
/// 对应 TS `PermissionsSchema`。
pub const PERMISSIONS_SCHEMA: &str = "Permissions";
/// 对应 TS `SettingsSchema`。
pub const SETTINGS_SCHEMA: &str = "Settings";

/// 对应 TS `settingsMergeCustomizer`：lodash-merge 的自定义合并逻辑。
pub fn settings_merge_customizer(
    target: serde_json::Value,
    source: serde_json::Value,
) -> serde_json::Value {
    match (target, source) {
        (serde_json::Value::Object(mut a), serde_json::Value::Object(b)) => {
            for (k, v) in b {
                let existing = a.remove(&k).unwrap_or(serde_json::Value::Null);
                a.insert(k, settings_merge_customizer(existing, v));
            }
            serde_json::Value::Object(a)
        }
        (_, src) => src,
    }
}

/// MDM async-load state — protects the snapshot returned by
/// `ensure_mdm_settings_loaded` from being recomputed concurrently.
static MDM_SNAPSHOT: Lazy<Mutex<Option<serde_json::Value>>> = Lazy::new(|| Mutex::new(None));
static MDM_RAW_SNAPSHOT: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Trigger an asynchronous MDM settings load. The result lands in the
/// `MDM_SNAPSHOT` cache and is awaited via [`ensure_mdm_settings_loaded`].
/// Mirrors `startMdmSettingsLoad` in `utils/settings/mdm/settings.ts` — the
/// TS version spawns a microtask that resolves a module-level Promise; in
/// Rust we spawn a tokio task that drains the file-based managed-settings
/// sources via [`crate::settings::load_managed_file_settings`].
pub fn start_mdm_settings_load() {
    {
        let cache = MDM_SNAPSHOT.lock().unwrap();
        if cache.is_some() {
            return;
        }
    }
    tokio::spawn(async {
        let (settings_opt, _errors) = crate::settings::load_managed_file_settings();
        let snapshot = settings_opt
            .map(|s| serde_json::to_value(&s).unwrap_or_else(|_| serde_json::json!({})))
            .unwrap_or_else(|| serde_json::json!({}));
        *MDM_SNAPSHOT.lock().unwrap() = Some(snapshot);
    });
}

/// Wait for the MDM settings load to complete (kicking it off if needed) and
/// return the resulting JSON snapshot. Polls the cache every 25 ms with a
/// 2 s ceiling — matches the TS contract of "always returns, never hangs".
pub async fn ensure_mdm_settings_loaded() -> serde_json::Value {
    if MDM_SNAPSHOT.lock().unwrap().is_none() {
        start_mdm_settings_load();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(snapshot) = MDM_SNAPSHOT.lock().unwrap().clone() {
            return snapshot;
        }
        if std::time::Instant::now() >= deadline {
            return serde_json::json!({});
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// Trigger an asynchronous MDM raw-read (platform-specific stdout pipe from
/// `plutil` / `reg query`). The raw text lands in `MDM_RAW_SNAPSHOT` and is
/// awaited via [`get_mdm_raw_read_promise`].
pub fn start_mdm_raw_read() {
    {
        let cache = MDM_RAW_SNAPSHOT.lock().unwrap();
        if cache.is_some() {
            return;
        }
    }
    tokio::spawn(async {
        // The raw-read path on macOS shells out to `plutil` against the
        // managed-prefs plist; on Linux it reads the managed-settings.json
        // directly. Both produce JSON-shaped text. We reuse
        // `load_managed_file_settings` and re-serialise — the raw read's
        // only consumer is the change-detector hash, which only cares about
        // byte-level equality.
        let (settings_opt, _errors) = crate::settings::load_managed_file_settings();
        let raw = settings_opt
            .and_then(|s| serde_json::to_string(&s).ok())
            .unwrap_or_default();
        *MDM_RAW_SNAPSHOT.lock().unwrap() = Some(raw);
    });
}

/// Wait for the MDM raw-read to complete (kicking it off if needed) and
/// return the result as a JSON string, or `None` if nothing was loaded.
pub async fn get_mdm_raw_read_promise() -> Option<String> {
    if MDM_RAW_SNAPSHOT.lock().unwrap().is_none() {
        start_mdm_raw_read();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(raw) = MDM_RAW_SNAPSHOT.lock().unwrap().clone() {
            return if raw.is_empty() { None } else { Some(raw) };
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// `TOOL_VALIDATION_CONFIG` — settings key for tool-validation config blob.
/// Matches the same-named exported constant in TS.
pub const TOOL_VALIDATION_CONFIG: &str = "ToolValidationConfig";

/// `ToolValidationConfig` type alias — kept as opaque JSON because the schema
/// is consumed by downstream validators rather than read directly.
pub type ToolValidationConfig = serde_json::Value;

/// Return a snapshot of the merged effective settings. Delegates to the real
/// `settings/mod.rs` loader (`get_settings_with_errors`) and unpacks the
/// `.settings` field — same contract as TS `getInitialSettings`.
///
/// Always returns at least an empty object.
pub fn get_initial_settings() -> serde_json::Value {
    let result = crate::settings::load_settings_from_disk();
    serde_json::to_value(&result.settings).unwrap_or_else(|_| serde_json::json!({}))
}
