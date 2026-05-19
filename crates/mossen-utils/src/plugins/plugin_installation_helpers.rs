use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rand::Rng;
use tracing::debug;

use super::schemas::{PluginMarketplaceEntry, PluginScope, PluginSource};

/// Plugin installation metadata.
#[derive(Debug, Clone)]
pub struct PluginInstallationInfo {
    pub plugin_id: String,
    pub install_path: String,
    pub version: Option<String>,
}

/// Get current ISO timestamp.
pub fn get_current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Validate that a resolved path stays within a base directory.
/// Prevents path traversal attacks.
pub fn validate_path_within_base(base_path: &Path, relative_path: &str) -> Result<PathBuf, anyhow::Error> {
    let resolved = base_path.join(relative_path);
    let resolved = resolved.canonicalize().unwrap_or(resolved);
    let normalized_base = base_path.canonicalize().unwrap_or_else(|_| base_path.to_path_buf());

    if !resolved.starts_with(&normalized_base) && resolved != normalized_base {
        return Err(anyhow::anyhow!(
            "Path traversal detected: \"{}\" would escape the base directory",
            relative_path
        ));
    }

    Ok(resolved)
}

/// Structured result from the install core.
#[derive(Debug, Clone)]
pub enum InstallCoreResult {
    Ok { closure: Vec<String>, dep_note: String },
    LocalSourceNoLocation { plugin_name: String },
    SettingsWriteFailed { message: String },
    ResolutionFailed { message: String },
    BlockedByPolicy { plugin_name: String },
    DependencyBlockedByPolicy { plugin_name: String, blocked_dependency: String },
}

impl InstallCoreResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, InstallCoreResult::Ok { .. })
    }
}

