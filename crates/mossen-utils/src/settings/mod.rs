//! Settings management — translated from utils/settings/
//!
//! Covers: types.ts, constants.ts, settings.ts, settingsCache.ts, validation.ts,
//! validationTips.ts, permissionValidation.ts, toolValidationConfig.ts,
//! managedPath.ts, internalWrites.ts, changeDetector.ts, allErrors.ts,
//! applySettingsChange.ts, pluginOnlyPolicy.ts, schemaOutput.ts, validateEditTool.ts,
//! mdm/constants.ts, mdm/rawRead.ts, mdm/settings.ts

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── constants.ts ───────────────────────────────────────────────────────────

/// All possible sources where settings can come from.
/// Order matters - later sources override earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Editable setting sources (excludes PolicySettings and FlagSettings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditableSettingSource {
    UserSettings,
    ProjectSettings,
    LocalSettings,
}

/// Sources where permission rules can be saved.
pub const EDITABLE_SOURCES: &[EditableSettingSource] = &[
    EditableSettingSource::LocalSettings,
    EditableSettingSource::ProjectSettings,
    EditableSettingSource::UserSettings,
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
pub fn parse_setting_sources_flag(flag: &str) -> Result<Vec<SettingSource>> {
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
                return Err(anyhow!(
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

/// The JSON Schema URL for Mossen settings.
pub const MOSSEN_CODE_SETTINGS_SCHEMA_URL: &str =
    "https://schemas.mossen.invalid/cli-settings.json";

// ─── types.ts ───────────────────────────────────────────────────────────────

/// Surfaces lockable by `strictPluginOnlyCustomization`.
pub const CUSTOMIZATION_SURFACES: &[&str] = &["skills", "agents", "hooks", "mcp"];

/// Unified settings JSON structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub file_suggestion: Option<FileSuggestionConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respect_gitignore: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_period_days: Option<u32>,
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
    pub sandbox: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_survey_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_tips_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_verbs: Option<SpinnerVerbsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner_tips_override: Option<Value>,
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
    pub ssh_configs: Option<Vec<SshConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossen_md_excludes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_trust_message: Option<String>,
    /// Catch-all for unknown/extra fields.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSuggestionConfig {
    #[serde(rename = "type")]
    pub config_type: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_directories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparse_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusLineConfig {
    #[serde(rename = "type")]
    pub config_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpinnerVerbsConfig {
    pub mode: String,
    pub verbs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_environment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPluginEntry {
    pub marketplace: String,
    pub plugin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub id: String,
    pub name: String,
    pub ssh_host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_identity_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedMcpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeniedMcpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraKnownMarketplace {
    pub source: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, HashMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, Value>>,
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

/// Plugin hook matcher (internal type, not user-facing).
#[derive(Debug, Clone)]
pub struct PluginHookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    pub plugin_root: String,
    pub plugin_name: String,
    pub plugin_id: String,
}

/// Skill hook matcher (internal type, not user-facing).
#[derive(Debug, Clone)]
pub struct SkillHookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<Value>,
    pub skill_root: String,
    pub skill_name: String,
}

// ─── managedPath.ts ─────────────────────────────────────────────────────────

/// Get the path to the managed settings directory based on the current platform.
pub fn get_managed_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("MOSSEN_CODE_MANAGED_SETTINGS_PATH") {
        if std::env::var("USER_TYPE").as_deref() == Ok("internal") {
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

// ─── internalWrites.ts ──────────────────────────────────────────────────────

static INTERNAL_WRITE_TIMESTAMPS: Lazy<Mutex<HashMap<PathBuf, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Mark a path as internally written.
pub fn mark_internal_write(path: &Path) {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    map.insert(path.to_path_buf(), Instant::now());
}

/// True if path was marked within window_ms. Consumes the mark.
pub fn consume_internal_write(path: &Path, window: Duration) -> bool {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    if let Some(ts) = map.get(path) {
        if ts.elapsed() < window {
            map.remove(path);
            return true;
        }
    }
    false
}

/// Clear all internal write records.
pub fn clear_internal_writes() {
    let mut map = INTERNAL_WRITE_TIMESTAMPS.lock().unwrap();
    map.clear();
}

// ─── settingsCache.ts ───────────────────────────────────────────────────────

/// Validation error from settings parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpErrorMetadata {
    pub scope: String,
    pub server_name: Option<String>,
    pub severity: Option<String>,
}

/// Settings with associated validation errors.
#[derive(Debug, Clone)]
pub struct SettingsWithErrors {
    pub settings: SettingsJson,
    pub errors: Vec<ValidationError>,
}

struct SettingsCache {
    session_cache: Option<SettingsWithErrors>,
    per_source_cache: HashMap<SettingSource, Option<SettingsJson>>,
    parse_file_cache: HashMap<PathBuf, ParsedSettings>,
    plugin_settings_base: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone)]
struct ParsedSettings {
    settings: Option<SettingsJson>,
    errors: Vec<ValidationError>,
}

static SETTINGS_CACHE: Lazy<RwLock<SettingsCache>> = Lazy::new(|| {
    RwLock::new(SettingsCache {
        session_cache: None,
        per_source_cache: HashMap::new(),
        parse_file_cache: HashMap::new(),
        plugin_settings_base: None,
    })
});

pub fn get_session_settings_cache() -> Option<SettingsWithErrors> {
    let cache = SETTINGS_CACHE.read().unwrap();
    cache.session_cache.clone()
}

pub fn set_session_settings_cache(value: SettingsWithErrors) {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.session_cache = Some(value);
}

pub fn get_cached_settings_for_source(source: SettingSource) -> Option<Option<SettingsJson>> {
    let cache = SETTINGS_CACHE.read().unwrap();
    cache.per_source_cache.get(&source).cloned()
}

pub fn set_cached_settings_for_source(source: SettingSource, value: Option<SettingsJson>) {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.per_source_cache.insert(source, value);
}

pub fn get_cached_parsed_file(path: &Path) -> Option<ParsedSettings> {
    let cache = SETTINGS_CACHE.read().unwrap();
    cache.parse_file_cache.get(path).cloned()
}

pub fn set_cached_parsed_file(path: &Path, value: ParsedSettings) {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.parse_file_cache.insert(path.to_path_buf(), value);
}

pub fn reset_settings_cache() {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.session_cache = None;
    cache.per_source_cache.clear();
    cache.parse_file_cache.clear();
}

pub fn get_plugin_settings_base() -> Option<HashMap<String, Value>> {
    let cache = SETTINGS_CACHE.read().unwrap();
    cache.plugin_settings_base.clone()
}

pub fn set_plugin_settings_base(settings: Option<HashMap<String, Value>>) {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.plugin_settings_base = settings;
}

pub fn clear_plugin_settings_base() {
    let mut cache = SETTINGS_CACHE.write().unwrap();
    cache.plugin_settings_base = None;
}

// ─── validationTips.ts ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ValidationTip {
    pub suggestion: Option<String>,
    pub doc_link: Option<String>,
}

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

pub fn get_validation_tip(context: &TipContext) -> Option<ValidationTip> {
    // Permission defaultMode
    if context.path == "permissions.defaultMode" && context.code == "invalid_value" {
        return Some(ValidationTip {
            suggestion: Some(
                "Valid modes: \"acceptEdits\" (ask before file changes), \"plan\" (analysis only), \"bypassPermissions\" (auto-accept all), or \"default\" (standard behavior)".to_string()
            ),
            doc_link: Some(format!("{}/iam#permission-modes", DOCUMENTATION_BASE)),
        });
    }
    // apiKeyHelper
    if context.path == "apiKeyHelper" && context.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Provide a shell command that outputs your API key to stdout. The script should output only the API key. Example: \"/bin/generate_temp_api_key.sh\"".to_string()
            ),
            doc_link: None,
        });
    }
    // cleanupPeriodDays
    if context.path == "cleanupPeriodDays"
        && context.code == "too_small"
        && context.expected.as_deref() == Some("0")
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Must be 0 or greater. Set a positive number for days to retain transcripts (default is 30). Setting 0 disables session persistence entirely.".to_string()
            ),
            doc_link: None,
        });
    }
    // Environment variables
    if context.path.starts_with("env.") && context.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Environment variables must be strings. Wrap numbers and booleans in quotes. Example: \"DEBUG\": \"true\", \"PORT\": \"3000\"".to_string()
            ),
            doc_link: Some(format!("{}/settings#environment-variables", DOCUMENTATION_BASE)),
        });
    }
    // Permission arrays
    if (context.path == "permissions.allow" || context.path == "permissions.deny")
        && context.code == "invalid_type"
        && context.expected.as_deref() == Some("array")
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Permission rules must be in an array. Format: [\"Tool(specifier)\"]. Examples: [\"Bash(npm run build)\", \"Edit(docs/**)\", \"Read(~/.zshrc)\"].".to_string()
            ),
            doc_link: None,
        });
    }
    // Hooks
    if context.path.contains("hooks") && context.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Hooks use a matcher + hooks array. The matcher is a string: a tool name (\"Bash\"), pipe-separated list (\"Edit|Write\"), or empty to match all.".to_string()
            ),
            doc_link: None,
        });
    }
    // Boolean
    if context.code == "invalid_type" && context.expected.as_deref() == Some("boolean") {
        return Some(ValidationTip {
            suggestion: Some(
                "Use true or false without quotes. Example: \"includeCoAuthoredBy\": true"
                    .to_string(),
            ),
            doc_link: None,
        });
    }
    // Unrecognized keys
    if context.code == "unrecognized_keys" {
        return Some(ValidationTip {
            suggestion: Some(
                "Check for typos or refer to the documentation for valid fields".to_string(),
            ),
            doc_link: Some(format!("{}/settings", DOCUMENTATION_BASE)),
        });
    }
    // Enum values
    if context.code == "invalid_value" && context.enum_values.is_some() {
        let values = context.enum_values.as_ref().unwrap();
        return Some(ValidationTip {
            suggestion: Some(format!(
                "Valid values: {}",
                values
                    .iter()
                    .map(|v| format!("\"{}\"", v))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            doc_link: None,
        });
    }
    // JSON syntax
    if context.code == "invalid_type"
        && context.expected.as_deref() == Some("object")
        && context.received.as_deref() == Some("null")
        && context.path.is_empty()
    {
        return Some(ValidationTip {
            suggestion: Some(
                "Check for missing commas, unmatched brackets, or trailing commas. Use a JSON validator to identify the exact syntax error.".to_string()
            ),
            doc_link: None,
        });
    }
    // additionalDirectories
    if context.path == "permissions.additionalDirectories" && context.code == "invalid_type" {
        return Some(ValidationTip {
            suggestion: Some(
                "Must be an array of directory paths. Example: [\"~/projects\", \"/tmp/workspace\"].".to_string()
            ),
            doc_link: Some(format!("{}/iam#working-directories", DOCUMENTATION_BASE)),
        });
    }
    None
}

// ─── toolValidationConfig.ts ────────────────────────────────────────────────

/// File pattern tools (accept *.ts, src/**, etc.)
pub const FILE_PATTERN_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "NotebookRead",
    "NotebookEdit",
];

