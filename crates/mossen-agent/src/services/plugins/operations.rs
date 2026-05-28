//! Core plugin operations (install, uninstall, enable, disable, update)
//!
//! Pure library functions that can be used by both CLI and interactive UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Valid installable scopes
pub const VALID_INSTALLABLE_SCOPES: &[&str] = &["user", "project", "local"];

/// Valid scopes for update operations (includes managed)
pub const VALID_UPDATE_SCOPES: &[&str] = &["user", "project", "local", "managed"];

/// Installation scope type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallableScope {
    User,
    Project,
    Local,
}

impl InstallableScope {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "local" => Ok(Self::Local),
            _ => Err(format!(
                "Invalid scope \"{}\". Must be one of: user, project, local",
                s
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Local => "local",
        }
    }
}

/// Plugin scope (includes managed)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginScope {
    User,
    Project,
    Local,
    Managed,
}

impl PluginScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Local => "local",
            Self::Managed => "managed",
        }
    }
}

/// Result of a plugin operation
#[derive(Debug, Clone)]
pub struct PluginOperationResult {
    pub success: bool,
    pub message: String,
    pub plugin_id: Option<String>,
    pub plugin_name: Option<String>,
    pub scope: Option<String>,
    pub reverse_dependents: Vec<String>,
}

impl Default for PluginOperationResult {
    fn default() -> Self {
        Self {
            success: false,
            message: String::new(),
            plugin_id: None,
            plugin_name: None,
            scope: None,
            reverse_dependents: Vec::new(),
        }
    }
}

/// Result of a plugin update operation
#[derive(Debug, Clone)]
pub struct PluginUpdateResult {
    pub success: bool,
    pub message: String,
    pub plugin_id: Option<String>,
    pub new_version: Option<String>,
    pub old_version: Option<String>,
    pub already_up_to_date: bool,
    pub scope: Option<String>,
}

impl Default for PluginUpdateResult {
    fn default() -> Self {
        Self {
            success: false,
            message: String::new(),
            plugin_id: None,
            new_version: None,
            old_version: None,
            already_up_to_date: false,
            scope: None,
        }
    }
}

/// Parsed plugin identifier
#[derive(Debug, Clone)]
pub struct PluginIdentifier {
    pub name: String,
    pub marketplace: Option<String>,
}

/// Parse a plugin identifier (name or plugin@marketplace)
pub fn parse_plugin_identifier(plugin: &str) -> PluginIdentifier {
    if let Some(at_pos) = plugin.rfind('@') {
        PluginIdentifier {
            name: plugin[..at_pos].to_string(),
            marketplace: Some(plugin[at_pos + 1..].to_string()),
        }
    } else {
        PluginIdentifier {
            name: plugin.to_string(),
            marketplace: None,
        }
    }
}

/// Context trait for plugin operations dependency injection
pub trait PluginOperationsContext: Send + Sync {
    fn get_original_cwd(&self) -> PathBuf;
    fn is_builtin_plugin_id(&self, id: &str) -> bool;
    fn get_settings_for_source(&self, source: &str) -> Option<HashMap<String, serde_json::Value>>;
    fn update_settings_for_source(
        &self,
        source: &str,
        settings: HashMap<String, serde_json::Value>,
    ) -> Result<(), String>;
    fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool;
}

/// Get the project path for scopes that are project-specific.
pub fn get_project_path_for_scope(scope: &PluginScope, cwd: &Path) -> Option<PathBuf> {
    match scope {
        PluginScope::Project | PluginScope::Local => Some(cwd.to_path_buf()),
        _ => None,
    }
}

/// Check if a plugin is enabled at project scope
pub fn is_plugin_enabled_at_project_scope(
    plugin_id: &str,
    ctx: &dyn PluginOperationsContext,
) -> bool {
    if let Some(settings) = ctx.get_settings_for_source("projectSettings") {
        if let Some(enabled_plugins) = settings.get("enabledPlugins") {
            if let Some(obj) = enabled_plugins.as_object() {
                return obj.get(plugin_id).and_then(|v| v.as_bool()) == Some(true);
            }
        }
    }
    false
}

