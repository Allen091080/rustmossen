use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::schemas::McpServerConfig;

/// Scoped MCP server config with plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedMcpServerConfig {
    #[serde(flatten)]
    pub config: McpServerConfig,
    pub scope: String,
    pub plugin_source: String,
}

/// MCP plugin error.
#[derive(Debug, Clone)]
pub struct McpPluginError {
    pub error_type: String,
    pub source: String,
    pub plugin: String,
    pub server_name: Option<String>,
    pub validation_error: Option<String>,
    pub url: Option<String>,
    pub reason: Option<String>,
    pub mcpb_path: Option<String>,
}

/// Unconfigured channel entry from a plugin's manifest.
#[derive(Debug, Clone)]
pub struct UnconfiguredChannel {
    pub server: String,
    pub display_name: String,
    pub config_schema: HashMap<String, serde_json::Value>,
}

/// Load MCP servers from a plugin's manifest.
pub async fn load_plugin_mcp_servers(
    plugin_path: &Path,
    plugin_name: &str,
    manifest_mcp_servers: Option<&McpServersSpec>,
    errors: &mut Vec<McpPluginError>,
) -> Option<HashMap<String, McpServerConfig>> {
    let mut servers: HashMap<String, McpServerConfig> = HashMap::new();

    // 1. Check for .mcp.json in plugin directory
    let mcp_json_path = plugin_path.join(".mcp.json");
    if let Ok(content) = tokio::fs::read_to_string(&mcp_json_path).await {
        match load_mcp_servers_from_content(&content) {
            Ok(parsed) => servers.extend(parsed),
            Err(e) => {
                debug!("Failed to load MCP servers from {:?}: {}", mcp_json_path, e);
            }
        }
    }

    // 2. Handle manifest mcpServers
    if let Some(spec) = manifest_mcp_servers {
        match spec {
            McpServersSpec::FilePath(path) => {
                if is_mcpb_source(path) {
                    if let Some(mcpb_servers) =
                        load_mcp_servers_from_mcpb(plugin_path, plugin_name, path, errors).await
                    {
                        servers.extend(mcpb_servers);
                    }
                } else {
                    let full_path = plugin_path.join(path);
                    if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                        match load_mcp_servers_from_content(&content) {
                            Ok(parsed) => servers.extend(parsed),
                            Err(e) => {
                                debug!("Failed to load MCP servers from {:?}: {}", full_path, e);
                            }
                        }
                    }
                }
            }
            McpServersSpec::Inline(configs) => {
                servers.extend(configs.clone());
            }
            McpServersSpec::Array(items) => {
                for item in items {
                    match item {
                        McpServersSpecItem::FilePath(path) => {
                            if is_mcpb_source(path) {
                                if let Some(mcpb_servers) = load_mcp_servers_from_mcpb(
                                    plugin_path,
                                    plugin_name,
                                    path,
                                    errors,
                                )
                                .await
                                {
                                    servers.extend(mcpb_servers);
                                }
                            } else {
                                let full_path = plugin_path.join(path);
                                if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
                                    if let Ok(parsed) = load_mcp_servers_from_content(&content) {
                                        servers.extend(parsed);
                                    }
                                }
                            }
                        }
                        McpServersSpecItem::Inline(configs) => {
                            servers.extend(configs.clone());
                        }
                    }
                }
            }
        }
    }

    if servers.is_empty() {
        None
    } else {
        Some(servers)
    }
}