/// Bash wildcard tools (accept * anywhere, and legacy command:* syntax)
pub const BASH_PREFIX_TOOLS: &[&str] = &["Bash"];

pub fn is_file_pattern_tool(tool_name: &str) -> bool {
    FILE_PATTERN_TOOLS.contains(&tool_name)
}

pub fn is_bash_prefix_tool(tool_name: &str) -> bool {
    BASH_PREFIX_TOOLS.contains(&tool_name)
}

/// Custom validation for specific tools.
pub fn get_custom_validation(tool_name: &str, content: &str) -> Option<PermissionValidationResult> {
    match tool_name {
        "WebSearch" => {
            if content.contains('*') || content.contains('?') {
                Some(PermissionValidationResult {
                    valid: false,
                    error: Some("WebSearch does not support wildcards".to_string()),
                    suggestion: Some("Use exact search terms without * or ?".to_string()),
                    examples: Some(vec![
                        "WebSearch(mossen ai)".to_string(),
                        "WebSearch(typescript tutorial)".to_string(),
                    ]),
                })
            } else {
                Some(PermissionValidationResult {
                    valid: true,
                    error: None,
                    suggestion: None,
                    examples: None,
                })
            }
        }
        "WebFetch" => {
            if content.contains("://") || content.starts_with("http") {
                Some(PermissionValidationResult {
                    valid: false,
                    error: Some("WebFetch permissions use domain format, not URLs".to_string()),
                    suggestion: Some("Use \"domain:hostname\" format".to_string()),
                    examples: Some(vec![
                        "WebFetch(domain:example.com)".to_string(),
                        "WebFetch(domain:github.com)".to_string(),
                    ]),
                })
            } else if !content.starts_with("domain:") {
                Some(PermissionValidationResult {
                    valid: false,
                    error: Some("WebFetch permissions must use \"domain:\" prefix".to_string()),
                    suggestion: Some("Use \"domain:hostname\" format".to_string()),
                    examples: Some(vec![
                        "WebFetch(domain:example.com)".to_string(),
                        "WebFetch(domain:*.google.com)".to_string(),
                    ]),
                })
            } else {
                Some(PermissionValidationResult {
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

// ─── permissionValidation.ts ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PermissionValidationResult {
    pub valid: bool,
    pub error: Option<String>,
    pub suggestion: Option<String>,
    pub examples: Option<Vec<String>>,
}

/// Checks if a character at a given index is escaped (preceded by odd number of backslashes).
fn is_escaped(s: &str, index: usize) -> bool {
    let bytes = s.as_bytes();
    let mut backslash_count = 0;
    let mut j = index as isize - 1;
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
        if bytes[i] == b'(' && bytes[i + 1] == b')' {
            if !is_escaped(s, i) {
                return true;
            }
        }
    }
    false
}

/// Parse rule into tool_name and rule_content.
fn parse_permission_rule_value(rule: &str) -> (String, Option<String>) {
    if let Some(paren_idx) = rule.find('(') {
        if !is_escaped(rule, paren_idx) && rule.ends_with(')') {
            let tool_name = rule[..paren_idx].to_string();
            let content = rule[paren_idx + 1..rule.len() - 1].to_string();
            let content = if content == "*" { None } else { Some(content) };
            return (tool_name, content);
        }
    }
    (rule.to_string(), None)
}

/// Validates permission rule format and content.
pub fn validate_permission_rule(rule: &str) -> PermissionValidationResult {
    if rule.is_empty() || rule.trim().is_empty() {
        return PermissionValidationResult {
            valid: false,
            error: Some("Permission rule cannot be empty".to_string()),
            suggestion: None,
            examples: None,
        };
    }

    let open_count = count_unescaped_char(rule, '(');
    let close_count = count_unescaped_char(rule, ')');
    if open_count != close_count {
        return PermissionValidationResult {
            valid: false,
            error: Some("Mismatched parentheses".to_string()),
            suggestion: Some(
                "Ensure all opening parentheses have matching closing parentheses".to_string(),
            ),
            examples: None,
        };
    }

    if has_unescaped_empty_parens(rule) {
        let tool_name = rule.split('(').next().unwrap_or("");
        if tool_name.is_empty() {
            return PermissionValidationResult {
                valid: false,
                error: Some("Empty parentheses with no tool name".to_string()),
                suggestion: Some("Specify a tool name before the parentheses".to_string()),
                examples: None,
            };
        }
        return PermissionValidationResult {
            valid: false,
            error: Some("Empty parentheses".to_string()),
            suggestion: Some(format!(
                "Either specify a pattern or use just \"{}\" without parentheses",
                tool_name
            )),
            examples: Some(vec![
                tool_name.to_string(),
                format!("{}(some-pattern)", tool_name),
            ]),
        };
    }

    let (tool_name, rule_content) = parse_permission_rule_value(rule);

    // MCP validation
    if tool_name.starts_with("mcp__") {
        if rule_content.is_some() || count_unescaped_char(rule, '(') > 0 {
            let parts: Vec<&str> = tool_name.splitn(3, "__").collect();
            let server_name = parts.get(1).unwrap_or(&"");
            return PermissionValidationResult {
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
        return PermissionValidationResult {
            valid: true,
            error: None,
            suggestion: None,
            examples: None,
        };
    }

    // Tool name validation
    if tool_name.is_empty() {
        return PermissionValidationResult {
            valid: false,
            error: Some("Tool name cannot be empty".to_string()),
            suggestion: None,
            examples: None,
        };
    }

    if let Some(first_char) = tool_name.chars().next() {
        if !first_char.is_uppercase() {
            let capitalized = format!(
                "{}{}",
                first_char.to_uppercase(),
                &tool_name[first_char.len_utf8()..]
            );
            return PermissionValidationResult {
                valid: false,
                error: Some("Tool names must start with uppercase".to_string()),
                suggestion: Some(format!("Use \"{}\"", capitalized)),
                examples: None,
            };
        }
    }

    // Custom validation
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
                return PermissionValidationResult {
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
                return PermissionValidationResult {
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
                return PermissionValidationResult {
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
            // Loose wildcard check
            if content.contains('*') {
                let re = Regex::new(r"^\*|\*$|\*\*|/\*|\*\.|\*\)").unwrap();
                if !re.is_match(content) && !content.contains("**") {
                    return PermissionValidationResult {
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

    PermissionValidationResult {
        valid: true,
        error: None,
        suggestion: None,
        examples: None,
    }
}

// ─── validation.ts ──────────────────────────────────────────────────────────

/// Format validation errors from serde parse failures.
pub fn format_validation_errors(errors: &[String], file_path: &str) -> Vec<ValidationError> {
    errors
        .iter()
        .map(|msg| ValidationError {
            file: Some(file_path.to_string()),
            path: String::new(),
            message: msg.clone(),
            expected: None,
            invalid_value: None,
            suggestion: None,
            doc_link: None,
            mcp_error_metadata: None,
        })
        .collect()
}

/// Validates settings file content conforms to SettingsJson.
pub fn validate_settings_file_content(content: &str) -> Result<(), String> {
    let json_data: Value =
        serde_json::from_str(content).map_err(|e| format!("Invalid JSON: {}", e))?;

    let result: Result<SettingsJson, _> = serde_json::from_value(json_data);
    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Settings validation failed: {}", e)),
    }
}

/// Filters invalid permission rules from raw parsed JSON data before schema validation.
pub fn filter_invalid_permission_rules(data: &mut Value, file_path: &str) -> Vec<ValidationError> {
    let mut warnings = Vec::new();

    let obj = match data.as_object_mut() {
        Some(o) => o,
        None => return warnings,
    };

    let perms = match obj.get_mut("permissions") {
        Some(Value::Object(p)) => p,
        _ => return warnings,
    };

    for key in &["allow", "deny", "ask"] {
        if let Some(Value::Array(rules)) = perms.get_mut(*key) {
            let mut valid_rules = Vec::new();
            for rule in rules.iter() {
                match rule.as_str() {
                    Some(s) => {
                        let result = validate_permission_rule(s);
                        if result.valid {
                            valid_rules.push(rule.clone());
                        } else {
                            let mut message =
                                format!("Invalid permission rule \"{}\" was skipped", s);
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
                                invalid_value: Some(rule.clone()),
                                suggestion: None,
                                doc_link: None,
                                mcp_error_metadata: None,
                            });
                        }
                    }
                    None => {
                        warnings.push(ValidationError {
                            file: Some(file_path.to_string()),
                            path: format!("permissions.{}", key),
                            message: format!("Non-string value in {} array was removed", key),
                            expected: None,
                            invalid_value: Some(rule.clone()),
                            suggestion: None,
                            doc_link: None,
                            mcp_error_metadata: None,
                        });
                    }
                }
            }
            *rules = valid_rules;
        }
    }

    warnings
}

// ─── settings.ts ────────────────────────────────────────────────────────────

/// Parse a settings file into structured format.
pub fn parse_settings_file(path: &Path) -> (Option<SettingsJson>, Vec<ValidationError>) {
    if let Some(cached) = get_cached_parsed_file(path) {
        return (cached.settings, cached.errors);
    }
    let result = parse_settings_file_uncached(path);
    set_cached_parsed_file(path, result.clone());
    (result.settings, result.errors)
}

fn parse_settings_file_uncached(path: &Path) -> ParsedSettings {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Error reading settings file {:?}: {}", path, e);
            }
            return ParsedSettings {
                settings: None,
                errors: vec![],
            };
        }
    };

    if content.trim().is_empty() {
        return ParsedSettings {
            settings: Some(SettingsJson::default()),
            errors: vec![],
        };
    }

    let mut data: Value = match serde_json::from_str(&content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("JSON parse error in {:?}: {}", path, e);
            return ParsedSettings {
                settings: None,
                errors: vec![],
            };
        }
    };

    let rule_warnings = filter_invalid_permission_rules(&mut data, &path.to_string_lossy());

    match serde_json::from_value::<SettingsJson>(data) {
        Ok(settings) => ParsedSettings {
            settings: Some(settings),
            errors: rule_warnings,
        },
        Err(e) => {
            let mut errors = rule_warnings;
            errors.push(ValidationError {
                file: Some(path.to_string_lossy().to_string()),
                path: String::new(),
                message: format!("Schema validation failed: {}", e),
                expected: None,
                invalid_value: None,
                suggestion: None,
                doc_link: None,
                mcp_error_metadata: None,
            });
            ParsedSettings {
                settings: None,
                errors,
            }
        }
    }
}

/// Get settings root path for source.
pub fn get_settings_root_path_for_source(
    source: SettingSource,
    cwd: &Path,
    config_home: &Path,
) -> PathBuf {
    match source {
        SettingSource::UserSettings => config_home.to_path_buf(),
        SettingSource::PolicySettings
        | SettingSource::ProjectSettings
        | SettingSource::LocalSettings => cwd.to_path_buf(),
        SettingSource::FlagSettings => cwd.to_path_buf(),
    }
}

/// Get settings file path for source.
pub fn get_settings_file_path_for_source(
    source: SettingSource,
    cwd: &Path,
    config_home: &Path,
    flag_settings_path: Option<&Path>,
) -> Option<PathBuf> {
    match source {
        SettingSource::UserSettings => Some(config_home.join("settings.json")),
        SettingSource::ProjectSettings => Some(cwd.join(".mossen").join("settings.json")),
        SettingSource::LocalSettings => Some(cwd.join(".mossen").join("settings.local.json")),
        SettingSource::PolicySettings => {
            Some(get_managed_file_path().join("managed-settings.json"))
        }
        SettingSource::FlagSettings => flag_settings_path.map(|p| p.to_path_buf()),
    }
}

/// Get relative settings file path for source.
pub fn get_relative_settings_file_path_for_source(source: EditableSettingSource) -> PathBuf {
    match source {
        EditableSettingSource::ProjectSettings => PathBuf::from(".mossen/settings.json"),
        EditableSettingSource::LocalSettings => PathBuf::from(".mossen/settings.local.json"),
        EditableSettingSource::UserSettings => PathBuf::from("settings.json"),
    }
}

/// Deep merge settings - arrays are concatenated and deduplicated.
pub fn merge_settings(base: &mut SettingsJson, overlay: &SettingsJson) {
    let base_val = serde_json::to_value(&*base).unwrap_or(Value::Object(Default::default()));
    let overlay_val = serde_json::to_value(overlay).unwrap_or(Value::Object(Default::default()));
    let merged = deep_merge_values(base_val, overlay_val);
    if let Ok(s) = serde_json::from_value::<SettingsJson>(merged) {
        *base = s;
    }
}

fn deep_merge_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                let merged = if let Some(base_val) = base_map.remove(&key) {
                    deep_merge_values(base_val, overlay_val)
                } else {
                    overlay_val
                };
                base_map.insert(key, merged);
            }
            Value::Object(base_map)
        }
        (Value::Array(mut base_arr), Value::Array(overlay_arr)) => {
            // Arrays: concatenate and deduplicate
            for item in overlay_arr {
                if !base_arr.contains(&item) {
                    base_arr.push(item);
                }
            }
            Value::Array(base_arr)
        }
        (_, overlay) => overlay,
    }
}

/// Get managed settings keys for logging purposes.
pub fn get_managed_settings_keys_for_logging(settings: &SettingsJson) -> Vec<String> {
    let val = serde_json::to_value(settings).unwrap_or(Value::Object(Default::default()));
    let obj = match val.as_object() {
        Some(o) => o,
        None => return vec![],
    };

    let keys_to_expand = ["permissions", "sandbox", "hooks"];
    let mut all_keys = Vec::new();

    for (key, value) in obj {
        if key == "$schema" || (value.is_null()) {
            continue;
        }
        if keys_to_expand.contains(&key.as_str()) {
            if let Some(nested) = value.as_object() {
                for nested_key in nested.keys() {
                    all_keys.push(format!("{}.{}", key, nested_key));
                }
            }
        } else {
            all_keys.push(key.clone());
        }
    }

    all_keys.sort();
    all_keys
}

/// Update settings for a given editable source.
pub fn update_settings_for_source(
    source: EditableSettingSource,
    settings: &SettingsJson,
    cwd: &Path,
    config_home: &Path,
) -> Result<()> {
    let setting_source = match source {
        EditableSettingSource::UserSettings => SettingSource::UserSettings,
        EditableSettingSource::ProjectSettings => SettingSource::ProjectSettings,
        EditableSettingSource::LocalSettings => SettingSource::LocalSettings,
    };

    let file_path = get_settings_file_path_for_source(setting_source, cwd, config_home, None)
        .ok_or_else(|| anyhow!("No file path for source"))?;

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let existing = match std::fs::read_to_string(&file_path) {
        Ok(content) => {
            if content.trim().is_empty() {
                SettingsJson::default()
            } else {
                serde_json::from_str(&content).unwrap_or_default()
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SettingsJson::default(),
        Err(e) => return Err(e.into()),
    };

    let mut merged = existing;
    merge_settings(&mut merged, settings);

    mark_internal_write(&file_path);

    let json_str = serde_json::to_string_pretty(&merged)?;
    std::fs::write(&file_path, format!("{}\n", json_str))?;

    reset_settings_cache();
    Ok(())
}

/// Check if raw settings contain a specific key.
pub fn raw_settings_contains_key(
    key: &str,
    sources: &[SettingSource],
    cwd: &Path,
    config_home: &Path,
) -> bool {
    for source in sources {
        if *source == SettingSource::PolicySettings {
            continue;
        }
        let file_path = match get_settings_file_path_for_source(*source, cwd, config_home, None) {
            Some(p) => p,
            None => continue,
        };
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        if let Ok(data) = serde_json::from_str::<Value>(&content) {
            if let Some(obj) = data.as_object() {
                if obj.contains_key(key) {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns true if any trusted settings source has accepted the bypass permissions prompt.
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
        if let Some(s) = get_source(*source) {
            if s.skip_dangerous_mode_permission_prompt == Some(true) {
                return true;
            }
        }
    }
    false
}

// ─── pluginOnlyPolicy.ts ────────────────────────────────────────────────────

pub type CustomizationSurface = &'static str;

/// Admin-trusted sources that bypass strictPluginOnlyCustomization.
const ADMIN_TRUSTED_SOURCES: &[&str] =
    &["plugin", "policySettings", "built-in", "builtin", "bundled"];

/// Check whether a customization surface is locked to plugin-only sources.
pub fn is_restricted_to_plugin_only(surface: &str, policy_settings: Option<&SettingsJson>) -> bool {
    let policy = match policy_settings {
        Some(s) => &s.strict_plugin_only_customization,
        None => return false,
    };
    match policy {
        Some(Value::Bool(true)) => true,
        Some(Value::Array(arr)) => arr.iter().any(|v| v.as_str() == Some(surface)),
        _ => false,
    }
}

/// Whether a customization's source is admin-trusted.
pub fn is_source_admin_trusted(source: Option<&str>) -> bool {
    match source {
        Some(s) => ADMIN_TRUSTED_SOURCES.contains(&s),
        None => false,
    }
}

// ─── mdm/constants.ts ───────────────────────────────────────────────────────

pub const MACOS_PREFERENCE_DOMAIN: &str = "com.mossen.mossencode";
pub const WINDOWS_REGISTRY_KEY_PATH_HKLM: &str = "HKLM\\SOFTWARE\\Policies\\MossenCode";
pub const WINDOWS_REGISTRY_KEY_PATH_HKCU: &str = "HKCU\\SOFTWARE\\Policies\\MossenCode";
pub const WINDOWS_REGISTRY_VALUE_NAME: &str = "Settings";
pub const PLUTIL_PATH: &str = "/usr/bin/plutil";
pub const MDM_SUBPROCESS_TIMEOUT_MS: u64 = 5000;

/// Build the list of macOS plist paths in priority order.
pub fn get_macos_plist_paths() -> Vec<(PathBuf, &'static str)> {
    let mut paths = Vec::new();

    if let Ok(username) = std::env::var("USER") {
        paths.push((
            PathBuf::from(format!(
                "/Library/Managed Preferences/{}/{}.plist",
                username, MACOS_PREFERENCE_DOMAIN
            )),
            "per-user managed preferences",
        ));
    }

    paths.push((
        PathBuf::from(format!(
            "/Library/Managed Preferences/{}.plist",
            MACOS_PREFERENCE_DOMAIN
        )),
        "device-level managed preferences",
    ));

    if std::env::var("USER_TYPE").as_deref() == Ok("internal") {
        if let Some(home) = dirs::home_dir() {
            paths.push((
                home.join("Library")
                    .join("Preferences")
                    .join(format!("{}.plist", MACOS_PREFERENCE_DOMAIN)),
                "user preferences (internal-only)",
            ));
        }
    }

    paths
}

// ─── mdm/rawRead.ts ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct RawReadResult {
    pub plist_stdouts: Option<Vec<(String, String)>>,
    pub hklm_stdout: Option<String>,
    pub hkcu_stdout: Option<String>,
}

/// Fire fresh subprocess reads for MDM settings and return raw stdout.
pub async fn fire_raw_read() -> RawReadResult {
    if cfg!(target_os = "macos") {
        let plist_paths = get_macos_plist_paths();
        let mut results = Vec::new();

        for (path, label) in plist_paths {
            if !path.exists() {
                continue;
            }
            match tokio::process::Command::new(PLUTIL_PATH)
                .args(&["-convert", "json", "-o", "-", "--", &path.to_string_lossy()])
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    if !stdout.is_empty() {
                        results.push((stdout, label.to_string()));
                        break; // first source wins
                    }
                }
                _ => continue,
            }
        }

        RawReadResult {
            plist_stdouts: Some(results),
            hklm_stdout: None,
            hkcu_stdout: None,
        }
    } else if cfg!(target_os = "windows") {
        let hklm = tokio::process::Command::new("reg")
            .args(&[
                "query",
                WINDOWS_REGISTRY_KEY_PATH_HKLM,
                "/v",
                WINDOWS_REGISTRY_VALUE_NAME,
            ])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

        let hkcu = tokio::process::Command::new("reg")
            .args(&[
                "query",
                WINDOWS_REGISTRY_KEY_PATH_HKCU,
                "/v",
                WINDOWS_REGISTRY_VALUE_NAME,
            ])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

        RawReadResult {
            plist_stdouts: None,
            hklm_stdout: hklm,
            hkcu_stdout: hkcu,
        }
    } else {
        RawReadResult::default()
    }
}

// ─── mdm/settings.ts ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct MdmResult {
    pub settings: SettingsJson,
    pub errors: Vec<ValidationError>,
}

/// Parse reg query stdout to extract a registry string value.
pub fn parse_reg_query_stdout(stdout: &str, value_name: &str) -> Option<String> {
    let escaped = regex::escape(value_name);
    let pattern = format!(r"(?i)^\s+{}\s+REG_(?:EXPAND_)?SZ\s+(.*)$", escaped);
    let re = Regex::new(&pattern).ok()?;
    for line in stdout.lines() {
        if let Some(caps) = re.captures(line) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().trim_end().to_string());
            }
        }
    }
    None
}

