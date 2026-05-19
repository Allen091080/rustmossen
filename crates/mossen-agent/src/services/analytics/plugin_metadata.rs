//! Plugin metadata — analytics metadata enrichment for MCP plugins.

use std::collections::{HashMap, HashSet};
use std::path::MAIN_SEPARATOR;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Plugin metadata collected for analytics events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub plugin_name: String,
    pub plugin_version: Option<String>,
    pub plugin_source: PluginSource,
    pub tool_count: usize,
    pub prompt_count: usize,
    pub resource_count: usize,
}

/// Source of a plugin (how it was discovered/installed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginSource {
    Builtin,
    UserInstalled,
    ProjectConfig,
    McpServer,
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/analytics/pluginMetadata.ts` exports.
// ---------------------------------------------------------------------------

const PLUGIN_ID_HASH_SALT: &str = "mossen-plugin-telemetry-v1";
const BUILTIN_MARKETPLACE_NAME: &str = "builtin";

/// `pluginMetadata.ts` `hashPluginId` — opaque per-plugin aggregation key.
/// `name@<lowercased-marketplace>` || salt, SHA-256, first 16 hex chars.
pub fn hash_plugin_id(name: &str, marketplace: Option<&str>) -> String {
    let key = match marketplace {
        Some(m) => format!("{}@{}", name, m.to_lowercase()),
        None => name.to_string(),
    };
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(PLUGIN_ID_HASH_SALT.as_bytes());
    let digest = hasher.finalize();
    let hex = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    hex[..16].to_string()
}

/// `pluginMetadata.ts` `TelemetryPluginScope` — 4-value plugin origin enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetryPluginScope {
    Official,
    Org,
    UserLocal,
    DefaultBundle,
}

impl TelemetryPluginScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            TelemetryPluginScope::Official => "official",
            TelemetryPluginScope::Org => "org",
            TelemetryPluginScope::UserLocal => "user-local",
            TelemetryPluginScope::DefaultBundle => "default-bundle",
        }
    }
}

/// `pluginMetadata.ts` `EnabledVia`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnabledVia {
    UserInstall,
    OrgPolicy,
    DefaultEnable,
    SeedMount,
}

impl EnabledVia {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnabledVia::UserInstall => "user-install",
            EnabledVia::OrgPolicy => "org-policy",
            EnabledVia::DefaultEnable => "default-enable",
            EnabledVia::SeedMount => "seed-mount",
        }
    }
}

/// `pluginMetadata.ts` `InvocationTrigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvocationTrigger {
    UserSlash,
    MossenProactive,
    NestedSkill,
}

/// `pluginMetadata.ts` `SkillExecutionContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillExecutionContext {
    Fork,
    Inline,
    Remote,
}

/// `pluginMetadata.ts` `InstallSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallSource {
    CliExplicit,
    UiDiscover,
    UiSuggestion,
    DeepLink,
}

/// `pluginMetadata.ts` `PluginCommandErrorCategory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginCommandErrorCategory {
    Network,
    NotFound,
    Permission,
    Validation,
    Unknown,
}

/// Simplified plugin descriptor mirroring the fields used by TS pluginMetadata.
#[derive(Debug, Clone, Default)]
pub struct LoadedPluginInfo {
    pub name: String,
    pub repository: String,
    pub path: String,
    pub is_builtin: bool,
    pub manifest_version: Option<String>,
    pub skills_path: Option<String>,
    pub skills_paths: Vec<String>,
    pub commands_path: Option<String>,
    pub commands_paths: Vec<String>,
    pub manifest_has_mcp_servers: bool,
    pub hooks_config_present: bool,
}

/// Minimal manifest helper.
#[derive(Debug, Clone, Default)]
pub struct PluginManifestInfo {
    pub name: String,
}

/// Mirror of `pluginIdentifier.ts` `isOfficialMarketplaceName`.
fn is_official_marketplace_name(marketplace: Option<&str>) -> bool {
    matches!(marketplace, Some("mossen-official") | Some("mossen") | Some("anthropic"))
}

/// `pluginMetadata.ts` `getTelemetryPluginScope`.
pub fn get_telemetry_plugin_scope(
    name: &str,
    marketplace: Option<&str>,
    managed_names: Option<&HashSet<String>>,
) -> TelemetryPluginScope {
    if marketplace == Some(BUILTIN_MARKETPLACE_NAME) {
        return TelemetryPluginScope::DefaultBundle;
    }
    if is_official_marketplace_name(marketplace) {
        return TelemetryPluginScope::Official;
    }
    if let Some(set) = managed_names {
        if set.contains(name) {
            return TelemetryPluginScope::Org;
        }
    }
    TelemetryPluginScope::UserLocal
}