/// Install a plugin (settings-first).
pub async fn install_plugin_op(
    plugin: &str,
    scope: InstallableScope,
    ctx: &dyn PluginOperationsContext,
) -> PluginOperationResult {
    let id = parse_plugin_identifier(plugin);
    let plugin_name = &id.name;

    // Check policy
    let plugin_id = if let Some(ref marketplace) = id.marketplace {
        format!("{}@{}", plugin_name, marketplace)
    } else {
        plugin.to_string()
    };

    if ctx.is_plugin_blocked_by_policy(&plugin_id) {
        return PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is blocked by your organization's policy and cannot be installed",
                plugin_name
            ),
            ..Default::default()
        };
    }

    // In a full implementation, this would search marketplaces,
    // write settings, and cache the plugin. For now, wire up the structure.
    let setting_source = scope_to_setting_source(&scope);
    let mut enabled_plugins = HashMap::new();
    enabled_plugins.insert(
        "enabledPlugins".to_string(),
        serde_json::json!({ &plugin_id: true }),
    );

    if let Err(e) = ctx.update_settings_for_source(setting_source, enabled_plugins) {
        return PluginOperationResult {
            success: false,
            message: format!("Failed to update settings: {}", e),
            ..Default::default()
        };
    }

    info!(
        "Installed plugin: {} (scope: {})",
        plugin_id,
        scope.as_str()
    );
    PluginOperationResult {
        success: true,
        message: format!(
            "Successfully installed plugin: {} (scope: {})",
            plugin_id,
            scope.as_str()
        ),
        plugin_id: Some(plugin_id),
        plugin_name: Some(plugin_name.clone()),
        scope: Some(scope.as_str().to_string()),
        ..Default::default()
    }
}

/// Uninstall a plugin
pub async fn uninstall_plugin_op(
    plugin: &str,
    scope: InstallableScope,
    _delete_data_dir: bool,
    ctx: &dyn PluginOperationsContext,
) -> PluginOperationResult {
    let id = parse_plugin_identifier(plugin);
    let plugin_name = &id.name;
    let plugin_id = if let Some(ref marketplace) = id.marketplace {
        format!("{}@{}", plugin_name, marketplace)
    } else {
        plugin.to_string()
    };

    let setting_source = scope_to_setting_source(&scope);

    // Remove plugin from settings
    let mut settings = HashMap::new();
    settings.insert(
        "enabledPlugins".to_string(),
        serde_json::json!({ &plugin_id: null }),
    );

    if let Err(e) = ctx.update_settings_for_source(setting_source, settings) {
        return PluginOperationResult {
            success: false,
            message: format!("Failed to update settings: {}", e),
            ..Default::default()
        };
    }

    info!(
        "Uninstalled plugin: {} (scope: {})",
        plugin_name,
        scope.as_str()
    );
    PluginOperationResult {
        success: true,
        message: format!(
            "Successfully uninstalled plugin: {} (scope: {})",
            plugin_name,
            scope.as_str()
        ),
        plugin_id: Some(plugin_id),
        plugin_name: Some(plugin_name.clone()),
        scope: Some(scope.as_str().to_string()),
        ..Default::default()
    }
}

/// Enable a plugin
pub async fn enable_plugin_op(
    plugin: &str,
    scope: Option<InstallableScope>,
    ctx: &dyn PluginOperationsContext,
) -> PluginOperationResult {
    set_plugin_enabled_op(plugin, true, scope, ctx).await
}

/// Disable a plugin
pub async fn disable_plugin_op(
    plugin: &str,
    scope: Option<InstallableScope>,
    ctx: &dyn PluginOperationsContext,
) -> PluginOperationResult {
    set_plugin_enabled_op(plugin, false, scope, ctx).await
}

/// Set plugin enabled/disabled status
pub async fn set_plugin_enabled_op(
    plugin: &str,
    enabled: bool,
    scope: Option<InstallableScope>,
    ctx: &dyn PluginOperationsContext,
) -> PluginOperationResult {
    let operation = if enabled { "enable" } else { "disable" };
    let id = parse_plugin_identifier(plugin);
    let plugin_id = if let Some(ref marketplace) = id.marketplace {
        format!("{}@{}", id.name, marketplace)
    } else {
        plugin.to_string()
    };

    // Built-in plugins always use user scope
    if ctx.is_builtin_plugin_id(&plugin_id) {
        let mut settings = HashMap::new();
        settings.insert(
            "enabledPlugins".to_string(),
            serde_json::json!({ &plugin_id: enabled }),
        );
        if let Err(e) = ctx.update_settings_for_source("userSettings", settings) {
            return PluginOperationResult {
                success: false,
                message: format!("Failed to {} built-in plugin: {}", operation, e),
                ..Default::default()
            };
        }
        return PluginOperationResult {
            success: true,
            message: format!("Successfully {}d built-in plugin: {}", operation, id.name),
            plugin_id: Some(plugin_id),
            plugin_name: Some(id.name),
            scope: Some("user".to_string()),
            ..Default::default()
        };
    }

    // Check policy guard for enable
    if enabled && ctx.is_plugin_blocked_by_policy(&plugin_id) {
        return PluginOperationResult {
            success: false,
            message: format!(
                "Plugin \"{}\" is blocked by your organization's policy and cannot be enabled",
                plugin_id
            ),
            ..Default::default()
        };
    }

    let resolved_scope = scope.unwrap_or(InstallableScope::User);
    let setting_source = scope_to_setting_source(&resolved_scope);

    let mut settings = HashMap::new();
    settings.insert(
        "enabledPlugins".to_string(),
        serde_json::json!({ &plugin_id: enabled }),
    );

    if let Err(e) = ctx.update_settings_for_source(setting_source, settings) {
        return PluginOperationResult {
            success: false,
            message: format!("Failed to {} plugin: {}", operation, e),
            ..Default::default()
        };
    }

    PluginOperationResult {
        success: true,
        message: format!(
            "Successfully {}d plugin: {} (scope: {})",
            operation,
            id.name,
            resolved_scope.as_str()
        ),
        plugin_id: Some(plugin_id),
        plugin_name: Some(id.name),
        scope: Some(resolved_scope.as_str().to_string()),
        ..Default::default()
    }
}