/// Parse command output (plutil stdout or registry JSON value) into SettingsJson.
pub fn parse_command_output_as_settings(stdout: &str, source_path: &str) -> MdmResult {
    let mut data: Value = match serde_json::from_str(stdout) {
        Ok(d) => d,
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
        Err(e) => {
            let mut errors = rule_warnings;
            errors.push(ValidationError {
                file: Some(source_path.to_string()),
                path: String::new(),
                message: format!("Schema validation failed: {}", e),
                expected: None,
                invalid_value: None,
                suggestion: None,
                doc_link: None,
                mcp_error_metadata: None,
            });
            MdmResult {
                settings: SettingsJson::default(),
                errors,
            }
        }
    }
}

/// Load managed settings from file-based sources.
pub fn load_managed_file_settings() -> (Option<SettingsJson>, Vec<ValidationError>) {
    let mut errors = Vec::new();
    let mut merged = SettingsJson::default();
    let mut found = false;

    let managed_path = get_managed_file_path().join("managed-settings.json");
    let (settings, base_errors) = parse_settings_file(&managed_path);
    errors.extend(base_errors);
    if let Some(s) = settings {
        merge_settings(&mut merged, &s);
        found = true;
    }

    // Check drop-in directory
    let drop_in_dir = get_managed_settings_drop_in_dir();
    if let Ok(entries) = std::fs::read_dir(&drop_in_dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                name_str.ends_with(".json")
                    && !name_str.starts_with('.')
                    && e.file_type()
                        .map(|ft| ft.is_file() || ft.is_symlink())
                        .unwrap_or(false)
            })
            .collect();
        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in files {
            let (settings, file_errors) = parse_settings_file(&entry.path());
            errors.extend(file_errors);
            if let Some(s) = settings {
                merge_settings(&mut merged, &s);
                found = true;
            }
        }
    }

    if found {
        (Some(merged), errors)
    } else {
        (None, errors)
    }
}