/// `pluginMetadata.ts` `getEnabledVia`.
pub fn get_enabled_via(
    plugin: &LoadedPluginInfo,
    managed_names: Option<&HashSet<String>>,
    seed_dirs: &[String],
) -> EnabledVia {
    if plugin.is_builtin {
        return EnabledVia::DefaultEnable;
    }
    if let Some(set) = managed_names {
        if set.contains(&plugin.name) {
            return EnabledVia::OrgPolicy;
        }
    }
    for dir in seed_dirs {
        let with_sep = if dir.ends_with(MAIN_SEPARATOR) {
            dir.clone()
        } else {
            format!("{}{}", dir, MAIN_SEPARATOR)
        };
        if plugin.path.starts_with(&with_sep) {
            return EnabledVia::SeedMount;
        }
    }
    EnabledVia::UserInstall
}

/// Output of `buildPluginTelemetryFields`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginTelemetryFields {
    pub plugin_id_hash: String,
    pub plugin_scope: String,
    pub plugin_name_redacted: String,
    pub marketplace_name_redacted: String,
    pub is_official_plugin: bool,
}

/// `pluginMetadata.ts` `buildPluginTelemetryFields`.
pub fn build_plugin_telemetry_fields(
    name: &str,
    marketplace: Option<&str>,
    managed_names: Option<&HashSet<String>>,
) -> PluginTelemetryFields {
    let scope = get_telemetry_plugin_scope(name, marketplace, managed_names);
    let is_mossen_controlled = matches!(
        scope,
        TelemetryPluginScope::Official | TelemetryPluginScope::DefaultBundle
    );
    PluginTelemetryFields {
        plugin_id_hash: hash_plugin_id(name, marketplace),
        plugin_scope: scope.as_str().to_string(),
        plugin_name_redacted: if is_mossen_controlled {
            name.to_string()
        } else {
            "third-party".to_string()
        },
        marketplace_name_redacted: if is_mossen_controlled {
            marketplace.unwrap_or("third-party").to_string()
        } else {
            "third-party".to_string()
        },
        is_official_plugin: is_mossen_controlled,
    }
}

/// Parse `repo@marketplace` style plugin identifiers — minimal port of
/// `utils/plugins/pluginIdentifier.ts` `parsePluginIdentifier`.
fn parse_plugin_identifier(s: &str) -> (String, Option<String>) {
    if let Some(idx) = s.rfind('@') {
        let (name, rest) = s.split_at(idx);
        (name.to_string(), Some(rest[1..].to_string()))
    } else {
        (s.to_string(), None)
    }
}

/// `pluginMetadata.ts` `buildPluginCommandTelemetryFields`.
pub fn build_plugin_command_telemetry_fields(
    manifest: &PluginManifestInfo,
    repository: &str,
    managed_names: Option<&HashSet<String>>,
) -> PluginTelemetryFields {
    let (_, marketplace) = parse_plugin_identifier(repository);
    build_plugin_telemetry_fields(&manifest.name, marketplace.as_deref(), managed_names)
}

/// `pluginMetadata.ts` `logPluginsEnabledForSession` — produce one telemetry
/// payload per plugin. Caller forwards to event sink (we don't presume a
/// concrete logger here).
pub fn log_plugins_enabled_for_session(
    plugins: &[LoadedPluginInfo],
    managed_names: Option<&HashSet<String>>,
    seed_dirs: &[String],
) -> Vec<HashMap<String, Value>> {
    let mut out = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let (_, marketplace) = parse_plugin_identifier(&plugin.repository);
        let fields = build_plugin_telemetry_fields(&plugin.name, marketplace.as_deref(), managed_names);
        let mut map = HashMap::new();
        map.insert("_PROTO_plugin_name".to_string(), Value::String(plugin.name.clone()));
        if let Some(m) = marketplace {
            map.insert("_PROTO_marketplace_name".to_string(), Value::String(m));
        }
        map.insert("plugin_id_hash".to_string(), Value::String(fields.plugin_id_hash));
        map.insert("plugin_scope".to_string(), Value::String(fields.plugin_scope));
        map.insert("plugin_name_redacted".to_string(), Value::String(fields.plugin_name_redacted));
        map.insert(
            "marketplace_name_redacted".to_string(),
            Value::String(fields.marketplace_name_redacted),
        );
        map.insert("is_official_plugin".to_string(), Value::Bool(fields.is_official_plugin));
        map.insert(
            "enabled_via".to_string(),
            Value::String(get_enabled_via(plugin, managed_names, seed_dirs).as_str().to_string()),
        );
        let skill_path_count = (if plugin.skills_path.is_some() { 1 } else { 0 })
            + plugin.skills_paths.len();
        let command_path_count = (if plugin.commands_path.is_some() { 1 } else { 0 })
            + plugin.commands_paths.len();
        map.insert(
            "skill_path_count".to_string(),
            Value::Number(serde_json::Number::from(skill_path_count)),
        );
        map.insert(
            "command_path_count".to_string(),
            Value::Number(serde_json::Number::from(command_path_count)),
        );
        map.insert("has_mcp".to_string(), Value::Bool(plugin.manifest_has_mcp_servers));
        map.insert("has_hooks".to_string(), Value::Bool(plugin.hooks_config_present));
        if let Some(v) = &plugin.manifest_version {
            map.insert("version".to_string(), Value::String(v.clone()));
        }
        out.push(map);
    }
    out
}

