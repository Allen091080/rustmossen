//! Headless plugin install — plugin installation for headless/CCR mode.
use super::zip_cache::{
    get_zip_cache_marketplaces_dir, get_zip_cache_plugins_dir, is_plugin_zip_cache_enabled,
};
use tracing::debug;

/// Install plugins for headless/CCR mode.
/// Returns true if any plugins were installed (caller should refresh MCP).
pub async fn install_plugins_for_headless(
    register_seed_marketplaces: impl std::future::Future<Output = bool>,
    clear_marketplaces_cache: impl Fn(),
    clear_plugin_cache: impl Fn(&str),
    get_declared_marketplaces_count: impl Fn() -> usize,
    reconcile_marketplaces: impl std::future::Future<Output = ReconcileResult>,
    sync_marketplaces_to_zip_cache: impl std::future::Future<Output = ()>,
    detect_and_uninstall_delisted: impl std::future::Future<Output = Vec<String>>,
    _cleanup_session_plugin_cache: impl Fn(Box<dyn FnOnce()>),
    mkdir: impl Fn(
        &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), std::io::Error>> + Send>,
    >,
    _log_event: impl Fn(&str, &std::collections::HashMap<String, serde_json::Value>),
) -> bool {
    let zip_cache_mode = is_plugin_zip_cache_enabled();
    debug!(
        "installPluginsForHeadless: starting{}",
        if zip_cache_mode {
            " (zip cache mode)"
        } else {
            ""
        }
    );

    let seed_changed = register_seed_marketplaces.await;
    if seed_changed {
        clear_marketplaces_cache();
        clear_plugin_cache("headlessPluginInstall: seed marketplaces registered");
    }

    if zip_cache_mode {
        let _ = mkdir(&get_zip_cache_marketplaces_dir()).await;
        let _ = mkdir(&get_zip_cache_plugins_dir()).await;
    }

    let declared_count = get_declared_marketplaces_count();
    let mut plugins_changed = seed_changed;

    let result: Result<bool, anyhow::Error> = async {
        if declared_count == 0 {
            debug!("installPluginsForHeadless: no marketplaces declared");
        } else {
            let reconcile_result = reconcile_marketplaces.await;
            let marketplaces_changed =
                reconcile_result.installed.len() + reconcile_result.updated.len();

            if !reconcile_result.skipped.is_empty() {
                debug!(
                    "installPluginsForHeadless: skipped {} marketplace(s) unsupported by zip cache",
                    reconcile_result.skipped.len()
                );
            }

            if marketplaces_changed > 0 {
                clear_marketplaces_cache();
                clear_plugin_cache("headlessPluginInstall: marketplaces reconciled");
                plugins_changed = true;
            }
        }

        if zip_cache_mode {
            sync_marketplaces_to_zip_cache.await;
        }

        let newly_delisted = detect_and_uninstall_delisted.await;
        if !newly_delisted.is_empty() {
            plugins_changed = true;
        }

        if plugins_changed {
            clear_plugin_cache("headlessPluginInstall: plugins changed");
        }

        Ok(plugins_changed)
    }
    .await;

    result.unwrap_or(false)
}

#[derive(Debug, Clone, Default)]
pub struct ReconcileResult {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub failed: Vec<ReconcileFailure>,
    pub skipped: Vec<String>,
    pub up_to_date: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReconcileFailure {
    pub name: String,
    pub error: String,
}
