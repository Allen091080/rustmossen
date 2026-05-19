//! Plugin startup check — checks for enabled plugins and installs missing ones.
use std::collections::HashMap;
use tracing::debug;
use super::plugin_identifier::{ExtendedPluginScope};
use super::schemas::PluginScope;

/// Checks for enabled plugins across all settings sources.
pub fn check_enabled_plugins(
    get_initial_settings: impl Fn() -> MergedSettings,
    get_add_dir_enabled_plugins: impl Fn() -> HashMap<String, bool>,
) -> Vec<String> {
    let settings = get_initial_settings();
    let mut enabled_plugins: Vec<String> = Vec::new();

    let add_dir_plugins = get_add_dir_enabled_plugins();
    for (plugin_id, value) in &add_dir_plugins {
        if plugin_id.contains('@') && *value {
            enabled_plugins.push(plugin_id.clone());
        }
    }

    if let Some(ref ep) = settings.enabled_plugins {
        for (plugin_id, value) in ep {
            if !plugin_id.contains('@') { continue; }
            let idx = enabled_plugins.iter().position(|p| p == plugin_id);
            if *value {
                if idx.is_none() { enabled_plugins.push(plugin_id.clone()); }
            } else {
                if let Some(i) = idx { enabled_plugins.remove(i); }
            }
        }
    }
    enabled_plugins
}

/// Gets the user-editable scope that "owns" each enabled plugin.
pub fn get_plugin_editable_scopes(
    get_add_dir_enabled_plugins: impl Fn() -> HashMap<String, bool>,
    get_settings_for_source: impl Fn(&str) -> Option<SourceSettings>,
) -> HashMap<String, ExtendedPluginScope> {
    let mut result = HashMap::new();
    let add_dir_plugins = get_add_dir_enabled_plugins();

    for (plugin_id, value) in &add_dir_plugins {
        if !plugin_id.contains('@') { continue; }
        if *value { result.insert(plugin_id.clone(), ExtendedPluginScope::Flag); }
        else { result.remove(plugin_id); }
    }

    let scope_sources = [
        (ExtendedPluginScope::Managed, "policySettings"),
        (ExtendedPluginScope::User, "userSettings"),
        (ExtendedPluginScope::Project, "projectSettings"),
        (ExtendedPluginScope::Local, "localSettings"),
        (ExtendedPluginScope::Flag, "flagSettings"),
    ];

    for (scope, source) in &scope_sources {
        if let Some(settings) = get_settings_for_source(source) {
            if let Some(ref ep) = settings.enabled_plugins {
                for (plugin_id, value) in ep {
                    if !plugin_id.contains('@') { continue; }
                    if *value { result.insert(plugin_id.clone(), scope.clone()); }
                    else { result.remove(plugin_id); }
                }
            }
        }
    }

    debug!("Found {} enabled plugins with scopes", result.len());
    result
}

/// Check if a scope is persistable (not session-only).
pub fn is_persistable_scope(scope: &ExtendedPluginScope) -> bool {
    !matches!(scope, ExtendedPluginScope::Flag)
}

/// Gets the list of currently installed plugins.
pub async fn get_installed_plugins(
    migrate_from_enabled: impl std::future::Future<Output = Result<(), anyhow::Error>>,
    get_in_memory: impl Fn() -> super::schemas::InstalledPluginsFileV2,
) -> Vec<String> {
    // Trigger migration in background
    let _ = migrate_from_enabled.await;
    let v2_data = get_in_memory();
    let installed: Vec<String> = v2_data.plugins.keys().cloned().collect();
    debug!("Found {} installed plugins", installed.len());
    installed
}

/// Finds plugins that are enabled but not installed.
pub async fn find_missing_plugins(
    enabled_plugins: &[String],
    get_installed: impl std::future::Future<Output = Vec<String>>,
    get_plugin_by_id: impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<()>> + Send>>,
) -> Vec<String> {
    let installed = get_installed.await;
    let not_installed: Vec<&String> = enabled_plugins.iter().filter(|id| !installed.contains(id)).collect();

    let mut missing = Vec::new();
    for plugin_id in not_installed {
        if get_plugin_by_id(plugin_id).await.is_some() {
            missing.push(plugin_id.clone());
        }
    }
    missing
}

/// Result of plugin installation attempt.
#[derive(Debug, Clone)]
pub struct PluginInstallResult {
    pub installed: Vec<String>,
    pub failed: Vec<PluginInstallFailure>,
}

#[derive(Debug, Clone)]
pub struct PluginInstallFailure {
    pub name: String,
    pub error: String,
}

