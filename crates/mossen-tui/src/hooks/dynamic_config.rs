//! Dynamic config hook (useDynamicConfig.ts).
//!
//! Provides access to dynamically-fetched configuration values
//! (from GrowthBook or similar feature flag service).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A cached dynamic config value.
#[derive(Debug, Clone)]
pub struct DynamicConfigEntry {
    pub value: serde_json::Value,
    pub fetched: bool,
}

/// State for dynamic configuration access.
#[derive(Debug, Clone)]
pub struct DynamicConfigState {
    pub configs: HashMap<String, DynamicConfigEntry>,
}

impl DynamicConfigState {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Get a config value, returning the default if not yet fetched.
    pub fn get<T: serde::de::DeserializeOwned>(&self, name: &str, default: T) -> T {
        match self.configs.get(name) {
            Some(entry) if entry.fetched => {
                serde_json::from_value(entry.value.clone()).unwrap_or(default)
            }
            _ => default,
        }
    }

    /// Get a config value as a raw JSON value.
    pub fn get_raw(&self, name: &str) -> Option<&serde_json::Value> {
        self.configs
            .get(name)
            .filter(|e| e.fetched)
            .map(|e| &e.value)
    }

    /// Set a config value (called when fetch completes).
    pub fn set(&mut self, name: String, value: serde_json::Value) {
        self.configs.insert(
            name,
            DynamicConfigEntry {
                value,
                fetched: true,
            },
        );
    }

    /// Check if a config has been fetched.
    pub fn is_fetched(&self, name: &str) -> bool {
        self.configs.get(name).map_or(false, |e| e.fetched)
    }
}

impl Default for DynamicConfigState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared dynamic config store for async access.
pub type SharedDynamicConfig = Arc<RwLock<DynamicConfigState>>;