/// Format a failed resolution into a user-facing message.
pub fn format_resolution_error(resolution: &ResolutionError) -> String {
    match resolution {
        ResolutionError::Cycle { chain } => {
            format!("Dependency cycle: {}", chain.join(" → "))
        }
        ResolutionError::CrossMarketplace { dependency, required_by, dep_marketplace } => {
            let where_str = match dep_marketplace {
                Some(mkt) => format!("marketplace \"{}\"", mkt),
                None => "a different marketplace".to_string(),
            };
            let hint = match dep_marketplace {
                Some(mkt) => format!(
                    " Add \"{}\" to allowCrossMarketplaceDependenciesOn in the ROOT marketplace's marketplace.json.",
                    mkt
                ),
                None => String::new(),
            };
            format!(
                "Dependency \"{}\" (required by {}) is in {}, which is not in the allowlist — cross-marketplace dependencies are blocked by default. Install it manually first.{}",
                dependency, required_by, where_str, hint
            )
        }
        ResolutionError::NotFound { missing, required_by, marketplace } => {
            match marketplace {
                Some(mkt) => format!(
                    "Dependency \"{}\" (required by {}) not found. Is the \"{}\" marketplace added?",
                    missing, required_by, mkt
                ),
                None => format!(
                    "Dependency \"{}\" (required by {}) not found in any configured marketplace",
                    missing, required_by
                ),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResolutionError {
    Cycle { chain: Vec<String> },
    CrossMarketplace { dependency: String, required_by: String, dep_marketplace: Option<String> },
    NotFound { missing: String, required_by: String, marketplace: Option<String> },
}

/// Trait for plugin installation operations.
#[async_trait::async_trait]
pub trait PluginInstallOps: Send + Sync {
    fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool;
    fn is_local_plugin_source(&self, source: &PluginSource) -> bool;
    async fn resolve_dependency_closure(
        &self,
        plugin_id: &str,
        scope: &str,
        allowed_cross: &HashSet<String>,
    ) -> Result<Vec<String>, ResolutionError>;
    async fn get_marketplace_cache_only(&self, marketplace: &str) -> Option<Vec<String>>;
    fn get_enabled_plugin_ids(&self, setting_source: &str) -> HashSet<String>;
    fn update_settings_enabled_plugins(&self, source: &str, plugins: &HashMap<String, bool>) -> Result<(), anyhow::Error>;
    async fn cache_and_register_plugin(
        &self,
        plugin_id: &str,
        entry: &PluginMarketplaceEntry,
        scope: PluginScope,
        project_path: Option<&str>,
        local_source_path: Option<&str>,
    ) -> Result<String, anyhow::Error>;
    fn clear_all_caches(&self);
    fn parse_plugin_identifier(&self, id: &str) -> (String, Option<String>);
    fn scope_to_setting_source(&self, scope: &str) -> String;
    fn format_dependency_count_suffix(&self, deps: &[String]) -> String;
    async fn get_plugin_by_id(&self, id: &str) -> Option<(PluginMarketplaceEntry, String)>;
    fn get_cwd(&self) -> Option<String>;
}

/// Core plugin install logic.
pub async fn install_resolved_plugin(
    plugin_id: &str,
    entry: &PluginMarketplaceEntry,
    scope: &str,
    marketplace_install_location: Option<&str>,
    ops: &dyn PluginInstallOps,
) -> InstallCoreResult {
    let setting_source = ops.scope_to_setting_source(scope);

    // Policy guard
    if ops.is_plugin_blocked_by_policy(plugin_id) {
        return InstallCoreResult::BlockedByPolicy {
            plugin_name: entry.name.clone(),
        };
    }

    // Check local source without location
    if ops.is_local_plugin_source(&entry.source) && marketplace_install_location.is_none() {
        return InstallCoreResult::LocalSourceNoLocation {
            plugin_name: entry.name.clone(),
        };
    }

    // Resolve dependency closure
    let (_, root_marketplace) = ops.parse_plugin_identifier(plugin_id);
    let allowed_cross = match &root_marketplace {
        Some(mkt) => ops
            .get_marketplace_cache_only(mkt)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<String>>(),
        None => HashSet::new(),
    };

    let closure = match ops
        .resolve_dependency_closure(plugin_id, &setting_source, &allowed_cross)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            return InstallCoreResult::ResolutionFailed {
                message: format_resolution_error(&e),
            };
        }
    };

    // Policy guard for transitive dependencies
    for id in &closure {
        if id != plugin_id && ops.is_plugin_blocked_by_policy(id) {
            return InstallCoreResult::DependencyBlockedByPolicy {
                plugin_name: entry.name.clone(),
                blocked_dependency: id.clone(),
            };
        }
    }

    // Write to settings
    let mut closure_enabled: HashMap<String, bool> = HashMap::new();
    for id in &closure {
        closure_enabled.insert(id.clone(), true);
    }
    if let Err(e) = ops.update_settings_enabled_plugins(&setting_source, &closure_enabled) {
        return InstallCoreResult::SettingsWriteFailed {
            message: e.to_string(),
        };
    }

    // Materialize: cache each closure member
    let project_path = if scope != "user" {
        ops.get_cwd()
    } else {
        None
    };

    let plugin_scope = match scope {
        "user" => PluginScope::User,
        "project" => PluginScope::Project,
        "local" => PluginScope::Local,
        _ => PluginScope::User,
    };

    for id in &closure {
        let info = if let Some(loc) = marketplace_install_location {
            if id == plugin_id {
                Some((entry.clone(), loc.to_string()))
            } else {
                ops.get_plugin_by_id(id).await
            }
        } else {
            ops.get_plugin_by_id(id).await
        };

        if let Some((dep_entry, install_loc)) = info {
            let local_source_path = if ops.is_local_plugin_source(&dep_entry.source) {
                match validate_path_within_base(Path::new(&install_loc), &dep_entry.source.to_string()) {
                    Ok(p) => Some(p.to_string_lossy().to_string()),
                    Err(_) => None,
                }
            } else {
                None
            };

            let _ = ops
                .cache_and_register_plugin(
                    id,
                    &dep_entry,
                    plugin_scope.clone(),
                    project_path.as_deref(),
                    local_source_path.as_deref(),
                )
                .await;
        }
    }

    ops.clear_all_caches();

    let dep_note = ops.format_dependency_count_suffix(
        &closure
            .iter()
            .filter(|id| *id != plugin_id)
            .cloned()
            .collect::<Vec<_>>(),
    );

    InstallCoreResult::Ok { closure, dep_note }
}

/// Result of a plugin installation operation.
#[derive(Debug, Clone)]
pub enum InstallPluginResult {
    Success { message: String },
    Failure { error: String },
}

/// Parameters for installing a plugin from marketplace.
#[derive(Debug, Clone)]
pub struct InstallPluginParams {
    pub plugin_id: String,
    pub entry: PluginMarketplaceEntry,
    pub marketplace_name: String,
    pub scope: String,
    pub trigger: String,
}

/// Install a single plugin from a marketplace with the specified scope.
pub async fn install_plugin_from_marketplace(
    params: &InstallPluginParams,
    ops: &dyn PluginInstallOps,
) -> InstallPluginResult {
    let plugin_info = ops.get_plugin_by_id(&params.plugin_id).await;
    let marketplace_install_location = plugin_info.as_ref().map(|(_, loc)| loc.as_str());

    let result = install_resolved_plugin(
        &params.plugin_id,
        &params.entry,
        &params.scope,
        marketplace_install_location,
        ops,
    )
    .await;

    match result {
        InstallCoreResult::Ok { dep_note, .. } => InstallPluginResult::Success {
            message: format!(
                "✓ Installed {}{}. Run /reload-plugins to activate.",
                params.entry.name, dep_note
            ),
        },
        InstallCoreResult::LocalSourceNoLocation { plugin_name } => InstallPluginResult::Failure {
            error: format!(
                "Cannot install local plugin \"{}\" without marketplace install location",
                plugin_name
            ),
        },
        InstallCoreResult::SettingsWriteFailed { message } => InstallPluginResult::Failure {
            error: format!("Failed to update settings: {}", message),
        },
        InstallCoreResult::ResolutionFailed { message } => InstallPluginResult::Failure {
            error: message,
        },
        InstallCoreResult::BlockedByPolicy { plugin_name } => InstallPluginResult::Failure {
            error: format!(
                "Plugin \"{}\" is blocked by your organization's policy and cannot be installed",
                plugin_name
            ),
        },
        InstallCoreResult::DependencyBlockedByPolicy {
            plugin_name,
            blocked_dependency,
        } => InstallPluginResult::Failure {
            error: format!(
                "Cannot install \"{}\": dependency \"{}\" is blocked by your organization's policy",
                plugin_name, blocked_dependency
            ),
        },
    }
}

/// Parse plugin ID into components.
pub fn parse_plugin_id(plugin_id: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = plugin_id.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string()))
}

/// Register a plugin installation without caching.
pub fn register_plugin_installation(
    info: &PluginInstallationInfo,
    scope: PluginScope,
    _project_path: Option<&str>,
) {
    let _now = get_current_timestamp();
    // In production, this would call addInstalledPlugin
    debug!(
        "Registered plugin {} at {} with scope {:?}",
        info.plugin_id, info.install_path, scope
    );
}