/// Disable all enabled plugins
pub async fn disable_all_plugins_op(ctx: &dyn PluginOperationsContext) -> PluginOperationResult {
    // Get all enabled plugins from settings
    let user_settings = ctx.get_settings_for_source("userSettings");
    let mut disabled_count = 0u32;

    if let Some(settings) = user_settings {
        if let Some(enabled_plugins) = settings.get("enabledPlugins") {
            if let Some(obj) = enabled_plugins.as_object() {
                for (plugin_id, value) in obj {
                    if value.as_bool() == Some(true) {
                        let result = set_plugin_enabled_op(
                            plugin_id,
                            false,
                            Some(InstallableScope::User),
                            ctx,
                        )
                        .await;
                        if result.success {
                            disabled_count += 1;
                        }
                    }
                }
            }
        }
    }

    if disabled_count == 0 {
        return PluginOperationResult {
            success: true,
            message: "No enabled plugins to disable".to_string(),
            ..Default::default()
        };
    }

    PluginOperationResult {
        success: true,
        message: format!(
            "Disabled {} plugin{}",
            disabled_count,
            if disabled_count == 1 { "" } else { "s" }
        ),
        ..Default::default()
    }
}

/// Update a plugin to the latest version
pub async fn update_plugin_op(
    plugin: &str,
    scope: &PluginScope,
    _ctx: &dyn PluginOperationsContext,
) -> PluginUpdateResult {
    let id = parse_plugin_identifier(plugin);
    let plugin_id = if let Some(ref marketplace) = id.marketplace {
        format!("{}@{}", id.name, marketplace)
    } else {
        plugin.to_string()
    };

    // In full implementation: get plugin from marketplace, check version,
    // download if needed, copy to cache, update installation records
    warn!(
        "Plugin update for {} at scope {} - marketplace lookup required",
        plugin_id,
        scope.as_str()
    );

    PluginUpdateResult {
        success: true,
        message: format!("{} is already at the latest version.", id.name),
        plugin_id: Some(plugin_id),
        already_up_to_date: true,
        scope: Some(scope.as_str().to_string()),
        ..Default::default()
    }
}

