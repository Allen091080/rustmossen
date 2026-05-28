//! Plugin schema definitions — types, validation, and constants.
//!
//! Translated from `utils/plugins/schemas.ts` (1680 lines).
//! Provides all core types used across the plugin subsystem.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ── Official marketplace names ──

/// Official marketplace names reserved for Mossen official use.
pub static ALLOWED_OFFICIAL_MARKETPLACE_NAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("mossen-code-marketplace");
    s.insert("mossen-code-plugins");
    s.insert("mossen-plugins-official");
    s.insert("mossen-marketplace");
    s.insert("mossen-plugins");
    s.insert("agent-skills");
    s.insert("life-sciences");
    s.insert("knowledge-work-plugins");
    s
});

/// Official marketplaces that should NOT auto-update by default.
static NO_AUTO_UPDATE_OFFICIAL_MARKETPLACES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::new();
    s.insert("knowledge-work-plugins");
    s
});

/// Check if auto-update is enabled for a marketplace.
pub fn is_marketplace_auto_update(marketplace_name: &str, auto_update: Option<bool>) -> bool {
    let normalized = marketplace_name.to_lowercase();
    match auto_update {
        Some(v) => v,
        None => {
            ALLOWED_OFFICIAL_MARKETPLACE_NAMES.contains(normalized.as_str())
                && !NO_AUTO_UPDATE_OFFICIAL_MARKETPLACES.contains(normalized.as_str())
        }
    }
}

/// Pattern to detect names that impersonate official Mossen marketplaces.
static BLOCKED_OFFICIAL_NAME_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:official[^a-z0-9]*(mossen)|(?:mossen)[^a-z0-9]*official|^(?:mossen)[^a-z0-9]*(marketplace|plugins|official))").unwrap()
});

/// Pattern to detect non-ASCII characters (homograph attack prevention).
static NON_ASCII_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\x20-\x7E]").unwrap());

/// Check if a marketplace name impersonates an official Mossen marketplace.
pub fn is_blocked_official_name(name: &str) -> bool {
    if ALLOWED_OFFICIAL_MARKETPLACE_NAMES.contains(name.to_lowercase().as_str()) {
        return false;
    }
    if NON_ASCII_PATTERN.is_match(name) {
        return true;
    }
    BLOCKED_OFFICIAL_NAME_PATTERN.is_match(name)
}

/// The official GitHub organization for Mossen marketplaces.
pub const OFFICIAL_GITHUB_ORG: &str = "mossen";

/// Validate that a marketplace with a reserved name comes from the official source.
pub fn validate_official_name_source(
    name: &str,
    source_type: &str,
    repo: Option<&str>,
    url: Option<&str>,
) -> Option<String> {
    let normalized = name.to_lowercase();
    if !ALLOWED_OFFICIAL_MARKETPLACE_NAMES.contains(normalized.as_str()) {
        return None;
    }

    if source_type == "github" {
        let r = repo.unwrap_or("");
        if !r
            .to_lowercase()
            .starts_with(&format!("{}/", OFFICIAL_GITHUB_ORG))
        {
            return Some(format!(
                "The name '{}' is reserved for official Mossen marketplaces. Only repositories from 'github.com/{}/' can use this name.",
                name, OFFICIAL_GITHUB_ORG
            ));
        }
        return None;
    }

    if source_type == "git" {
        if let Some(u) = url {
            let lower = u.to_lowercase();
            if lower.contains("github.com/mossen/") || lower.contains("git@github.com:mossen/") {
                return None;
            }
            return Some(format!(
                "The name '{}' is reserved for official Mossen marketplaces. Only repositories from 'github.com/{}/' can use this name.",
                name, OFFICIAL_GITHUB_ORG
            ));
        }
    }

    Some(format!(
        "The name '{}' is reserved for official Mossen marketplaces and can only be used with GitHub sources from the '{}' organization.",
        name, OFFICIAL_GITHUB_ORG
    ))
}

// ── Marketplace name validation ──

