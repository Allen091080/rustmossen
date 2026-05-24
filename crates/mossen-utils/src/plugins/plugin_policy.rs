//! Plugin policy checks backed by managed settings.
//!
//! Translated from `utils/plugins/pluginPolicy.ts` (20 lines).

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;

/// In-memory store for policy-enabled plugins.
/// Maps pluginId -> enabled (true/false).
static POLICY_ENABLED_PLUGINS: Lazy<RwLock<HashMap<String, bool>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Set the policy enabled plugins map (called when policy settings load).
pub fn set_policy_enabled_plugins(plugins: HashMap<String, bool>) {
    *POLICY_ENABLED_PLUGINS.write() = plugins;
}

/// Check if a plugin is force-disabled by org policy (managed-settings.json).
///
/// Policy-blocked plugins cannot be installed or enabled by the user at any
/// scope. Used as the single source of truth for policy blocking across the
/// install chokepoint, enable op, and UI filters.
pub fn is_plugin_blocked_by_policy(plugin_id: &str) -> bool {
    let map = POLICY_ENABLED_PLUGINS.read();
    match map.get(plugin_id) {
        Some(&false) => true,
        _ => false,
    }
}