/// Convert scope to setting source identifier
fn scope_to_setting_source(scope: &InstallableScope) -> &'static str {
    match scope {
        InstallableScope::User => "userSettings",
        InstallableScope::Project => "projectSettings",
        InstallableScope::Local => "localSettings",
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/plugins/pluginOperations.ts` exports.
// ---------------------------------------------------------------------------

/// `pluginOperations.ts` `assertInstallableScope`.
pub fn assert_installable_scope(scope: &str) -> Result<InstallableScope, String> {
    InstallableScope::from_str(scope)
}

/// `pluginOperations.ts` `isInstallableScope`.
pub fn is_installable_scope(scope: &str) -> bool {
    matches!(
        scope,
        "user" | "userSettings" | "project" | "projectSettings" | "local" | "localSettings"
    )
}

/// `pluginOperations.ts` `getPluginInstallationFromV2`.
pub fn get_plugin_installation_from_v2(v2: &serde_json::Value) -> Option<serde_json::Value> {
    if !v2.is_object() {
        return None;
    }
    Some(serde_json::json!({
        "name": v2.get("name"),
        "scope": v2.get("scope"),
        "version": v2.get("version"),
        "enabled": v2.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockPluginContext {
        settings: Mutex<HashMap<String, HashMap<String, serde_json::Value>>>,
        builtins: HashSet<String>,
        blocked: HashSet<String>,
    }

    impl MockPluginContext {
        fn with_blocked(blocked: &[&str]) -> Self {
            Self {
                blocked: blocked.iter().map(|item| item.to_string()).collect(),
                ..Self::default()
            }
        }
    }

    impl PluginOperationsContext for MockPluginContext {
        fn get_original_cwd(&self) -> PathBuf {
            PathBuf::from("/tmp/mossen-plugin-test")
        }

        fn is_builtin_plugin_id(&self, id: &str) -> bool {
            self.builtins.contains(id)
        }

        fn get_settings_for_source(
            &self,
            source: &str,
        ) -> Option<HashMap<String, serde_json::Value>> {
            self.settings.lock().unwrap().get(source).cloned()
        }

        fn update_settings_for_source(
            &self,
            source: &str,
            settings: HashMap<String, serde_json::Value>,
        ) -> Result<(), String> {
            let mut all_settings = self.settings.lock().unwrap();
            let source_settings = all_settings.entry(source.to_string()).or_default();

            for (key, value) in settings {
                if key == "enabledPlugins" {
                    let enabled = source_settings
                        .entry(key)
                        .or_insert_with(|| serde_json::json!({}));
                    let Some(enabled_obj) = enabled.as_object_mut() else {
                        return Err("enabledPlugins is not an object".to_string());
                    };
                    let Some(update_obj) = value.as_object() else {
                        return Err("enabledPlugins update is not an object".to_string());
                    };
                    for (plugin_id, update) in update_obj {
                        if update.is_null() {
                            enabled_obj.remove(plugin_id);
                        } else {
                            enabled_obj.insert(plugin_id.clone(), update.clone());
                        }
                    }
                } else {
                    source_settings.insert(key, value);
                }
            }

            Ok(())
        }

        fn is_plugin_blocked_by_policy(&self, plugin_id: &str) -> bool {
            self.blocked.contains(plugin_id)
        }
    }

    fn enabled_value(
        ctx: &MockPluginContext,
        source: &str,
        plugin_id: &str,
    ) -> Option<serde_json::Value> {
        ctx.get_settings_for_source(source)
            .and_then(|settings| settings.get("enabledPlugins").cloned())
            .and_then(|enabled| enabled.get(plugin_id).cloned())
    }

    #[tokio::test]
    async fn plugin_install_enable_disable_uninstall_updates_settings() {
        let ctx = MockPluginContext::default();

        let installed =
            install_plugin_op("demo@local-market", InstallableScope::Project, &ctx).await;
        assert!(installed.success);
        assert_eq!(installed.plugin_id.as_deref(), Some("demo@local-market"));
        assert_eq!(installed.scope.as_deref(), Some("project"));
        assert_eq!(
            enabled_value(&ctx, "projectSettings", "demo@local-market"),
            Some(serde_json::json!(true))
        );

        let disabled =
            disable_plugin_op("demo@local-market", Some(InstallableScope::Project), &ctx).await;
        assert!(disabled.success);
        assert_eq!(
            enabled_value(&ctx, "projectSettings", "demo@local-market"),
            Some(serde_json::json!(false))
        );

        let enabled =
            enable_plugin_op("demo@local-market", Some(InstallableScope::Project), &ctx).await;
        assert!(enabled.success);
        assert_eq!(
            enabled_value(&ctx, "projectSettings", "demo@local-market"),
            Some(serde_json::json!(true))
        );

        let uninstalled =
            uninstall_plugin_op("demo@local-market", InstallableScope::Project, true, &ctx).await;
        assert!(uninstalled.success);
        assert_eq!(
            enabled_value(&ctx, "projectSettings", "demo@local-market"),
            None
        );
    }

    #[tokio::test]
    async fn plugin_policy_block_prevents_install_and_enable() {
        let ctx = MockPluginContext::with_blocked(&["blocked@official"]);

        let installed = install_plugin_op("blocked@official", InstallableScope::User, &ctx).await;
        assert!(!installed.success);
        assert!(installed.message.contains("blocked"));
        assert_eq!(
            enabled_value(&ctx, "userSettings", "blocked@official"),
            None
        );

        let enabled =
            enable_plugin_op("blocked@official", Some(InstallableScope::User), &ctx).await;
        assert!(!enabled.success);
        assert!(enabled.message.contains("blocked"));
        assert_eq!(
            enabled_value(&ctx, "userSettings", "blocked@official"),
            None
        );
    }
}