/// Validate a marketplace name string.
pub fn validate_marketplace_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Marketplace must have a name".into());
    }
    if name.contains(' ') {
        return Err(
            "Marketplace name cannot contain spaces. Use kebab-case (e.g., \"my-marketplace\")"
                .into(),
        );
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") || name == "." {
        return Err("Marketplace name cannot contain path separators (/ or \\), \"..\" sequences, or be \".\"".into());
    }
    if is_blocked_official_name(name) {
        return Err("Marketplace name impersonates an official Mossen marketplace".into());
    }
    let lower = name.to_lowercase();
    if lower == "inline" {
        return Err(
            "Marketplace name \"inline\" is reserved for --plugin-dir session plugins".into(),
        );
    }
    if lower == "builtin" {
        return Err("Marketplace name \"builtin\" is reserved for built-in plugins".into());
    }
    Ok(())
}

// ── Plugin ID validation ──

static PLUGIN_ID_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^[a-z0-9][-a-z0-9._]*@[a-z0-9][-a-z0-9._]*$").unwrap());

/// Validate a plugin ID string (format: plugin@marketplace).
pub fn validate_plugin_id(id: &str) -> bool {
    PLUGIN_ID_REGEX.is_match(id)
}

/// Plugin ID type alias.
pub type PluginId = String;

// ── Type definitions ──

/// Plugin author information.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Plugin scopes for installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginScope {
    Managed,
    User,
    Project,
    Local,
}

impl std::fmt::Display for PluginScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginScope::Managed => write!(f, "managed"),
            PluginScope::User => write!(f, "user"),
            PluginScope::Project => write!(f, "project"),
            PluginScope::Local => write!(f, "local"),
        }
    }
}

/// Command metadata for plugin commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "argumentHint", skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(rename = "allowedTools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
}

/// User config option type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserConfigOptionType {
    String,
    Number,
    Boolean,
    Directory,
    File,
}

/// A single user-configurable option in plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUserConfigOption {
    #[serde(rename = "type")]
    pub option_type: UserConfigOptionType,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// LSP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(rename = "extensionToLanguage")]
    pub extension_to_language: HashMap<String, String>,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(
        rename = "initializationOptions",
        skip_serializing_if = "Option::is_none"
    )]
    pub initialization_options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(rename = "workspaceFolder", skip_serializing_if = "Option::is_none")]
    pub workspace_folder: Option<String>,
    #[serde(rename = "startupTimeout", skip_serializing_if = "Option::is_none")]
    pub startup_timeout: Option<u64>,
    #[serde(rename = "shutdownTimeout", skip_serializing_if = "Option::is_none")]
    pub shutdown_timeout: Option<u64>,
    #[serde(rename = "restartOnCrash", skip_serializing_if = "Option::is_none")]
    pub restart_on_crash: Option<bool>,
    #[serde(rename = "maxRestarts", skip_serializing_if = "Option::is_none")]
    pub max_restarts: Option<u32>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

/// Channel declaration in plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifestChannel {
    pub server: String,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(rename = "userConfig", skip_serializing_if = "Option::is_none")]
    pub user_config: Option<HashMap<String, PluginUserConfigOption>>,
}

/// Plugin manifest (plugin.json).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginManifest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<PluginAuthor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<serde_json::Value>,
    #[serde(rename = "outputStyles", skip_serializing_if = "Option::is_none")]
    pub output_styles: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<PluginManifestChannel>>,
    #[serde(rename = "mcpServers", skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<serde_json::Value>,
    #[serde(rename = "lspServers", skip_serializing_if = "Option::is_none")]
    pub lsp_servers: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "userConfig", skip_serializing_if = "Option::is_none")]
    pub user_config: Option<HashMap<String, PluginUserConfigOption>>,
}

