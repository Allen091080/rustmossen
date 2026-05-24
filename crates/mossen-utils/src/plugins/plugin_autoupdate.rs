//! Plugin autoupdate — background marketplace and plugin updates.

use std::collections::HashSet;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use tracing::debug;

use super::plugin_identifier::parse_plugin_identifier;
use super::schemas::PluginScope;

/// Callback type for notifying when plugins have been updated.
pub type PluginAutoUpdateCallback = Box<dyn Fn(&[String]) + Send + Sync>;

static PLUGIN_UPDATE_CALLBACK: Lazy<Mutex<Option<PluginAutoUpdateCallback>>> =
    Lazy::new(|| Mutex::new(None));
static PENDING_NOTIFICATION: Lazy<Mutex<Option<Vec<String>>>> = Lazy::new(|| Mutex::new(None));

/// Register a callback to be notified when plugins are auto-updated.
pub fn on_plugins_auto_updated(callback: PluginAutoUpdateCallback) -> impl FnOnce() {
    {
        let mut guard = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
        *guard = Some(callback);
    }

    // Deliver pending notifications if any
    let pending = PENDING_NOTIFICATION.lock().unwrap().take();
    if let Some(ref updates) = pending {
        if !updates.is_empty() {
            let guard = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
            if let Some(ref cb) = *guard {
                cb(updates);
            }
        }
    }

    || {
        let mut guard = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
        *guard = None;
    }
}

/// Check if pending updates came from autoupdate.
pub fn get_auto_updated_plugin_names(
    has_pending_updates: impl Fn() -> bool,
    get_pending_updates_details: impl Fn() -> Vec<PendingUpdateDetail>,
) -> Vec<String> {
    if !has_pending_updates() {
        return vec![];
    }
    get_pending_updates_details()
        .iter()
        .map(|d| parse_plugin_identifier(&d.plugin_id).name)
        .collect()
}

#[derive(Debug, Clone)]
pub struct PendingUpdateDetail {
    pub plugin_id: String,
}

/// Get the set of marketplaces that have autoUpdate enabled.
pub async fn get_auto_update_enabled_marketplaces(
    load_known_config: impl std::future::Future<
        Output = Result<
            std::collections::HashMap<String, super::source_status::KnownMarketplaceInfo>,
            anyhow::Error,
        >,
    >,
    get_declared: impl Fn() -> std::collections::HashMap<String, super::reconciler::DeclaredMarketplace>,
    is_marketplace_auto_update: impl Fn(&str, &super::source_status::KnownMarketplaceInfo) -> bool,
) -> HashSet<String> {
    let config = match load_known_config.await {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    let declared = get_declared();
    let mut enabled = HashSet::new();

    for (name, entry) in &config {
        let declared_auto = declared.get(name).and_then(|d| d.auto_update);
        let auto_update = match declared_auto {
            Some(v) => v,
            None => is_marketplace_auto_update(name, entry),
        };
        if auto_update {
            enabled.insert(name.to_lowercase());
        }
    }

    enabled
}

/// Update a single plugin's installations.
pub async fn update_plugin(
    plugin_id: &str,
    installations: &[PluginInstallation],
    update_plugin_op: impl Fn(
        &str,
        &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<UpdateResult, anyhow::Error>> + Send>,
    >,
) -> Option<String> {
    let mut was_updated = false;

    for installation in installations {
        let scope_str = match installation.scope {
            PluginScope::User => "user",
            PluginScope::Project => "project",
            PluginScope::Local => "local",
            PluginScope::Managed => "managed",
        };
        match update_plugin_op(plugin_id, scope_str).await {
            Ok(result) => {
                if result.success && !result.already_up_to_date {
                    was_updated = true;
                    debug!(
                        "Plugin autoupdate: updated {} from {} to {}",
                        plugin_id,
                        result.old_version.as_deref().unwrap_or("?"),
                        result.new_version.as_deref().unwrap_or("?")
                    );
                } else if !result.already_up_to_date {
                    debug!(
                        "Plugin autoupdate: failed to update {}: {}",
                        plugin_id,
                        result.message.as_deref().unwrap_or("")
                    );
                }
            }
            Err(error) => {
                debug!("Plugin autoupdate: error updating {}: {}", plugin_id, error);
            }
        }
    }

    if was_updated {
        Some(plugin_id.to_string())
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct PluginInstallation {
    pub scope: PluginScope,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub success: bool,
    pub already_up_to_date: bool,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub message: Option<String>,
}

/// Update plugins from marketplaces that have autoUpdate enabled.
pub async fn update_plugins_for_marketplaces(
    marketplace_names: &HashSet<String>,
    load_installed_plugins: impl Fn() -> super::schemas::InstalledPluginsFileV2,
    is_installation_relevant: impl Fn(&super::schemas::PluginInstallationEntry) -> bool,
    update_plugin_op: impl Fn(
        &str,
        &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<UpdateResult, anyhow::Error>> + Send>,
    >,
) -> Vec<String> {
    let installed_plugins = load_installed_plugins();
    let plugin_ids: Vec<String> = installed_plugins.plugins.keys().cloned().collect();

    if plugin_ids.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();
    for plugin_id in &plugin_ids {
        let parsed = parse_plugin_identifier(plugin_id);
        let marketplace = match parsed.marketplace {
            Some(m) => m,
            None => continue,
        };
        if !marketplace_names.contains(&marketplace.to_lowercase()) {
            continue;
        }

        let all_installations = match installed_plugins.plugins.get(plugin_id) {
            Some(i) if !i.is_empty() => i,
            _ => continue,
        };

        let relevant: Vec<PluginInstallation> = all_installations
            .iter()
            .filter(|i| is_installation_relevant(*i))
            .map(|i| PluginInstallation {
                scope: i.scope.clone(),
                project_path: i.project_path.clone(),
            })
            .collect();

        if relevant.is_empty() {
            continue;
        }

        if let Some(id) = update_plugin(plugin_id, &relevant, &update_plugin_op).await {
            results.push(id);
        }
    }

    results
}

/// Auto-update marketplaces and plugins in the background.
pub fn auto_update_marketplaces_and_plugins_in_background(
    should_skip: bool,
    get_auto_update_enabled: impl std::future::Future<Output = HashSet<String>> + Send + 'static,
    refresh_marketplace: impl Fn(
            &str,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + 'static,
    update_plugins_fn: impl std::future::Future<Output = Vec<String>> + Send + 'static,
) {
    if should_skip {
        debug!("Plugin autoupdate: skipped (auto-updater disabled)");
        return;
    }

    tokio::spawn(async move {
        let auto_update_marketplaces = get_auto_update_enabled.await;
        if auto_update_marketplaces.is_empty() {
            return;
        }

        // Refresh marketplaces
        for name in &auto_update_marketplaces {
            if let Err(e) = refresh_marketplace(name).await {
                debug!(
                    "Plugin autoupdate: failed to refresh marketplace {}: {}",
                    name, e
                );
            }
        }

        debug!("Plugin autoupdate: checking installed plugins");
        let updated_plugins = update_plugins_fn.await;

        if !updated_plugins.is_empty() {
            let guard = PLUGIN_UPDATE_CALLBACK.lock().unwrap();
            if let Some(ref cb) = *guard {
                cb(&updated_plugins);
            } else {
                drop(guard);
                let mut pending = PENDING_NOTIFICATION.lock().unwrap();
                *pending = Some(updated_plugins);
            }
        }
    });
}