/// Check which file-based managed settings sources are present.
pub fn get_managed_file_settings_presence() -> (bool, bool) {
    let managed_path = get_managed_file_path().join("managed-settings.json");
    let (base_settings, _) = parse_settings_file(&managed_path);
    let has_base = base_settings.is_some();

    let drop_in_dir = get_managed_settings_drop_in_dir();
    let has_drop_ins = std::fs::read_dir(&drop_in_dir)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                name_str.ends_with(".json") && !name_str.starts_with('.')
            })
        })
        .unwrap_or(false);

    (has_base, has_drop_ins)
}

// ─── validateEditTool.ts ────────────────────────────────────────────────────

/// Validate settings file edit to ensure result conforms to schema.
pub fn validate_input_for_settings_file_edit(
    file_path: &str,
    original_content: &str,
    get_updated_content: impl FnOnce() -> String,
) -> Option<String> {
    // Only validate Mossen settings files
    if !file_path.contains(".mossen/settings") && !file_path.ends_with("settings.json") {
        return None;
    }

    // Check if current file is valid
    if validate_settings_file_content(original_content).is_err() {
        return None;
    }

    let updated = get_updated_content();
    match validate_settings_file_content(&updated) {
        Ok(()) => None,
        Err(e) => Some(format!(
            "Mossen settings.json validation failed after edit:\n{}\nIMPORTANT: Do not update the env unless explicitly instructed to do so.",
            e
        )),
    }
}

