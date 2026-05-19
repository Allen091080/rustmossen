//! Background plugin and marketplace installation manager

use tracing::{debug, error, info, warn};

/// Marketplace installation status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketplaceInstallStatus {
    Pending,
    Installing,
    Installed,
    Failed(String),
}

/// Progress event during marketplace reconciliation
#[derive(Debug, Clone)]
pub enum ReconcileProgressEvent {
    Installing { name: String },
    Installed { name: String },
    Failed { name: String, error: String },
}

/// Result of marketplace reconciliation
#[derive(Debug, Clone)]
pub struct ReconcileResult {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub failed: Vec<String>,
    pub up_to_date: Vec<String>,
}

/// Callback for state updates during plugin installation
pub trait PluginInstallStateCallback: Send + Sync {
    fn update_marketplace_status(&self, name: &str, status: MarketplaceInstallStatus);
    fn set_needs_refresh(&self, needs_refresh: bool);
}

/// Perform background plugin startup checks and installations.
///
/// This is a thin wrapper around marketplace reconciliation that maps
/// progress events to state updates for the UI. After marketplaces are
/// reconciled:
/// - New installs -> auto-refresh plugins
/// - Updates only -> set needsRefresh, show notification for /reload-plugins
pub async fn perform_background_plugin_installations(
    callback: &dyn PluginInstallStateCallback,
    get_declared_marketplaces: impl Fn() -> Vec<String>,
    reconcile_marketplaces: impl std::future::Future<Output = Result<ReconcileResult, String>>,
) {
    debug!("performBackgroundPluginInstallations called");

    let declared = get_declared_marketplaces();
    if declared.is_empty() {
        return;
    }

    // Initialize with pending status
    for name in &declared {
        callback.update_marketplace_status(name, MarketplaceInstallStatus::Pending);
    }

    info!("Installing {} marketplace(s) in background", declared.len());

    let result = match reconcile_marketplaces.await {
        Ok(r) => r,
        Err(e) => {
            error!("Marketplace reconciliation failed: {}", e);
            return;
        }
    };

    info!(
        "Marketplace reconciliation complete: {} installed, {} updated, {} failed, {} up-to-date",
        result.installed.len(),
        result.updated.len(),
        result.failed.len(),
        result.up_to_date.len()
    );

    if !result.installed.is_empty() {
        // New marketplaces were installed - auto-refresh plugins
        debug!(
            "Auto-refreshing plugins after {} new marketplace(s) installed",
            result.installed.len()
        );
        // In full implementation: clear caches and reload plugins
        // On failure: fall back to needsRefresh notification
        callback.set_needs_refresh(true);
    } else if !result.updated.is_empty() {
        // Existing marketplaces updated - notify user to run /reload-plugins
        callback.set_needs_refresh(true);
    }
}
