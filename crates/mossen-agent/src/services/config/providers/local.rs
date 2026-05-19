//! Local file-based configuration providers.
//!
//! - LocalDefaultProvider: built-in defaults, read-only, lowest priority
//! - UserSettingsProvider: ~/.mossen/settings.json
//! - ProjectSettingsProvider: <cwd>/.mossen/settings.json

use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::services::config::defaults::MOSSEN_BUILTIN_DEFAULTS;
use crate::services::config::types::{ConfigValueSource, MossenConfigProvider, ProviderResult};

/// Built-in default provider (read-only, lowest priority).
pub struct LocalDefaultProvider;

impl MossenConfigProvider for LocalDefaultProvider {
    fn name(&self) -> ConfigValueSource {
        ConfigValueSource::Default
    }

    fn priority(&self) -> u8 {
        ConfigValueSource::Default.priority()
    }

    fn enabled(&self) -> bool {
        true
    }

    fn get(&self, key: &str) -> Option<ProviderResult> {
        MOSSEN_BUILTIN_DEFAULTS.get(key).map(|value| ProviderResult {
            value: value.clone(),
            source: ConfigValueSource::Default,
            resolved_key: None,
        })
    }
}

/// Shared base implementation for file-based settings providers.
struct SettingsProviderInner {
    settings_path: PathBuf,
    secure_permission_mode: Option<u32>,
}

impl SettingsProviderInner {
    fn read_settings(&self) -> Option<HashMap<String, Value>> {
        if !self.settings_path.exists() {
            return None;
        }
        let raw = fs::read_to_string(&self.settings_path).ok()?;
        let parsed: Value = serde_json::from_str(&raw).ok()?;
        match parsed {
            Value::Object(map) => {
                let mut result = HashMap::new();
                for (k, v) in map {
                    result.insert(k, v);
                }
                Some(result)
            }
            _ => None,
        }
    }

    fn get_value(&self, key: &str, source: ConfigValueSource) -> Option<ProviderResult> {
        let data = self.read_settings()?;
        data.get(key).map(|value| ProviderResult {
            value: value.clone(),
            source,
            resolved_key: None,
        })
    }

    fn set_value(&self, key: &str, value: Value) {
        let mut current = self.read_settings().unwrap_or_default();
        current.insert(key.to_string(), value);
        if let Some(parent) = self.settings_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let map: serde_json::Map<String, Value> = current.into_iter().collect();
        let json_str = serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default();
        let content = format!("{}\n", json_str);
        let _ = fs::write(&self.settings_path, content);
        self.enforce_secure_permission();
    }

    fn clear_value(&self, key: Option<&str>) {
        if !self.settings_path.exists() {
            return;
        }
        match key {
            None => {
                let _ = fs::remove_file(&self.settings_path);
            }
            Some(k) => {
                if let Some(mut current) = self.read_settings() {
                    if current.contains_key(k) {
                        current.remove(k);
                        let map: serde_json::Map<String, Value> =
                            current.into_iter().collect();
                        let json_str =
                            serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default();
                        let content = format!("{}\n", json_str);
                        let _ = fs::write(&self.settings_path, content);
                        self.enforce_secure_permission();
                    }
                }
            }
        }
    }

    #[cfg(unix)]
    fn enforce_secure_permission(&self) {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = self.secure_permission_mode {
            if let Ok(metadata) = fs::metadata(&self.settings_path) {
                let current_mode = metadata.permissions().mode() & 0o777;
                if current_mode != mode {
                    let _ = fs::set_permissions(
                        &self.settings_path,
                        fs::Permissions::from_mode(mode),
                    );
                }
            }
        }
    }

    #[cfg(not(unix))]
    fn enforce_secure_permission(&self) {
        // chmod not available on non-Unix platforms
    }
}

/// User settings provider (~/.mossen/settings.json).
pub struct UserSettingsProvider {
    inner: Mutex<SettingsProviderInner>,
}

impl UserSettingsProvider {
    pub fn new() -> Self {
        let config_dir = env::var("MOSSEN_CONFIG_DIR").unwrap_or_else(|_| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".mossen").to_string_lossy().to_string()
        });
        let settings_path = Path::new(&config_dir).join("settings.json");
        Self {
            inner: Mutex::new(SettingsProviderInner {
                settings_path,
                secure_permission_mode: Some(0o600),
            }),
        }
    }
}

impl Default for UserSettingsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MossenConfigProvider for UserSettingsProvider {
    fn name(&self) -> ConfigValueSource {
        ConfigValueSource::User
    }

    fn priority(&self) -> u8 {
        ConfigValueSource::User.priority()
    }

    fn enabled(&self) -> bool {
        true
    }

    fn get(&self, key: &str) -> Option<ProviderResult> {
        let inner = self.inner.lock();
        inner.get_value(key, ConfigValueSource::User)
    }

    fn set(&self, key: &str, value: Value) {
        let inner = self.inner.lock();
        inner.set_value(key, value);
    }

    fn clear(&self, key: Option<&str>) {
        let inner = self.inner.lock();
        inner.clear_value(key);
    }
}

/// Project settings provider (<cwd>/.mossen/settings.json).
pub struct ProjectSettingsProvider {
    inner: Mutex<SettingsProviderInner>,
}

impl ProjectSettingsProvider {
    pub fn new() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let settings_path = cwd.join(".mossen").join("settings.json");
        Self {
            inner: Mutex::new(SettingsProviderInner {
                settings_path,
                secure_permission_mode: None,
            }),
        }
    }
}

impl Default for ProjectSettingsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MossenConfigProvider for ProjectSettingsProvider {
    fn name(&self) -> ConfigValueSource {
        ConfigValueSource::Project
    }

    fn priority(&self) -> u8 {
        ConfigValueSource::Project.priority()
    }

    fn enabled(&self) -> bool {
        true
    }

    fn get(&self, key: &str) -> Option<ProviderResult> {
        let inner = self.inner.lock();
        inner.get_value(key, ConfigValueSource::Project)
    }

    fn set(&self, key: &str, value: Value) {
        let inner = self.inner.lock();
        inner.set_value(key, value);
    }

    fn clear(&self, key: Option<&str>) {
        let inner = self.inner.lock();
        inner.clear_value(key);
    }
}