// ─── schemaOutput.ts ────────────────────────────────────────────────────────

/// Generate JSON Schema for settings.
pub fn generate_settings_json_schema() -> String {
    // Return a simplified schema representation
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "model": { "type": "string" },
            "permissions": { "type": "object" },
            "hooks": { "type": "object" },
            "env": { "type": "object" },
            "cleanupPeriodDays": { "type": "integer", "minimum": 0 },
        }
    });
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

// ─── changeDetector.ts ──────────────────────────────────────────────────────

/// File stability threshold in milliseconds.
pub const FILE_STABILITY_THRESHOLD_MS: u64 = 1000;
/// Polling interval for file stability.
pub const FILE_STABILITY_POLL_INTERVAL_MS: u64 = 500;
/// Internal write window in milliseconds.
pub const INTERNAL_WRITE_WINDOW_MS: u64 = 5000;
/// MDM poll interval in milliseconds (30 minutes).
pub const MDM_POLL_INTERVAL_MS: u64 = 30 * 60 * 1000;
/// Deletion grace period in milliseconds.
pub const DELETION_GRACE_MS: u64 =
    FILE_STABILITY_THRESHOLD_MS + FILE_STABILITY_POLL_INTERVAL_MS + 200;

/// Config change source mapping.
pub fn setting_source_to_config_change_source(source: SettingSource) -> &'static str {
    match source {
        SettingSource::UserSettings => "user_settings",
        SettingSource::ProjectSettings => "project_settings",
        SettingSource::LocalSettings => "local_settings",
        SettingSource::FlagSettings | SettingSource::PolicySettings => "policy_settings",
    }
}

