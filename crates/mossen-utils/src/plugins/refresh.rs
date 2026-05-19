use std::collections::HashSet;

use tracing::debug;

/// Result of refreshing all active plugin components.
#[derive(Debug, Clone)]
pub struct RefreshActivePluginsResult {
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub command_count: usize,
    pub agent_count: usize,
    pub hook_count: usize,
    pub mcp_count: usize,
    pub lsp_count: usize,
    pub error_count: usize,
}

/// Trait representing the plugin error type.
#[derive(Debug, Clone)]
pub struct PluginError {
    pub error_type: String,
    pub source: String,
    pub message: String,
}

/// Trait for set-app-state operations.
pub trait AppStateUpdater: Send + Sync {
    fn update_plugins(
        &self,
        enabled_count: usize,
        disabled_count: usize,
        commands_count: usize,
        errors: Vec<PluginError>,
    );
    fn increment_mcp_reconnect_key(&self);
}

/// Trait for plugin loading operations.
#[async_trait::async_trait]
pub trait PluginLoader: Send + Sync {
    async fn load_all_plugins(&self) -> PluginLoadResult;
    async fn get_plugin_commands(&self) -> Vec<PluginCommand>;
    async fn load_plugin_hooks(&self) -> Result<(), anyhow::Error>;
    async fn load_plugin_mcp_servers(
        &self,
        plugin_name: &str,
        errors: &mut Vec<PluginError>,
    ) -> usize;
    async fn load_plugin_lsp_servers(
        &self,
        plugin_name: &str,
        errors: &mut Vec<PluginError>,
    ) -> usize;
}

/// Trait for cache clearing operations.
pub trait CacheClearer: Send + Sync {
    fn clear_all_caches(&self);
    fn clear_plugin_cache_exclusions(&self);
    fn reinitialize_lsp_server_manager(&self);
}

#[derive(Debug, Clone)]
pub struct PluginLoadResult {
    pub enabled: Vec<LoadedPluginInfo>,
    pub disabled: Vec<LoadedPluginInfo>,
    pub errors: Vec<PluginError>,
}

#[derive(Debug, Clone)]
pub struct LoadedPluginInfo {
    pub name: String,
    pub source: String,
    pub has_mcp_servers: bool,
    pub has_lsp_servers: bool,
    pub hooks_count: usize,
}

#[derive(Debug, Clone)]
pub struct PluginCommand {
    pub name: String,
}

/// Refresh all active plugin components: commands, agents, hooks, MCP-reconnect
/// trigger, AppState plugin arrays. Clears ALL plugin caches.
///
/// Consumes plugins.needsRefresh (sets to false).
/// Increments mcp.pluginReconnectKey so useManageMCPConnections effects re-run
/// and pick up new plugin MCP servers.
pub async fn refresh_active_plugins(
    state_updater: &dyn AppStateUpdater,
    plugin_loader: &dyn PluginLoader,
    cache_clearer: &dyn CacheClearer,
) -> RefreshActivePluginsResult {
    debug!("refreshActivePlugins: clearing all plugin caches");
    cache_clearer.clear_all_caches();
    cache_clearer.clear_plugin_cache_exclusions();

    let plugin_result = plugin_loader.load_all_plugins().await;
    let plugin_commands = plugin_loader.get_plugin_commands().await;

    let mut errors = plugin_result.errors.clone();
    let enabled = &plugin_result.enabled;
    let disabled = &plugin_result.disabled;

    // Load MCP and LSP servers for each enabled plugin
    let mut mcp_count = 0usize;
    let mut lsp_count = 0usize;
    for plugin in enabled.iter() {
        if !plugin.has_mcp_servers {
            let count = plugin_loader
                .load_plugin_mcp_servers(&plugin.name, &mut errors)
                .await;
            mcp_count += count;
        }
        if !plugin.has_lsp_servers {
            let count = plugin_loader
                .load_plugin_lsp_servers(&plugin.name, &mut errors)
                .await;
            lsp_count += count;
        }
    }

    state_updater.update_plugins(
        enabled.len(),
        disabled.len(),
        plugin_commands.len(),
        errors.clone(),
    );
    state_updater.increment_mcp_reconnect_key();

    // Re-initialize LSP manager
    cache_clearer.reinitialize_lsp_server_manager();

    // Load plugin hooks
    let mut hook_load_failed = false;
    if let Err(e) = plugin_loader.load_plugin_hooks().await {
        hook_load_failed = true;
        debug!("refreshActivePlugins: loadPluginHooks failed: {}", e);
    }

    // Count hooks
    let hook_count: usize = enabled.iter().map(|p| p.hooks_count).sum();

    debug!(
        "refreshActivePlugins: {} enabled, {} commands, {} hooks, {} MCP, {} LSP",
        enabled.len(),
        plugin_commands.len(),
        hook_count,
        mcp_count,
        lsp_count
    );

    RefreshActivePluginsResult {
        enabled_count: enabled.len(),
        disabled_count: disabled.len(),
        command_count: plugin_commands.len(),
        agent_count: 0, // populated by caller
        hook_count,
        mcp_count,
        lsp_count,
        error_count: errors.len() + if hook_load_failed { 1 } else { 0 },
    }
}

/// Merge fresh plugin-load errors with existing errors, preserving LSP and
/// plugin-component errors that were recorded by other systems and deduplicating.
pub fn merge_plugin_errors(
    existing: &[PluginError],
    fresh: &[PluginError],
) -> Vec<PluginError> {
    let preserved: Vec<&PluginError> = existing
        .iter()
        .filter(|e| e.source == "lsp-manager" || e.source.starts_with("plugin:"))
        .collect();

    let fresh_keys: HashSet<String> = fresh.iter().map(|e| error_key(e)).collect();
    let deduped: Vec<PluginError> = preserved
        .into_iter()
        .filter(|e| !fresh_keys.contains(&error_key(e)))
        .cloned()
        .collect();

    let mut result = deduped;
    result.extend(fresh.iter().cloned());
    result
}

fn error_key(e: &PluginError) -> String {
    if e.error_type == "generic-error" {
        format!("generic-error:{}:{}", e.source, e.message)
    } else {
        format!("{}:{}", e.error_type, e.source)
    }
}
