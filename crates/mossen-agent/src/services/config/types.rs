//! Configuration types — provider interface and shared types.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Source layer for a configuration value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValueSource {
    Env,
    Project,
    User,
    Default,
    Remote,
    Override,
}

impl ConfigValueSource {
    /// Provider priority (lower = higher priority).
    pub fn priority(&self) -> u8 {
        match self {
            Self::Override => 0,
            Self::Env => 1,
            Self::Project => 2,
            Self::User => 3,
            Self::Remote => 4,
            Self::Default => 5,
        }
    }
}

/// Result from a provider query.
#[derive(Debug, Clone)]
pub struct ProviderResult {
    pub value: Value,
    pub source: ConfigValueSource,
    pub resolved_key: Option<String>,
}

/// Configuration override scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOverrideScope {
    Override,
    User,
    Project,
}

/// Mossen key naming pattern. TS: `export const MOSSEN_KEY_PATTERN = /^mossen\.…/`.
pub static MOSSEN_KEY_PATTERN: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"^mossen\.[a-z][a-z0-9]*\.[a-zA-Z][a-zA-Z0-9]*$").unwrap());

/// Provider priority map (lower = higher priority). TS:
/// `export const PROVIDER_PRIORITY: Record<ConfigValueSource, number> = { … }`.
pub static PROVIDER_PRIORITY: Lazy<HashMap<ConfigValueSource, u8>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(ConfigValueSource::Override, 0);
    m.insert(ConfigValueSource::Env, 1);
    m.insert(ConfigValueSource::Project, 2);
    m.insert(ConfigValueSource::User, 3);
    m.insert(ConfigValueSource::Remote, 4);
    m.insert(ConfigValueSource::Default, 5);
    m
});

/// Mossen key pattern validation. Mirrors TS `validateMossenKey`.
pub fn validate_mossen_key(key: &str) -> Result<(), String> {
    if MOSSEN_KEY_PATTERN.is_match(key) {
        Ok(())
    } else {
        Err(format!(
            "invalid Mossen key \"{}\": expected mossen.<domain>.<feature>",
            key
        ))
    }
}

/// Resolved configuration record (debug/UI). Returned by `resolve_mossen_config`.
#[derive(Debug, Clone)]
pub struct ResolvedMossenConfig {
    pub value: Value,
    pub source: ConfigValueSource,
    pub resolved_key: Option<String>,
}

/// Mossen façade trait. Mirrors TS `interface MossenConfigFacade`.
pub trait MossenConfigFacade: Send + Sync {
    /// Equivalent to TS `getMossenFeatureValue`. Cached, may be stale.
    fn get_mossen_feature_value(&self, key: &str, default_value: Value) -> Value;
    /// Equivalent to TS `getMossenDynamicConfig`. Cached object value.
    fn get_mossen_dynamic_config(&self, key: &str, default_value: Value) -> Value;
    /// Equivalent to TS `checkMossenGate`. Cached boolean.
    fn check_mossen_gate(&self, key: &str, default_value: bool) -> bool;
    /// Subscribe to refresh, return an unsubscribe closure.
    fn on_mossen_config_refresh(
        &self,
        listener: MossenConfigRefreshListener,
    ) -> Box<dyn FnOnce() + Send>;
    /// Persist an override (scope = override is in-process; user/project writes file).
    fn set_mossen_config_override(
        &self,
        key: &str,
        value: Value,
        scope: Option<ConfigOverrideScope>,
    );
    /// Clear override(s). When key is None, clears the whole scope.
    fn clear_mossen_config_overrides(&self, scope: Option<ConfigOverrideScope>, key: Option<&str>);
    /// All currently-resolved values (debug/UI).
    fn get_all_mossen_config_values(&self) -> HashMap<String, Value>;
    /// Resolve a key returning value + provenance.
    fn resolve_mossen_config(&self, key: &str, default_value: Value) -> ResolvedMossenConfig;
}

/// GrowthBook → Mossen key alias map type.
pub type GrowthBookAliasMap = HashMap<String, String>;

/// Mossen config provider trait.
pub trait MossenConfigProvider: Send + Sync {
    fn name(&self) -> ConfigValueSource;
    fn priority(&self) -> u8;
    fn enabled(&self) -> bool;
    fn get(&self, key: &str) -> Option<ProviderResult>;
    fn set(&self, _key: &str, _value: Value) {}
    fn clear(&self, _key: Option<&str>) {}
}

/// Refresh listener type.
pub type MossenConfigRefreshListener = Box<dyn Fn() + Send + Sync>;
