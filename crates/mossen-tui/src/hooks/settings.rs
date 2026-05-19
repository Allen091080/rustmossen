//! Settings hook (useSettings.ts).
//! Provides reactive access to current settings from AppState.

use std::collections::HashMap;

/// Read-only settings snapshot from AppState.
#[derive(Debug, Clone)]
pub struct SettingsState {
    pub values: HashMap<String, serde_json::Value>,
    pub version: u64,
}

impl SettingsState {
    pub fn new() -> Self { Self { values: HashMap::new(), version: 0 } }
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.values.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    pub fn get_bool(&self, key: &str) -> Option<bool> { self.get(key) }
    pub fn get_string(&self, key: &str) -> Option<String> { self.get(key) }
    pub fn get_u64(&self, key: &str) -> Option<u64> { self.get(key) }
    pub fn update(&mut self, values: HashMap<String, serde_json::Value>) { self.values = values; self.version += 1; }
}
impl Default for SettingsState { fn default() -> Self { Self::new() } }

/// Settings type as stored in `AppState` (an immutable view). Translated
/// from TS `export type ReadonlySettings = AppState['settings']`. The TS
/// alias is structural; here we wrap the `SettingsState` snapshot.
pub type ReadonlySettings = SettingsState;