/// Settings change detector state.
pub struct SettingsChangeDetector {
    initialized: bool,
    disposed: bool,
    last_mdm_snapshot: Option<String>,
    subscribers: Vec<Box<dyn Fn(SettingSource) + Send + Sync>>,
}

impl SettingsChangeDetector {
    pub fn new() -> Self {
        Self {
            initialized: false,
            disposed: false,
            last_mdm_snapshot: None,
            subscribers: Vec::new(),
        }
    }

    /// Initialize file watching.
    pub async fn initialize(&mut self) {
        if self.initialized || self.disposed {
            return;
        }
        self.initialized = true;
        // In Rust, we'd use notify crate for file watching.
        // For now, the structure is in place.
    }

    /// Dispose and clean up.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.last_mdm_snapshot = None;
        self.subscribers.clear();
    }

    /// Subscribe to settings changes.
    pub fn subscribe(&mut self, callback: Box<dyn Fn(SettingSource) + Send + Sync>) {
        self.subscribers.push(callback);
    }

    /// Notify listeners of a settings change.
    pub fn notify_change(&self, source: SettingSource) {
        reset_settings_cache();
        for subscriber in &self.subscribers {
            subscriber(source);
        }
    }

    /// Reset for testing.
    pub fn reset_for_testing(&mut self) {
        self.initialized = false;
        self.disposed = false;
        self.last_mdm_snapshot = None;
    }
}

