//! # plugin — 插件系统类型
//!
//! 对应 TypeScript `types/plugin.ts`。
//! 定义 `PluginError`（25 个变体）、`LoadedPlugin` 等类型。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件组件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginComponent {
    Commands,
    Agents,
    Skills,
    Hooks,
    OutputStyles,
}

/// 插件清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 清单数据。
    #[serde(flatten)]
    pub data: HashMap<String, serde_json::Value>,
}

/// 插件作者。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    /// 名称。
    pub name: String,
    /// 额外字段。
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// 命令元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    /// 元数据。
    #[serde(flatten)]
    pub data: HashMap<String, serde_json::Value>,
}

/// 插件仓库。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepository {
    pub url: String,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

/// 插件配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub repositories: HashMap<String, PluginRepository>,
}

/// 内置插件定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinPluginDefinition {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_enabled: Option<bool>,
}

/// 已加载的插件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedPlugin {
    pub name: String,
    pub manifest: PluginManifest,
    pub path: String,
    pub source: String,
    pub repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_builtin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_metadata: Option<HashMap<String, CommandMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_styles_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_styles_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_servers: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<HashMap<String, serde_json::Value>>,
}

/// Git 认证类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitAuthType {
    Ssh,
    Https,
}

/// Git 操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitOperation {
    Clone,
    Pull,
}

/// 依赖不满足原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyUnsatisfiedReason {
    NotEnabled,
    NotFound,
}

/// 插件错误（25 个变体）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PluginError {
    PathNotFound {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        path: String,
        component: PluginComponent,
    },
    GitAuthFailed {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        git_url: String,
        auth_type: GitAuthType,
    },
    GitTimeout {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        git_url: String,
        operation: GitOperation,
    },
    NetworkError {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    ManifestParseError {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        manifest_path: String,
        parse_error: String,
    },
    ManifestValidationError {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        manifest_path: String,
        validation_errors: Vec<String>,
    },
    PluginNotFound {
        source: String,
        plugin_id: String,
        marketplace: String,
    },
    MarketplaceNotFound {
        source: String,
        marketplace: String,
        available_marketplaces: Vec<String>,
    },
    MarketplaceLoadFailed {
        source: String,
        marketplace: String,
        reason: String,
    },
    McpConfigInvalid {
        source: String,
        plugin: String,
        server_name: String,
        validation_error: String,
    },
    McpServerSuppressedDuplicate {
        source: String,
        plugin: String,
        server_name: String,
        duplicate_of: String,
    },
    LspConfigInvalid {
        source: String,
        plugin: String,
        server_name: String,
        validation_error: String,
    },
    HookLoadFailed {
        source: String,
        plugin: String,
        hook_path: String,
        reason: String,
    },
    ComponentLoadFailed {
        source: String,
        plugin: String,
        component: PluginComponent,
        path: String,
        reason: String,
    },
    McpbDownloadFailed {
        source: String,
        plugin: String,
        url: String,
        reason: String,
    },
    McpbExtractFailed {
        source: String,
        plugin: String,
        mcpb_path: String,
        reason: String,
    },
    McpbInvalidManifest {
        source: String,
        plugin: String,
        mcpb_path: String,
        validation_error: String,
    },
    LspServerStartFailed {
        source: String,
        plugin: String,
        server_name: String,
        reason: String,
    },
    LspServerCrashed {
        source: String,
        plugin: String,
        server_name: String,
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signal: Option<String>,
    },
    LspRequestTimeout {
        source: String,
        plugin: String,
        server_name: String,
        method: String,
        timeout_ms: u64,
    },
    LspRequestFailed {
        source: String,
        plugin: String,
        server_name: String,
        method: String,
        error: String,
    },
    MarketplaceBlockedByPolicy {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        marketplace: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_by_blocklist: Option<bool>,
        allowed_sources: Vec<String>,
    },
    DependencyUnsatisfied {
        source: String,
        plugin: String,
        dependency: String,
        reason: DependencyUnsatisfiedReason,
    },
    PluginCacheMiss {
        source: String,
        plugin: String,
        install_path: String,
    },
    GenericError {
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
        error: String,
    },
}

/// 插件加载结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoadResult {
    pub enabled: Vec<LoadedPlugin>,
    pub disabled: Vec<LoadedPlugin>,
    pub errors: Vec<PluginError>,
}

