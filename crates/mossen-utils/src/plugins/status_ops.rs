use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::debug;

use super::schemas::PluginCacheSummary;

/// Read-only /plugin status summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusSummary {
    /// ~/.mossen/plugins/ root
    pub plugin_root_path: String,
    /// Whether ~/.mossen/plugins/ exists on disk
    pub plugin_root_exists: bool,
    /// Cache summary from cacheUtils.summarizePluginCache
    pub cache: PluginCacheSummary,
    /// ~/.mossen/plugins/marketplaces/
    pub marketplaces_dir: String,
    /// Whether marketplaces dir exists
    pub marketplaces_dir_exists: bool,
    /// Number of plugin records loaded from installed_plugins.json
    pub installed_record_count: usize,
    /// Sum across all plugins of installed-version count
    pub installed_version_count: usize,
    /// True iff loading installed_plugins.json succeeded
    pub installed_registry_loadable: bool,
    /// Path to installed_plugins.json
    pub installed_registry_path: String,
    /// True iff there is at least one cached version not in registry
    pub prune_eligible: bool,
    /// Suggested next command for the user
    pub suggested_command: String,
}

async fn dir_exists(path: &str) -> bool {
    match fs::metadata(path).await {
        Ok(meta) => meta.is_dir(),
        Err(_) => false,
    }
}

/// Read-only summary for /plugin status. Never modifies disk state.
pub async fn describe_plugin_status(
    config_home: &str,
    summarize_plugin_cache: impl std::future::Future<Output = PluginCacheSummary>,
    get_marketplaces_cache_dir: impl Fn() -> String,
    load_installed_plugins: impl Fn() -> Result<InstalledPluginsData, anyhow::Error>,
) -> PluginStatusSummary {
    let plugin_root_path = format!("{}/plugins", config_home);
    let plugin_root_exists = dir_exists(&plugin_root_path).await;

    let cache = summarize_plugin_cache.await;
    let marketplaces_dir = get_marketplaces_cache_dir();
    let marketplaces_dir_exists = dir_exists(&marketplaces_dir).await;

    let mut installed_record_count = 0usize;
    let mut installed_version_count = 0usize;
    let mut installed_registry_loadable = true;
    let installed_registry_path = format!("{}/installed_plugins.json", plugin_root_path);

    match load_installed_plugins() {
        Ok(data) => {
            installed_record_count = data.plugins.len();
            for installations in data.plugins.values() {
                installed_version_count += installations.len();
            }
        }
        Err(error) => {
            debug!("statusOps: failed to load installed_plugins: {}", error);
            installed_registry_loadable = false;
        }
    }

    let prune_eligible = !cache.zip_cache_mode
        && (cache.expired_orphan_count > 0 || cache.unmarked_orphan_count > 0);
    let suggested_command = if cache.zip_cache_mode {
        "(zip-cache mode active — /plugin prune does not apply)".to_string()
    } else if prune_eligible {
        "/plugin prune".to_string()
    } else {
        "(no orphans — /plugin prune would no-op)".to_string()
    };

    PluginStatusSummary {
        plugin_root_path,
        plugin_root_exists,
        cache,
        marketplaces_dir,
        marketplaces_dir_exists,
        installed_record_count,
        installed_version_count,
        installed_registry_loadable,
        installed_registry_path,
        prune_eligible,
        suggested_command,
    }
}

/// Minimal installed plugins data for status display.
#[derive(Debug, Clone)]
pub struct InstalledPluginsData {
    pub plugins: std::collections::HashMap<String, Vec<InstalledPluginEntry>>,
}

#[derive(Debug, Clone)]
pub struct InstalledPluginEntry {
    pub scope: String,
    pub project_path: Option<String>,
}
