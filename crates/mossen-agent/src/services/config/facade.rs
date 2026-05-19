//! Configuration facade — provider chain + public API.
//!
//! Provider priority (override > env > project > user > default):
//!   override(0) > env(1) > project(2) > user(3) > default(5)

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::alias_map::resolve_aliased_key;
use super::defaults::MOSSEN_BUILTIN_DEFAULTS;
use super::providers::env_override::EnvOverrideProvider;
use super::providers::local::{LocalDefaultProvider, ProjectSettingsProvider, UserSettingsProvider};
use super::types::{ConfigOverrideScope, ConfigValueSource, MossenConfigProvider, ProviderResult};

/// Process-internal runtime override provider (highest priority).
struct RuntimeOverrideProvider {
    store: RwLock<HashMap<String, Value>>,
}

impl RuntimeOverrideProvider {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

impl MossenConfigProvider for RuntimeOverrideProvider {
    fn name(&self) -> ConfigValueSource {
        ConfigValueSource::Override
    }

    fn priority(&self) -> u8 {
        ConfigValueSource::Override.priority()
    }

    fn enabled(&self) -> bool {
        true
    }

    fn get(&self, key: &str) -> Option<ProviderResult> {
        let store = self.store.read();
        store.get(key).map(|value| ProviderResult {
            value: value.clone(),
            source: ConfigValueSource::Override,
            resolved_key: None,
        })
    }

    fn set(&self, key: &str, value: Value) {
        let mut store = self.store.write();
        store.insert(key.to_string(), value);
    }

    fn clear(&self, key: Option<&str>) {
        let mut store = self.store.write();
        match key {
            None => store.clear(),
            Some(k) => {
                store.remove(k);
            }
        }
    }
}

struct FacadeInner {
    runtime_override: Arc<RuntimeOverrideProvider>,
    env_override: Arc<EnvOverrideProvider>,
    project_settings: Arc<ProjectSettingsProvider>,
    user_settings: Arc<UserSettingsProvider>,
    local_default: Arc<LocalDefaultProvider>,
    refresh_listeners: RwLock<Vec<Arc<dyn Fn() + Send + Sync>>>,
}

static FACADE: Lazy<FacadeInner> = Lazy::new(|| FacadeInner {
    runtime_override: Arc::new(RuntimeOverrideProvider::new()),
    env_override: Arc::new(EnvOverrideProvider),
    project_settings: Arc::new(ProjectSettingsProvider::new()),
    user_settings: Arc::new(UserSettingsProvider::new()),
    local_default: Arc::new(LocalDefaultProvider),
    refresh_listeners: RwLock::new(Vec::new()),
});

impl FacadeInner {
    fn providers(&self) -> Vec<&dyn MossenConfigProvider> {
        vec![
            self.runtime_override.as_ref() as &dyn MossenConfigProvider,
            self.env_override.as_ref() as &dyn MossenConfigProvider,
            self.project_settings.as_ref() as &dyn MossenConfigProvider,
            self.user_settings.as_ref() as &dyn MossenConfigProvider,
            self.local_default.as_ref() as &dyn MossenConfigProvider,
        ]
    }

    fn pick_writable_provider(&self, scope: ConfigOverrideScope) -> &dyn MossenConfigProvider {
        match scope {
            ConfigOverrideScope::Override => self.runtime_override.as_ref(),
            ConfigOverrideScope::Project => self.project_settings.as_ref(),
            ConfigOverrideScope::User => self.user_settings.as_ref(),
        }
    }

    fn notify_refresh_listeners(&self) {
        let listeners = self.refresh_listeners.read();
        for listener in listeners.iter() {
            listener();
        }
    }
}

/// Resolve a key through the provider chain.
fn resolve(key: &str, default_value: Value) -> ResolveResult {
    let resolved_key = resolve_aliased_key(key);
    let is_aliased = resolved_key != key;

    for provider in FACADE.providers() {
        if !provider.enabled() {
            continue;
        }
        if let Some(result) = provider.get(resolved_key) {
            return ResolveResult {
                value: result.value,
                source: result.source,
                resolved_key: if is_aliased {
                    Some(resolved_key.to_string())
                } else {
                    None
                },
            };
        }
    }
    ResolveResult {
        value: default_value,
        source: ConfigValueSource::Default,
        resolved_key: if is_aliased {
            Some(resolved_key.to_string())
        } else {
            None
        },
    }
}

/// Result of resolving a configuration key.
#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub value: Value,
    pub source: ConfigValueSource,
    pub resolved_key: Option<String>,
}

// ===== Public facade API =====

/// Get a feature value by key.
pub fn get_mossen_feature_value(key: &str, default_value: Value) -> Value {
    resolve(key, default_value).value
}

/// Get a dynamic config by key.
pub fn get_mossen_dynamic_config(key: &str, default_value: Value) -> Value {
    resolve(key, default_value).value
}

/// Check a gate (boolean).
pub fn check_mossen_gate(key: &str, default_value: bool) -> bool {
    let v = resolve(key, Value::Bool(default_value)).value;
    match v {
        Value::Bool(b) => b,
        _ => default_value,
    }
}

/// Subscribe to config refresh events. Returns unsubscribe closure.
pub fn on_mossen_config_refresh(listener: Arc<dyn Fn() + Send + Sync>) -> impl FnOnce() {
    let listener_clone = Arc::clone(&listener);
    FACADE.refresh_listeners.write().push(listener_clone);
    let listener_ptr = Arc::as_ptr(&listener) as *const () as usize;
    move || {
        let mut listeners = FACADE.refresh_listeners.write();
        listeners.retain(|l| Arc::as_ptr(l) as *const () as usize != listener_ptr);
    }
}

/// Set a config override.
pub fn set_mossen_config_override(key: &str, value: Value, scope: ConfigOverrideScope) {
    let provider = FACADE.pick_writable_provider(scope);
    provider.set(key, value);
    FACADE.notify_refresh_listeners();
}

/// Clear config overrides.
pub fn clear_mossen_config_overrides(scope: ConfigOverrideScope, key: Option<&str>) {
    let provider = FACADE.pick_writable_provider(scope);
    provider.clear(key);
    FACADE.notify_refresh_listeners();
}

/// Get all resolved config values (built-in defaults only).
pub fn get_all_mossen_config_values() -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (key, default_val) in MOSSEN_BUILTIN_DEFAULTS.iter() {
        let resolved = resolve(key, default_val.clone());
        out.insert(key.to_string(), resolved.value);
    }
    out
}

/// Resolve a config key with full metadata.
pub fn resolve_mossen_config(key: &str, default_value: Value) -> ResolveResult {
    resolve(key, default_value)
}

/// Notify all refresh listeners (internal use).
pub fn notify_refresh_listeners() {
    FACADE.notify_refresh_listeners();
}

/// Reset facade for testing.
pub fn reset_facade_for_testing() {
    FACADE.runtime_override.clear(None);
    FACADE.refresh_listeners.write().clear();
}