impl PluginError {
    /// 获取插件错误的显示消息。
    pub fn display_message(&self) -> String {
        match self {
            Self::GenericError { error, .. } => error.clone(),
            Self::PathNotFound {
                path, component, ..
            } => {
                format!("Path not found: {} ({:?})", path, component)
            }
            Self::GitAuthFailed {
                auth_type, git_url, ..
            } => {
                format!("Git authentication failed ({:?}): {}", auth_type, git_url)
            }
            Self::GitTimeout {
                operation, git_url, ..
            } => {
                format!("Git {:?} timeout: {}", operation, git_url)
            }
            Self::NetworkError { url, details, .. } => {
                if let Some(d) = details {
                    format!("Network error: {} - {}", url, d)
                } else {
                    format!("Network error: {}", url)
                }
            }
            Self::ManifestParseError { parse_error, .. } => {
                format!("Manifest parse error: {}", parse_error)
            }
            Self::ManifestValidationError {
                validation_errors, ..
            } => {
                format!(
                    "Manifest validation failed: {}",
                    validation_errors.join(", ")
                )
            }
            Self::PluginNotFound {
                plugin_id,
                marketplace,
                ..
            } => {
                format!(
                    "Plugin {} not found in marketplace {}",
                    plugin_id, marketplace
                )
            }
            Self::MarketplaceNotFound { marketplace, .. } => {
                format!("Marketplace {} not found", marketplace)
            }
            Self::MarketplaceLoadFailed {
                marketplace,
                reason,
                ..
            } => {
                format!("Marketplace {} failed to load: {}", marketplace, reason)
            }
            Self::McpConfigInvalid {
                server_name,
                validation_error,
                ..
            } => {
                format!("MCP server {} invalid: {}", server_name, validation_error)
            }
            Self::McpServerSuppressedDuplicate {
                server_name,
                duplicate_of,
                ..
            } => {
                format!(
                    "MCP server \"{}\" skipped — duplicate of {}",
                    server_name, duplicate_of
                )
            }
            Self::HookLoadFailed { reason, .. } => format!("Hook load failed: {}", reason),
            Self::ComponentLoadFailed {
                component,
                path,
                reason,
                ..
            } => {
                format!("{:?} load failed from {}: {}", component, path, reason)
            }
            Self::McpbDownloadFailed { url, reason, .. } => {
                format!("Failed to download MCPB from {}: {}", url, reason)
            }
            Self::McpbExtractFailed {
                mcpb_path, reason, ..
            } => {
                format!("Failed to extract MCPB {}: {}", mcpb_path, reason)
            }
            Self::McpbInvalidManifest {
                mcpb_path,
                validation_error,
                ..
            } => {
                format!(
                    "MCPB manifest invalid at {}: {}",
                    mcpb_path, validation_error
                )
            }
            Self::LspConfigInvalid {
                plugin,
                server_name,
                validation_error,
                ..
            } => {
                format!(
                    "Plugin \"{}\" has invalid LSP server config for \"{}\": {}",
                    plugin, server_name, validation_error
                )
            }
            Self::LspServerStartFailed {
                plugin,
                server_name,
                reason,
                ..
            } => {
                format!(
                    "Plugin \"{}\" failed to start LSP server \"{}\": {}",
                    plugin, server_name, reason
                )
            }
            Self::LspServerCrashed {
                plugin,
                server_name,
                exit_code,
                signal,
                ..
            } => {
                if let Some(sig) = signal {
                    format!(
                        "Plugin \"{}\" LSP server \"{}\" crashed with signal {}",
                        plugin, server_name, sig
                    )
                } else {
                    format!(
                        "Plugin \"{}\" LSP server \"{}\" crashed with exit code {}",
                        plugin,
                        server_name,
                        exit_code.map_or("unknown".to_string(), |c| c.to_string())
                    )
                }
            }
            Self::LspRequestTimeout {
                plugin,
                server_name,
                method,
                timeout_ms,
                ..
            } => {
                format!(
                    "Plugin \"{}\" LSP server \"{}\" timed out on {} request after {}ms",
                    plugin, server_name, method, timeout_ms
                )
            }
            Self::LspRequestFailed {
                plugin,
                server_name,
                method,
                error,
                ..
            } => {
                format!(
                    "Plugin \"{}\" LSP server \"{}\" {} request failed: {}",
                    plugin, server_name, method, error
                )
            }
            Self::MarketplaceBlockedByPolicy {
                marketplace,
                blocked_by_blocklist,
                ..
            } => {
                if blocked_by_blocklist == &Some(true) {
                    format!(
                        "Marketplace '{}' is blocked by enterprise policy",
                        marketplace
                    )
                } else {
                    format!(
                        "Marketplace '{}' is not in the allowed marketplace list",
                        marketplace
                    )
                }
            }
            Self::DependencyUnsatisfied {
                dependency, reason, ..
            } => {
                let hint = match reason {
                    DependencyUnsatisfiedReason::NotEnabled => {
                        "disabled — enable it or remove the dependency"
                    }
                    DependencyUnsatisfiedReason::NotFound => {
                        "not found in any configured marketplace"
                    }
                };
                format!("Dependency \"{}\" is {}", dependency, hint)
            }
            Self::PluginCacheMiss {
                plugin,
                install_path,
                ..
            } => {
                format!(
                    "Plugin \"{}\" not cached at {} — run /plugins to refresh",
                    plugin, install_path
                )
            }
        }
    }
}