/// Marketplace source — discriminated union on `source` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "source")]
pub enum MarketplaceSource {
    #[serde(rename = "url")]
    Url {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(rename = "sparsePaths", skip_serializing_if = "Option::is_none")]
        sparse_paths: Option<Vec<String>>,
    },
    #[serde(rename = "git")]
    Git {
        url: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(rename = "sparsePaths", skip_serializing_if = "Option::is_none")]
        sparse_paths: Option<Vec<String>>,
    },
    #[serde(rename = "npm")]
    Npm { package: String },
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "directory")]
    Directory { path: String },
    #[serde(rename = "hostPattern")]
    HostPattern {
        #[serde(rename = "hostPattern")]
        host_pattern: String,
    },
    #[serde(rename = "pathPattern")]
    PathPattern {
        #[serde(rename = "pathPattern")]
        path_pattern: String,
    },
    #[serde(rename = "settings")]
    Settings {
        name: String,
        plugins: Vec<SettingsMarketplacePlugin>,
        #[serde(skip_serializing_if = "Option::is_none")]
        owner: Option<PluginAuthor>,
    },
}

/// Plugin source — where to fetch a plugin from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PluginSource {
    /// Relative path starting with "./"
    RelativePath(String),
    /// Structured source object
    Structured(StructuredPluginSource),
}

impl Default for PluginSource {
    fn default() -> Self {
        PluginSource::RelativePath(String::new())
    }
}

impl PluginSource {
    /// Convenience constructors matching TS variant naming used in translated code.
    pub fn is_local(&self) -> bool {
        matches!(self, PluginSource::RelativePath(_))
    }
}

impl std::fmt::Display for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginSource::RelativePath(p) => write!(f, "{}", p),
            PluginSource::Structured(s) => write!(f, "{:?}", s),
        }
    }
}

/// Structured plugin source types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "source")]
pub enum StructuredPluginSource {
    #[serde(rename = "npm")]
    Npm {
        package: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        registry: Option<String>,
    },
    #[serde(rename = "pip")]
    Pip {
        package: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        registry: Option<String>,
    },
    #[serde(rename = "url")]
    Url {
        url: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
    #[serde(rename = "github")]
    GitHub {
        repo: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
    #[serde(rename = "git-subdir")]
    GitSubdir {
        url: String,
        path: String,
        #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha: Option<String>,
    },
}

/// Check if a plugin source is a local path (relative, starts with "./").
pub fn is_local_plugin_source(source: &PluginSource) -> bool {
    match source {
        PluginSource::RelativePath(p) => p.starts_with("./"),
        PluginSource::Structured(_) => false,
    }
}

/// Whether a marketplace source points at a local filesystem path.
pub fn is_local_marketplace_source(source: &MarketplaceSource) -> bool {
    matches!(
        source,
        MarketplaceSource::File { .. } | MarketplaceSource::Directory { .. }
    )
}

/// Settings marketplace plugin (narrow schema for inline settings).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsMarketplacePlugin {
    pub name: String,
    pub source: PluginSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Plugin marketplace entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginMarketplaceEntry {
    pub name: String,
    pub source: PluginSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_strict")]
    pub strict: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<PluginAuthor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<serde_json::Value>,
    #[serde(rename = "outputStyles", skip_serializing_if = "Option::is_none")]
    pub output_styles: Option<serde_json::Value>,
    #[serde(rename = "mcpServers", skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<serde_json::Value>,
    #[serde(rename = "lspServers", skip_serializing_if = "Option::is_none")]
    pub lsp_servers: Option<serde_json::Value>,
    #[serde(rename = "userConfig", skip_serializing_if = "Option::is_none")]
    pub user_config: Option<HashMap<String, PluginUserConfigOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<PluginManifestChannel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, serde_json::Value>>,
}

fn default_strict() -> bool {
    true
}

/// Marketplace metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarketplaceMetadata {
    #[serde(rename = "pluginRoot", skip_serializing_if = "Option::is_none")]
    pub plugin_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Plugin marketplace definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMarketplace {
    pub name: String,
    pub owner: PluginAuthor,
    pub plugins: Vec<PluginMarketplaceEntry>,
    #[serde(
        rename = "forceRemoveDeletedPlugins",
        skip_serializing_if = "Option::is_none"
    )]
    pub force_remove_deleted_plugins: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MarketplaceMetadata>,
    #[serde(
        rename = "allowCrossMarketplaceDependenciesOn",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_cross_marketplace_dependencies_on: Option<Vec<String>>,
}

/// Installed plugin metadata (V1 format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub version: String,
    #[serde(rename = "installedAt")]
    pub installed_at: String,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(rename = "installPath")]
    pub install_path: String,
    #[serde(rename = "gitCommitSha", skip_serializing_if = "Option::is_none")]
    pub git_commit_sha: Option<String>,
}