/// MCP servers specification formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServersSpec {
    FilePath(String),
    Inline(HashMap<String, McpServerConfig>),
    Array(Vec<McpServersSpecItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServersSpecItem {
    FilePath(String),
    Inline(HashMap<String, McpServerConfig>),
}

fn is_mcpb_source(path: &str) -> bool {
    path.ends_with(".mcpb") || path.starts_with("http://") || path.starts_with("https://")
}

fn load_mcp_servers_from_content(
    content: &str,
) -> Result<HashMap<String, McpServerConfig>, anyhow::Error> {
    let parsed: serde_json::Value = serde_json::from_str(content)?;
    let mcp_servers = if parsed.get("mcpServers").is_some() {
        parsed.get("mcpServers").unwrap()
    } else {
        &parsed
    };
    let servers: HashMap<String, McpServerConfig> = serde_json::from_value(mcp_servers.clone())?;
    Ok(servers)
}

async fn load_mcp_servers_from_mcpb(
    _plugin_path: &Path,
    plugin_name: &str,
    mcpb_path: &str,
    _errors: &mut Vec<McpPluginError>,
) -> Option<HashMap<String, McpServerConfig>> {
    debug!("Loading MCP servers from MCPB: {}", mcpb_path);
    // MCPB loading would involve downloading/extracting DXT packages.
    // For now, report as needing configuration.
    debug!(
        "MCPB {} requires loading via mcpbHandler. Plugin: {}",
        mcpb_path, plugin_name
    );
    None
}

/// Add plugin scope to MCP server configs.
pub fn add_plugin_scope_to_servers(
    servers: &HashMap<String, McpServerConfig>,
    plugin_name: &str,
    plugin_source: &str,
) -> HashMap<String, ScopedMcpServerConfig> {
    let mut scoped = HashMap::new();
    for (name, config) in servers {
        let scoped_name = format!("plugin:{}:{}", plugin_name, name);
        scoped.insert(
            scoped_name,
            ScopedMcpServerConfig {
                config: config.clone(),
                scope: "dynamic".to_string(),
                plugin_source: plugin_source.to_string(),
            },
        );
    }
    scoped
}

/// Resolve environment variables for plugin MCP servers.
pub fn resolve_plugin_mcp_environment(
    config: &McpServerConfig,
    plugin_path: &str,
    plugin_source: &str,
    user_config: Option<&HashMap<String, serde_json::Value>>,
    get_plugin_data_dir: impl Fn(&str) -> String,
    expand_env_vars: impl Fn(&str) -> (String, Vec<String>),
) -> (McpServerConfig, Vec<String>) {
    let mut all_missing_vars: Vec<String> = Vec::new();

    let mut resolve_value = |value: &str| -> String {
        let mut resolved = super::plugin_options_storage::substitute_plugin_variables(
            value,
            plugin_path,
            Some(plugin_source),
            &get_plugin_data_dir,
        );
        if let Some(uc) = user_config {
            if let Ok(r) =
                super::plugin_options_storage::substitute_user_config_variables(&resolved, uc)
            {
                resolved = r;
            }
        }
        let (expanded, missing) = expand_env_vars(&resolved);
        all_missing_vars.extend(missing);
        expanded
    };

    let mut resolved = config.clone();

    // Handle stdio type
    if let Some(ref cmd) = resolved.command {
        resolved.command = Some(resolve_value(cmd));
    }
    if let Some(ref args) = resolved.args {
        resolved.args = Some(args.iter().map(|a| resolve_value(a)).collect());
    }

    // Add MOSSEN_PLUGIN_ROOT/DATA to env
    let mut resolved_env: HashMap<String, String> = HashMap::new();
    resolved_env.insert("MOSSEN_PLUGIN_ROOT".to_string(), plugin_path.to_string());
    resolved_env.insert(
        "MOSSEN_PLUGIN_DATA".to_string(),
        get_plugin_data_dir(plugin_source),
    );
    if let Some(ref env) = resolved.env {
        for (key, value) in env {
            if key != "MOSSEN_PLUGIN_ROOT" && key != "MOSSEN_PLUGIN_DATA" {
                resolved_env.insert(key.clone(), resolve_value(value));
            }
        }
    }
    resolved.env = Some(resolved_env);

    // Handle URL for remote types
    if let Some(ref url) = resolved.url {
        resolved.url = Some(resolve_value(url));
    }
    if let Some(ref headers) = resolved.headers {
        let resolved_headers: HashMap<String, String> = headers
            .iter()
            .map(|(k, v)| (k.clone(), resolve_value(v)))
            .collect();
        resolved.headers = Some(resolved_headers);
    }

    if !all_missing_vars.is_empty() {
        let unique: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            all_missing_vars
                .into_iter()
                .filter(|v| seen.insert(v.clone()))
                .collect()
        };
        debug!(
            "Missing environment variables in plugin MCP config: {}",
            unique.join(", ")
        );
        return (resolved, unique);
    }

    (resolved, Vec::new())
}