/// `pluginMetadata.ts` `classifyPluginCommandError`.
pub fn classify_plugin_command_error(message: &str) -> PluginCommandErrorCategory {
    let re_network = regex::Regex::new(
        r"(?i)ENOTFOUND|ECONNREFUSED|EAI_AGAIN|ETIMEDOUT|ECONNRESET|network|Could not resolve|Connection refused|timed out",
    )
    .unwrap();
    if re_network.is_match(message) {
        return PluginCommandErrorCategory::Network;
    }
    let re_not_found = regex::Regex::new(r"(?i)\b404\b|not found|does not exist|no such plugin").unwrap();
    if re_not_found.is_match(message) {
        return PluginCommandErrorCategory::NotFound;
    }
    let re_perm = regex::Regex::new(r"(?i)\b40[13]\b|EACCES|EPERM|permission denied|unauthorized").unwrap();
    if re_perm.is_match(message) {
        return PluginCommandErrorCategory::Permission;
    }
    let re_val = regex::Regex::new(r"(?i)invalid|malformed|schema|validation|parse error").unwrap();
    if re_val.is_match(message) {
        return PluginCommandErrorCategory::Validation;
    }
    PluginCommandErrorCategory::Unknown
}

/// Plugin load error descriptor.
#[derive(Debug, Clone)]
pub struct PluginLoadError {
    pub source: String,
    pub error_type: String,
    pub plugin: Option<String>,
}

/// `pluginMetadata.ts` `logPluginLoadErrors`.
pub fn log_plugin_load_errors(
    errors: &[PluginLoadError],
    managed_names: Option<&HashSet<String>>,
) -> Vec<HashMap<String, Value>> {
    let mut out = Vec::with_capacity(errors.len());
    for err in errors {
        let (parsed_name, marketplace) = parse_plugin_identifier(&err.source);
        let plugin_name = err
            .plugin
            .clone()
            .unwrap_or(parsed_name);
        let fields = build_plugin_telemetry_fields(&plugin_name, marketplace.as_deref(), managed_names);
        let mut map = HashMap::new();
        map.insert("error_category".to_string(), Value::String(err.error_type.clone()));
        map.insert("_PROTO_plugin_name".to_string(), Value::String(plugin_name));
        if let Some(m) = marketplace {
            map.insert("_PROTO_marketplace_name".to_string(), Value::String(m));
        }
        map.insert("plugin_id_hash".to_string(), Value::String(fields.plugin_id_hash));
        map.insert("plugin_scope".to_string(), Value::String(fields.plugin_scope));
        map.insert("plugin_name_redacted".to_string(), Value::String(fields.plugin_name_redacted));
        map.insert(
            "marketplace_name_redacted".to_string(),
            Value::String(fields.marketplace_name_redacted),
        );
        map.insert("is_official_plugin".to_string(), Value::Bool(fields.is_official_plugin));
        out.push(map);
    }
    out
}

/// Collect metadata for all active plugins.
pub fn collect_plugin_metadata(plugins: &[PluginMetadata]) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    map.insert("plugin_count".to_string(), Value::Number(serde_json::Number::from(plugins.len())));
    let total_tools: usize = plugins.iter().map(|p| p.tool_count).sum();
    map.insert("total_tool_count".to_string(), Value::Number(serde_json::Number::from(total_tools)));
    let builtin_count = plugins.iter().filter(|p| matches!(p.plugin_source, PluginSource::Builtin)).count();
    let user_count = plugins.iter().filter(|p| matches!(p.plugin_source, PluginSource::UserInstalled)).count();
    let mcp_count = plugins.iter().filter(|p| matches!(p.plugin_source, PluginSource::McpServer)).count();
    map.insert("builtin_plugin_count".to_string(), Value::Number(serde_json::Number::from(builtin_count)));
    map.insert("user_plugin_count".to_string(), Value::Number(serde_json::Number::from(user_count)));
    map.insert("mcp_plugin_count".to_string(), Value::Number(serde_json::Number::from(mcp_count)));
    map
}