impl Default for SettingsChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ─── allErrors.ts ───────────────────────────────────────────────────────────

/// Get merged settings with all validation errors, including MCP config errors.
pub fn get_settings_with_all_errors(
    settings_with_errors: SettingsWithErrors,
    mcp_errors: Vec<ValidationError>,
) -> SettingsWithErrors {
    let mut errors = settings_with_errors.errors;
    errors.extend(mcp_errors);
    SettingsWithErrors {
        settings: settings_with_errors.settings,
        errors,
    }
}

/// Load the parsed `SettingsJson` for a single source directly from disk
/// (no caching). Used by callers that need one specific layer — typically the
/// `policySettings` layer to decide whether managed hooks override user
/// hooks. Returns `None` when the file doesn't exist or fails to parse.
pub fn load_settings_for_source(source: SettingSource) -> Option<SettingsJson> {
    let cwd = std::path::PathBuf::from(crate::cwd::pwd());
    let config_home = crate::naming::get_resolved_config_home_dir();
    let flag_path = std::env::var_os("MOSSEN_FLAG_SETTINGS").map(std::path::PathBuf::from);
    let path = get_settings_file_path_for_source(source, &cwd, &config_home, flag_path.as_deref())?;
    if !path.exists() {
        return None;
    }
    parse_settings_file(&path).0
}