/// Extract all MCP servers from loaded plugins.
pub async fn extract_mcp_servers_from_plugins(
    plugins: &[(PathBuf, String, String, bool, Option<McpServersSpec>)],
    errors: &mut Vec<McpPluginError>,
) -> HashMap<String, ScopedMcpServerConfig> {
    let mut all_servers: HashMap<String, ScopedMcpServerConfig> = HashMap::new();

    for (path, name, source, enabled, manifest_mcp) in plugins {
        if !*enabled {
            continue;
        }

        let servers = load_plugin_mcp_servers(path, name, manifest_mcp.as_ref(), errors).await;
        if let Some(servers) = servers {
            let scoped = add_plugin_scope_to_servers(&servers, name, source);
            all_servers.extend(scoped);
            debug!("Loaded {} MCP servers from plugin {}", servers.len(), name);
        }
    }

    all_servers
}

/// Get unconfigured channels from a plugin.
pub fn get_unconfigured_channels(
    channels: &[ChannelDef],
    plugin_id: &str,
    load_server_config: impl Fn(&str, &str) -> Option<HashMap<String, serde_json::Value>>,
    validate_config: impl Fn(
        &HashMap<String, serde_json::Value>,
        &HashMap<String, serde_json::Value>,
    ) -> bool,
) -> Vec<UnconfiguredChannel> {
    let mut unconfigured = Vec::new();
    for channel in channels {
        if channel.user_config.is_empty() {
            continue;
        }
        let saved = load_server_config(plugin_id, &channel.server).unwrap_or_default();
        if !validate_config(&saved, &channel.user_config) {
            unconfigured.push(UnconfiguredChannel {
                server: channel.server.clone(),
                display_name: channel
                    .display_name
                    .clone()
                    .unwrap_or_else(|| channel.server.clone()),
                config_schema: channel.user_config.clone(),
            });
        }
    }
    unconfigured
}

#[derive(Debug, Clone)]
pub struct ChannelDef {
    pub server: String,
    pub display_name: Option<String>,
    pub user_config: HashMap<String, serde_json::Value>,
}

/// Get MCP servers from a specific plugin with environment variable resolution and scoping.
pub async fn get_plugin_mcp_servers(
    plugin_path: &Path,
    plugin_name: &str,
    plugin_source: &str,
    enabled: bool,
    cached_servers: Option<&HashMap<String, McpServerConfig>>,
    manifest_mcp_servers: Option<&McpServersSpec>,
    user_config: Option<&HashMap<String, serde_json::Value>>,
    errors: &mut Vec<McpPluginError>,
    get_plugin_data_dir: impl Fn(&str) -> String + Clone,
    expand_env_vars: impl Fn(&str) -> (String, Vec<String>) + Clone,
) -> Option<HashMap<String, ScopedMcpServerConfig>> {
    if !enabled {
        return None;
    }

    let servers = if let Some(cached) = cached_servers {
        cached.clone()
    } else {
        load_plugin_mcp_servers(plugin_path, plugin_name, manifest_mcp_servers, errors).await?
    };

    let mut resolved_servers: HashMap<String, McpServerConfig> = HashMap::new();
    for (name, config) in &servers {
        let (resolved, _) = resolve_plugin_mcp_environment(
            config,
            &plugin_path.to_string_lossy(),
            plugin_source,
            user_config,
            get_plugin_data_dir.clone(),
            expand_env_vars.clone(),
        );
        resolved_servers.insert(name.clone(), resolved);
    }

    Some(add_plugin_scope_to_servers(
        &resolved_servers,
        plugin_name,
        plugin_source,
    ))
}
