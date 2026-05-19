//! Managed plugin names from policy settings.
//!
//! Translated from `utils/plugins/managedPlugins.ts` (27 lines).

use std::collections::{HashMap, HashSet};
use parking_lot::RwLock;
use once_cell::sync::Lazy;

/// In-memory store for policy-enabled plugins.
static POLICY_PLUGINS: Lazy<RwLock<HashMap<String, serde_json::Value>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Set the policy enabled plugins (called when settings load).
pub fn set_policy_plugins(plugins: HashMap<String, serde_json::Value>) {
    *POLICY_PLUGINS.write() = plugins;
}

/// Plugin names locked by org policy (policySettings.enabledPlugins).
///
/// Returns None when managed settings declare no plugin entries (common
/// case — no policy in effect).
pub fn get_managed_plugin_names() -> Option<HashSet<String>> {
    let enabled_plugins = POLICY_PLUGINS.read();
    if enabled_plugins.is_empty() {
        return None;
    }

    let mut names = HashSet::new();
    for (plugin_id, value) in enabled_plugins.iter() {
        // Only plugin@marketplace boolean entries (true OR false) are
        // protected. Legacy owner/repo array form is not.
        let is_boolean = value.is_boolean();
        if !is_boolean || !plugin_id.contains('@') {
            continue;
        }
        if let Some(name) = plugin_id.split('@').next() {
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }

    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}