/// End-to-end loader: read every enabled setting source from disk, merge in
/// priority order, return the merged `SettingsJson` plus any validation
/// errors collected along the way.
///
/// Mirrors TS `loadSettingsFromDisk` in `utils/settings/settings.ts` — the
/// helper that `getSettingsWithErrors` falls through to when there's no
/// session cache hit.
///
/// Paths:
///   * `UserSettings`   → `{config_home}/settings.json`
///   * `ProjectSettings`→ `{cwd}/.mossen/settings.json`
///   * `LocalSettings`  → `{cwd}/.mossen/settings.local.json`
///   * `PolicySettings` → managed-settings.json (resolved via
///     `get_managed_file_path`)
///   * `FlagSettings`   → flag-provided override (`MOSSEN_FLAG_SETTINGS`)
///
/// Sources are merged low-to-high so later sources override earlier ones,
/// matching the TS contract:
///   `userSettings → projectSettings → localSettings → policySettings → flagSettings`.
pub fn load_settings_from_disk() -> SettingsWithErrors {
    let cwd = std::path::PathBuf::from(crate::cwd::pwd());
    let config_home = crate::naming::get_resolved_config_home_dir();
    let flag_path = std::env::var_os("MOSSEN_FLAG_SETTINGS").map(std::path::PathBuf::from);

    // Lowest-to-highest priority — later entries win on key collision.
    let order: &[SettingSource] = &[
        SettingSource::UserSettings,
        SettingSource::ProjectSettings,
        SettingSource::LocalSettings,
        SettingSource::PolicySettings,
        SettingSource::FlagSettings,
    ];

    let mut effective = SettingsJson::default();
    let mut errors = Vec::new();

    for &source in order {
        let path =
            get_settings_file_path_for_source(source, &cwd, &config_home, flag_path.as_deref());
        let Some(path) = path else { continue };
        if !path.exists() {
            continue;
        }
        let (parsed, parse_errors) = parse_settings_file(&path);
        errors.extend(parse_errors);
        if let Some(s) = parsed {
            merge_settings(&mut effective, &s);
        }
    }

    SettingsWithErrors {
        settings: effective,
        errors,
    }
}

/// 对应 TS `getSettingsWithErrors`：load + merge settings 并保留每个源的解析错误。
///
/// Rust 端没有 IO 副作用入口，因此把 per-source 调用注入为 `get_source` 闭包；
/// 返回的 `SettingsWithErrors` 仅累积 `get_source` 返回的错误（实际文件解析错误
/// 由调用方在 `get_source` 中转换为 [`ValidationError`]）。
pub fn get_settings_with_errors(
    sources: &[SettingSource],
    get_source: impl Fn(SettingSource) -> std::result::Result<Option<SettingsJson>, ValidationError>,
    initial: SettingsJson,
) -> SettingsWithErrors {
    let mut effective = initial;
    let mut errors = Vec::new();
    for source in sources {
        match get_source(*source) {
            Ok(Some(s)) => {
                if let (Ok(a), Ok(b)) = (serde_json::to_value(&effective), serde_json::to_value(&s))
                {
                    if let serde_json::Value::Object(mut a_obj) = a {
                        if let serde_json::Value::Object(b_obj) = b {
                            for (k, v) in b_obj {
                                a_obj.insert(k, v);
                            }
                        }
                        if let Ok(merged) =
                            serde_json::from_value::<SettingsJson>(serde_json::Value::Object(a_obj))
                        {
                            effective = merged;
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    SettingsWithErrors {
        settings: effective,
        errors,
    }
}

// ─── applySettingsChange.ts ─────────────────────────────────────────────────

/// Settings with sources for debugging.
#[derive(Debug, Clone)]
pub struct SettingsWithSources {
    pub effective: SettingsJson,
    pub sources: Vec<(SettingSource, SettingsJson)>,
}

/// Get the effective merged settings alongside the raw per-source settings.
pub fn get_settings_with_sources(
    sources: &[SettingSource],
    get_source: impl Fn(SettingSource) -> Option<SettingsJson>,
    get_initial: impl Fn() -> SettingsJson,
) -> SettingsWithSources {
    reset_settings_cache();
    let mut result_sources = Vec::new();
    for source in sources {
        if let Some(settings) = get_source(*source) {
            let val = serde_json::to_value(&settings).unwrap_or(Value::Object(Default::default()));
            if let Some(obj) = val.as_object() {
                if !obj.is_empty() {
                    result_sources.push((*source, settings));
                }
            }
        }
    }
    SettingsWithSources {
        effective: get_initial(),
        sources: result_sources,
    }
}

/// Get the origin of the highest-priority active policy settings source.
pub fn get_policy_settings_origin(
    remote_settings: Option<&SettingsJson>,
    mdm_settings: &MdmResult,
    file_settings: Option<&SettingsJson>,
    hkcu_settings: &MdmResult,
) -> Option<&'static str> {
    if let Some(remote) = remote_settings {
        let val = serde_json::to_value(remote).unwrap_or(Value::Object(Default::default()));
        if val.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            return Some("remote");
        }
    }

    let mdm_val =
        serde_json::to_value(&mdm_settings.settings).unwrap_or(Value::Object(Default::default()));
    if mdm_val.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        if cfg!(target_os = "macos") {
            return Some("plist");
        } else {
            return Some("hklm");
        }
    }

    if file_settings.is_some() {
        return Some("file");
    }

    let hkcu_val =
        serde_json::to_value(&hkcu_settings.settings).unwrap_or(Value::Object(Default::default()));
    if hkcu_val.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        return Some("hkcu");
    }

    None
}

// =============================================================================
// `XxxSchema` 别名 — 对应 TS Zod 导出。Rust 用结构体承载，别名指向同一类型。
// =============================================================================

/// Alias for the extra known marketplace validator (mirrors TS `ExtraKnownMarketplaceSchema`).
pub type ExtraKnownMarketplaceSchema = ExtraKnownMarketplace;
/// Alias for the allowed MCP server entry validator (mirrors TS `AllowedMcpServerEntrySchema`).
pub type AllowedMcpServerEntrySchema = AllowedMcpServerEntry;
/// Alias for the denied MCP server entry validator (mirrors TS `DeniedMcpServerEntrySchema`).
pub type DeniedMcpServerEntrySchema = DeniedMcpServerEntry;
/// Alias for the permission rule string validator (mirrors TS `PermissionRuleSchema`).
/// In TS this is a `z.string().superRefine(...)` calling `validatePermissionRule`.
/// Rust represents the value as a plain string; use `validate_permission_rule` to validate.
pub type PermissionRuleSchema = String;