/// Plugin installation entry (V2 format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstallationEntry {
    pub scope: PluginScope,
    #[serde(rename = "projectPath", skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(rename = "installPath")]
    pub install_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "installedAt", skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(rename = "gitCommitSha", skip_serializing_if = "Option::is_none")]
    pub git_commit_sha: Option<String>,
}

/// Installed plugins file V1 format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginsFileV1 {
    pub version: u32,
    pub plugins: HashMap<String, InstalledPlugin>,
}

/// Installed plugins file V2 format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginsFileV2 {
    pub version: u32,
    pub plugins: HashMap<String, Vec<PluginInstallationEntry>>,
}

impl Default for InstalledPluginsFileV2 {
    fn default() -> Self {
        Self {
            version: 2,
            plugins: HashMap::new(),
        }
    }
}

/// Known marketplace entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownMarketplace {
    pub source: MarketplaceSource,
    #[serde(rename = "installLocation")]
    pub install_location: String,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(rename = "autoUpdate", skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
}

/// Known marketplaces file (Record<string, KnownMarketplace>).
pub type KnownMarketplacesFile = HashMap<String, KnownMarketplace>;

/// Settings JSON structure (settings.json content).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsJson {
    #[serde(rename = "enabledPlugins", skip_serializing_if = "Option::is_none")]
    pub enabled_plugins: Option<HashMap<String, bool>>,
    #[serde(
        rename = "extraKnownMarketplaces",
        skip_serializing_if = "Option::is_none"
    )]
    pub extra_known_marketplaces: Option<HashMap<String, serde_json::Value>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Mossen hint (recommendation) entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenHint {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// The value associated with the hint (e.g. plugin identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The command that sourced this hint.
    #[serde(rename = "sourceCommand", skip_serializing_if = "Option::is_none")]
    pub source_command: Option<String>,
}

/// MCP server configuration in plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "serverType", skip_serializing_if = "Option::is_none")]
    pub server_type: Option<String>,
    #[serde(rename = "workspaceFolder", skip_serializing_if = "Option::is_none")]
    pub workspace_folder: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Hooks settings from plugin configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksSettings {
    #[serde(flatten)]
    pub hooks: HashMap<String, serde_json::Value>,
}

/// User config schema type (Record<string, PluginUserConfigOption>).
pub type UserConfigSchema = HashMap<String, PluginUserConfigOption>;

/// Plugin installation record for autoupdate tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstallationRecord {
    pub plugin_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
}

/// Re-export PluginCacheSummary from cache_utils.
pub use super::cache_utils::PluginCacheSummary;

/// Git SHA validation (40 hex chars).
pub fn validate_git_sha(sha: &str) -> bool {
    static GIT_SHA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-f0-9]{40}$").unwrap());
    GIT_SHA_RE.is_match(sha)
}

/// Dependency ref regex.
static DEP_REF_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[a-z0-9][-a-z0-9._]*(@[a-z0-9][-a-z0-9._]*)?(@\^[^@]*)?$").unwrap()
});

/// Validate and normalize a dependency reference string.
/// Strips trailing @^version suffix for forwards-compatibility.
pub fn normalize_dependency_ref(dep: &str) -> Option<String> {
    if !DEP_REF_REGEX.is_match(dep) {
        return None;
    }
    // Strip trailing @^version
    static VERSION_SUFFIX: Lazy<Regex> = Lazy::new(|| Regex::new(r"@\^[^@]*$").unwrap());
    Some(VERSION_SUFFIX.replace(dep, "").to_string())
}

/// Validate a plugin manifest. Returns list of validation errors.
pub fn validate_plugin_manifest(manifest: &PluginManifest) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.name.is_empty() {
        errors.push("Plugin name cannot be empty".into());
    }
    if manifest.name.contains(' ') {
        errors
            .push("Plugin name cannot contain spaces. Use kebab-case (e.g., \"my-plugin\")".into());
    }
    errors
}

