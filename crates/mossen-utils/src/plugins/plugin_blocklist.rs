use std::collections::HashSet;
use tracing::debug;

use super::schemas::{InstalledPluginsFileV2, PluginMarketplace, PluginScope};

/// Detect plugins installed from a marketplace that are no longer listed there.
///
/// Returns list of delisted plugin IDs in "name@marketplace" format.
pub fn detect_delisted_plugins(
    installed_plugins: &InstalledPluginsFileV2,
    marketplace: &PluginMarketplace,
    marketplace_name: &str,
) -> Vec<String> {
    let marketplace_plugin_names: HashSet<&str> = marketplace
        .plugins
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    let suffix = format!("@{}", marketplace_name);

    let mut delisted = Vec::new();
    for plugin_id in installed_plugins.plugins.keys() {
        if !plugin_id.ends_with(&suffix) {
            continue;
        }
        let plugin_name = &plugin_id[..plugin_id.len() - suffix.len()];
        if !marketplace_plugin_names.contains(plugin_name) {
            delisted.push(plugin_id.clone());
        }
    }

    delisted
}

/// Detect delisted plugins across all marketplaces, auto-uninstall them,
/// and record them as flagged.
///
/// Returns list of newly flagged plugin IDs.
pub async fn detect_and_uninstall_delisted_plugins(
    load_flagged_plugins: impl std::future::Future<Output = ()>,
    load_installed_plugins_v2: impl Fn() -> InstalledPluginsFileV2,
    get_flagged_plugins: impl Fn() -> std::collections::HashMap<String, super::plugin_flagging::FlaggedPlugin>,
    load_known_marketplaces_safe: impl std::future::Future<Output = std::collections::HashMap<String, super::source_status::KnownMarketplaceInfo>>,
    get_marketplace: impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<PluginMarketplace, anyhow::Error>> + Send>>,
    uninstall_plugin_op: impl Fn(&str, &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send>>,
    add_flagged_plugin: impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
) -> Vec<String> {
    load_flagged_plugins.await;

    let installed_plugins = load_installed_plugins_v2();
    let already_flagged = get_flagged_plugins();
    let known_marketplaces = load_known_marketplaces_safe.await;
    let mut newly_flagged: Vec<String> = Vec::new();

    for marketplace_name in known_marketplaces.keys() {
        let marketplace = match get_marketplace(marketplace_name).await {
            Ok(m) => m,
            Err(error) => {
                debug!(
                    "Failed to check for delisted plugins in \"{}\": {}",
                    marketplace_name, error
                );
                continue;
            }
        };

        if !marketplace.force_remove_deleted_plugins.unwrap_or(false) {
            continue;
        }

        let delisted = detect_delisted_plugins(&installed_plugins, &marketplace, marketplace_name);

        for plugin_id in delisted {
            if already_flagged.contains_key(&plugin_id) {
                continue;
            }

            // Skip managed-only plugins
            let installations = installed_plugins
                .plugins
                .get(&plugin_id)
                .cloned()
                .unwrap_or_default();
            let has_user_install = installations.iter().any(|i| {
                i.scope == PluginScope::User
                    || i.scope == PluginScope::Project
                    || i.scope == PluginScope::Local
            });
            if !has_user_install {
                continue;
            }

            // Auto-uninstall the delisted plugin from all user-controllable scopes
            for installation in &installations {
                let scope = &installation.scope;
                if *scope != PluginScope::User
                    && *scope != PluginScope::Project
                    && *scope != PluginScope::Local
                {
                    continue;
                }
                let scope_str = match scope {
                    PluginScope::User => "user",
                    PluginScope::Project => "project",
                    PluginScope::Local => "local",
                    _ => continue,
                };
                if let Err(error) = uninstall_plugin_op(&plugin_id, scope_str).await {
                    debug!(
                        "Failed to auto-uninstall delisted plugin {} from {}: {}",
                        plugin_id, scope_str, error
                    );
                }
            }

            add_flagged_plugin(&plugin_id).await;
            newly_flagged.push(plugin_id);
        }
    }

    newly_flagged
}
