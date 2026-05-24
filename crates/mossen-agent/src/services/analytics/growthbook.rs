//! GrowthBook compatibility wrapper — local facade for feature flag resolution.
//!
//! All feature value lookups are resolved through the local Mossen config facade.
//! No remote GrowthBook client exists in the personal edition.

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// GrowthBook user attributes (type retained for backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthBookUserAttributes {
    pub id: String,
    pub session_id: String,
    pub device_id: String,
    pub platform: String,
    pub api_base_url_host: Option<String>,
    pub organization_uuid: Option<String>,
    pub account_uuid: Option<String>,
    pub user_type: Option<String>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub first_token_time: Option<u64>,
    pub email: Option<String>,
    pub app_version: Option<String>,
}

/// Config override storage.
static CONFIG_OVERRIDES: RwLock<Option<HashMap<String, Value>>> = RwLock::new(None);

/// Resolve a feature value via local facade.
fn resolve_via_facade<T: for<'de> Deserialize<'de> + Clone>(key: &str, default_value: T) -> T {
    // Check env overrides first
    if let Some(val) = get_env_override(key) {
        if let Ok(parsed) = serde_json::from_value::<T>(val) {
            return parsed;
        }
    }
    // Check programmatic overrides
    if let Ok(overrides) = CONFIG_OVERRIDES.read() {
        if let Some(map) = &*overrides {
            if let Some(val) = map.get(key) {
                if let Ok(parsed) = serde_json::from_value::<T>(val.clone()) {
                    return parsed;
                }
            }
        }
    }
    default_value
}

fn get_env_override(key: &str) -> Option<Value> {
    for env_key in &["MOSSEN_CONFIG_OVERRIDES", "MOSSEN_INTERNAL_FC_OVERRIDES"] {
        if let Ok(raw) = env::var(env_key) {
            if let Ok(parsed) = serde_json::from_str::<HashMap<String, Value>>(&raw) {
                if let Some(val) = parsed.get(key) {
                    return Some(val.clone());
                }
            }
        }
    }
    None
}

/// Get feature value (cached, may be stale). Primary public API.
pub fn get_feature_value_cached_may_be_stale<T: for<'de> Deserialize<'de> + Clone>(
    feature: &str,
    default_value: T,
) -> T {
    resolve_via_facade(feature, default_value)
}

/// Get feature value with refresh (refresh interval ignored in personal edition).
pub fn get_feature_value_cached_with_refresh<T: for<'de> Deserialize<'de> + Clone>(
    feature: &str,
    default_value: T,
    _refresh_interval_ms: u64,
) -> T {
    resolve_via_facade(feature, default_value)
}

/// Get dynamic config (cached, may be stale).
pub fn get_dynamic_config_cached_may_be_stale<T: for<'de> Deserialize<'de> + Clone>(
    config: &str,
    default_value: T,
) -> T {
    resolve_via_facade(config, default_value)
}

/// Get dynamic config (blocks on init — in personal edition, resolves immediately).
pub async fn get_dynamic_config_blocks_on_init<T: for<'de> Deserialize<'de> + Clone>(
    config: &str,
    default_value: T,
) -> T {
    resolve_via_facade(config, default_value)
}

/// Deprecated: async feature value getter.
pub async fn get_feature_value_deprecated<T: for<'de> Deserialize<'de> + Clone>(
    feature: &str,
    default_value: T,
) -> T {
    resolve_via_facade(feature, default_value)
}

/// Check a Statsig feature gate (cached, may be stale).
pub fn check_statsig_feature_gate_cached_may_be_stale(gate: &str) -> bool {
    resolve_via_facade::<bool>(gate, false)
}

/// Check gate (cached or blocking — in personal edition, always synchronous).
pub async fn check_gate_cached_or_blocking(gate: &str) -> bool {
    resolve_via_facade::<bool>(gate, false)
}

/// Security restriction gate. Personal edition always returns false.
pub async fn check_security_restriction_gate(_gate: &str) -> bool {
    false
}

/// No-op: GrowthBook remote client removed.
pub async fn initialize_growthbook() {}

/// No-op.
pub fn reset_growthbook() {}

/// No-op.
pub async fn refresh_growthbook_features() {}

/// No-op.
pub fn refresh_growthbook_after_auth_change() {}

/// No-op.
pub fn setup_periodic_growthbook_refresh() {}

/// No-op.
pub fn stop_periodic_growthbook_refresh() {}

/// Check if a feature has an env override.
pub fn has_growthbook_env_override(feature: &str) -> bool {
    get_env_override(feature).is_some()
}

/// Get all feature values (from overrides + defaults).
pub fn get_all_growthbook_features() -> HashMap<String, Value> {
    let overrides = CONFIG_OVERRIDES.read().unwrap();
    overrides.clone().unwrap_or_default()
}

/// Legacy: always returns empty (field frozen).
pub fn get_growthbook_config_overrides() -> HashMap<String, Value> {
    HashMap::new()
}

/// Set a config override.
pub fn set_growthbook_config_override(key: &str, value: Value) {
    let mut overrides = CONFIG_OVERRIDES.write().unwrap();
    let map = overrides.get_or_insert_with(HashMap::new);
    map.insert(key.to_string(), value);
}

/// Clear all config overrides.
pub fn clear_growthbook_config_overrides() {
    let mut overrides = CONFIG_OVERRIDES.write().unwrap();
    *overrides = None;
}

/// Get current API base URL host (from custom backend config).
pub fn get_api_base_url_host() -> Option<String> {
    env::var("CUSTOM_API_BASE_URL")
        .ok()
        .and_then(|url| url::Url::parse(&url).ok())
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

/// Register a callback invoked when GrowthBook refreshes its values. Mirrors
/// TS `onGrowthBookRefresh(listener)`. Returns an unsubscribe closure.
pub fn on_growth_book_refresh(listener: Box<dyn Fn() + Send + Sync>) -> Box<dyn FnOnce() + Send> {
    // GrowthBook refresh listeners are stored module-level; we keep them
    // alive via a Vec of boxed callbacks.
    let mut guard = GROWTHBOOK_REFRESH_LISTENERS.lock().unwrap();
    guard.push(listener);
    let id = guard.len();
    drop(guard);
    Box::new(move || {
        let mut guard = GROWTHBOOK_REFRESH_LISTENERS.lock().unwrap();
        if id <= guard.len() {
            guard.remove(id - 1);
        }
    })
}

static GROWTHBOOK_REFRESH_LISTENERS: once_cell::sync::Lazy<
    std::sync::Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));