/// Installs the selected plugins.
pub async fn install_selected_plugins(
    plugins_to_install: &[String],
    scope: &str,
    get_plugin_by_id: impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<PluginInfo>> + Send>>,
    cache_and_register_plugin: impl Fn(&str, &str, &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>> + Send>>,
    update_settings: impl Fn(&str, HashMap<String, bool>),
    on_progress: Option<&dyn Fn(&str, usize, usize)>,
) -> PluginInstallResult {
    let mut installed = Vec::new();
    let mut failed = Vec::new();
    let mut updated_enabled = HashMap::new();

    for (i, plugin_id) in plugins_to_install.iter().enumerate() {
        if let Some(progress_fn) = on_progress {
            progress_fn(plugin_id, i + 1, plugins_to_install.len());
        }

        match get_plugin_by_id(plugin_id).await {
            Some(info) => {
                match cache_and_register_plugin(plugin_id, &info.install_location, scope).await {
                    Ok(_) => {
                        updated_enabled.insert(plugin_id.clone(), true);
                        installed.push(plugin_id.clone());
                    }
                    Err(e) => failed.push(PluginInstallFailure { name: plugin_id.clone(), error: e.to_string() }),
                }
            }
            None => failed.push(PluginInstallFailure { name: plugin_id.clone(), error: "Plugin not found in any marketplace".to_string() }),
        }
    }

    update_settings(scope, updated_enabled);
    PluginInstallResult { installed, failed }
}

#[derive(Debug, Clone)]
pub struct MergedSettings {
    pub enabled_plugins: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone)]
pub struct SourceSettings {
    pub enabled_plugins: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub install_location: String,
}

/// Outcome of [`perform_startup_checks`]. Mirror of the side-effects the TS
/// `performStartupChecks` function performs against `AppState`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartupCheckOutcome {
    /// `true` when the function was skipped because trust wasn't accepted.
    pub skipped_trust_not_accepted: bool,
    /// `true` when seed marketplaces registered new entries and caches
    /// were cleared (caller should also set `needsRefresh` on `AppState`).
    pub seed_marketplaces_changed: bool,
    /// `true` when background plugin installations were dispatched.
    pub background_installs_dispatched: bool,
    /// Optional human-readable error captured during the run. Errors are
    /// surfaced here so callers can record them in telemetry — they're
    /// never propagated so startup is never blocked.
    pub error: Option<String>,
}

/// Mirror of TS `performStartupChecks`. Runs the startup-time plugin
/// checks: verify the trust dialog has been accepted, register seed
/// marketplaces (clearing caches when their set changes), and kick off
/// background plugin installations.
///
/// Each side effect is delegated to a caller-supplied async closure so
/// `mossen-utils` doesn't depend on the AppState shape or the
/// PluginInstallationManager.
pub async fn perform_startup_checks<TrustCheck, SeedRegister, ClearMpCache, ClearPluginCache, BgInstall, FTrust, FSeed, FClearMp, FClearPl, FBg>(
    check_has_trust_dialog_accepted: TrustCheck,
    register_seed_marketplaces: SeedRegister,
    clear_marketplaces_cache: ClearMpCache,
    clear_plugin_cache: ClearPluginCache,
    perform_background_plugin_installations: BgInstall,
) -> StartupCheckOutcome
where
    TrustCheck: FnOnce() -> FTrust,
    SeedRegister: FnOnce() -> FSeed,
    ClearMpCache: FnOnce() -> FClearMp,
    ClearPluginCache: FnOnce(&'static str) -> FClearPl,
    BgInstall: FnOnce() -> FBg,
    FTrust: std::future::Future<Output = bool>,
    FSeed: std::future::Future<Output = Result<bool, String>>,
    FClearMp: std::future::Future<Output = ()>,
    FClearPl: std::future::Future<Output = ()>,
    FBg: std::future::Future<Output = Result<(), String>>,
{
    let mut outcome = StartupCheckOutcome::default();

    if !check_has_trust_dialog_accepted().await {
        outcome.skipped_trust_not_accepted = true;
        return outcome;
    }

    match register_seed_marketplaces().await {
        Ok(true) => {
            outcome.seed_marketplaces_changed = true;
            clear_marketplaces_cache().await;
            clear_plugin_cache("performStartupChecks: seed marketplaces changed").await;
        }
        Ok(false) => {}
        Err(e) => {
            outcome.error = Some(e);
            return outcome;
        }
    }

    if let Err(e) = perform_background_plugin_installations().await {
        outcome.error = Some(e);
    } else {
        outcome.background_installs_dispatched = true;
    }

    outcome
}
