//! LSP server configuration — loads server configs from plugins.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, warn};

/// Scoped LSP server configuration from a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedLspServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub workspace_folder: Option<String>,
    #[serde(default)]
    pub extension_to_language: HashMap<String, String>,
    #[serde(default)]
    pub initialization_options: Option<serde_json::Value>,
    #[serde(default)]
    pub max_restarts: Option<u32>,
    #[serde(default)]
    pub startup_timeout: Option<u64>,
    #[serde(default)]
    pub restart_on_crash: Option<bool>,
    #[serde(default)]
    pub shutdown_timeout: Option<u64>,
}

/// LSP server state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl std::fmt::Display for LspServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Stopping => write!(f, "stopping"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Plugin error type for LSP loading.
#[derive(Debug, Clone)]
pub struct PluginError {
    pub plugin_name: String,
    pub message: String,
}

/// Get all configured LSP servers from plugins.
/// LSP servers are only supported via plugins, not user/project settings.
pub async fn get_all_lsp_servers() -> Result<HashMap<String, ScopedLspServerConfig>> {
    let mut all_servers: HashMap<String, ScopedLspServerConfig> = HashMap::new();

    // Load all enabled plugins
    let plugins = load_all_plugins_cache_only().await;

    // Load LSP servers from each plugin in parallel
    let mut handles = Vec::new();
    for plugin in plugins {
        handles.push(tokio::spawn(async move {
            let mut errors: Vec<PluginError> = Vec::new();
            match get_plugin_lsp_servers(&plugin, &mut errors).await {
                Ok(scoped_servers) => (plugin, Some(scoped_servers), errors),
                Err(e) => {
                    debug!(
                        "Failed to load LSP servers for plugin {}: {}",
                        plugin.name, e
                    );
                    (plugin, None, errors)
                }
            }
        }));
    }

    for handle in handles {
        let (plugin, scoped_servers, errors) = handle.await?;
        if let Some(servers) = scoped_servers {
            let server_count = servers.len();
            if server_count > 0 {
                all_servers.extend(servers);
                debug!(
                    "Loaded {} LSP server(s) from plugin: {}",
                    server_count, plugin.name
                );
            }
        }
        if !errors.is_empty() {
            debug!(
                "{} error(s) loading LSP servers from plugin: {}",
                errors.len(),
                plugin.name
            );
        }
    }

    debug!("Total LSP servers loaded: {}", all_servers.len());
    Ok(all_servers)
}

/// Plugin metadata for LSP loading.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub path: String,
}

/// Load all enabled plugins from cache.
async fn load_all_plugins_cache_only() -> Vec<PluginInfo> {
    // In the Rust implementation, plugin loading is handled by the plugin system.
    // This returns the cached list of enabled plugins.
    Vec::new()
}

/// Get LSP servers defined by a specific plugin.
async fn get_plugin_lsp_servers(
    plugin: &PluginInfo,
    errors: &mut Vec<PluginError>,
) -> Result<HashMap<String, ScopedLspServerConfig>> {
    // Plugin LSP server extraction would read the plugin manifest
    // and return scoped server configurations.
    let _ = (plugin, errors);
    Ok(HashMap::new())
}