/// Validate a marketplace schema. Returns list of validation errors.
pub fn validate_marketplace(marketplace: &PluginMarketplace) -> Vec<String> {
    let mut errors = Vec::new();
    if let Err(e) = validate_marketplace_name(&marketplace.name) {
        errors.push(e);
    }
    if marketplace.owner.name.is_empty() {
        errors.push("Marketplace owner name cannot be empty".into());
    }
    errors
}

// =============================================================================
// Zod 风格 schema 标识符 — TS 中这些常量是 `z.object(...)` 描述符。Rust 端使用
// `serde`/`schemars` 在结构体上派生，因此把 schema 常量收敛为字符串标识符，便于
// 调用方按名字引用同一个序列化契约。
// =============================================================================

/// 对应 TS `DependencyRefSchema`。
pub const DEPENDENCY_REF_SCHEMA: &str = "DependencyRef";
/// 对应 TS `PluginHooksSchema`。
pub const PLUGIN_HOOKS_SCHEMA: &str = "PluginHooks";
/// 对应 TS `SettingsPluginEntrySchema`。
pub const SETTINGS_PLUGIN_ENTRY_SCHEMA: &str = "SettingsPluginEntry";
/// 对应 TS `InstalledPluginsFileSchemaV1`。
pub const INSTALLED_PLUGINS_FILE_SCHEMA_V1: &str = "InstalledPluginsFileV1";
/// 对应 TS `InstalledPluginsFileSchemaV2`。
pub const INSTALLED_PLUGINS_FILE_SCHEMA_V2: &str = "InstalledPluginsFileV2";
/// 对应 TS `InstalledPluginsFileSchema`（discriminated union）。
pub const INSTALLED_PLUGINS_FILE_SCHEMA: &str = "InstalledPluginsFile";
/// 对应 TS `gitSha` schema field name。
pub const GIT_SHA: &str = "gitSha";

// =============================================================================
// `XxxSchema` 别名 — 对应 TS Zod 导出。Rust 用结构体承载，别名指向同一类型。
// =============================================================================

/// Alias for the plugin author validator (mirrors TS `PluginAuthorSchema`).
pub type PluginAuthorSchema = PluginAuthor;
/// Alias for the command metadata validator (mirrors TS `CommandMetadataSchema`).
pub type CommandMetadataSchema = CommandMetadata;
/// Alias for the LSP server config validator (mirrors TS `LspServerConfigSchema`).
pub type LspServerConfigSchema = LspServerConfig;
/// Alias for the marketplace source validator (mirrors TS `MarketplaceSourceSchema`).
pub type MarketplaceSourceSchema = MarketplaceSource;
/// Alias for the known marketplace validator (mirrors TS `KnownMarketplaceSchema`).
pub type KnownMarketplaceSchema = KnownMarketplace;
/// Alias for the known marketplaces file validator (mirrors TS `KnownMarketplacesFileSchema`).
pub type KnownMarketplacesFileSchema = KnownMarketplacesFile;
/// Alias for the plugin scope validator (mirrors TS `PluginScopeSchema`).
pub type PluginScopeSchema = PluginScope;
/// Alias for the plugin source validator (mirrors TS `PluginSourceSchema`).
pub type PluginSourceSchema = PluginSource;
/// Alias for the plugin manifest validator (mirrors TS `PluginManifestSchema`).
pub type PluginManifestSchema = PluginManifest;
/// Alias for the plugin marketplace entry validator (mirrors TS `PluginMarketplaceEntrySchema`).
pub type PluginMarketplaceEntrySchema = PluginMarketplaceEntry;
/// Alias for the plugin marketplace validator (mirrors TS `PluginMarketplaceSchema`).
pub type PluginMarketplaceSchema = PluginMarketplace;
/// Alias for the installed plugin validator (mirrors TS `InstalledPluginSchema`).
pub type InstalledPluginSchema = InstalledPlugin;
/// Alias for the plugin installation entry validator (mirrors TS `PluginInstallationEntrySchema`).
pub type PluginInstallationEntrySchema = PluginInstallationEntry;
/// Alias for the plugin id validator (mirrors TS `PluginIdSchema`).
pub type PluginIdSchema = PluginId;
