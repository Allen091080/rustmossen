use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde_json::Value;
use tracing::debug;

/// Type alias for plugin option values.
pub type PluginOptionValues = HashMap<String, Value>;
/// Type alias for plugin option schema.
pub type PluginOptionSchema = HashMap<String, UserConfigSchemaEntry>;

/// A single field in the user config schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserConfigSchemaEntry {
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub default: Option<Value>,
}

/// Trait abstracting secure storage for plugin secrets.
pub trait SecureStorage: Send + Sync {
    fn read(&self) -> Option<SecureStorageData>;
    fn update(&self, data: SecureStorageData) -> SecureStorageResult;
}

#[derive(Debug, Clone, Default)]
pub struct SecureStorageData {
    pub plugin_secrets: Option<HashMap<String, HashMap<String, String>>>,
}

pub struct SecureStorageResult {
    pub success: bool,
    pub warning: Option<String>,
}

/// Trait abstracting settings read/write.
pub trait SettingsProvider: Send + Sync {
    fn get_plugin_configs(&self) -> HashMap<String, PluginConfigEntry>;
    fn update_plugin_configs(
        &self,
        configs: HashMap<String, PluginConfigEntry>,
    ) -> Result<(), anyhow::Error>;
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PluginConfigEntry {
    #[serde(default)]
    pub options: Option<PluginOptionValues>,
}

/// Memoized plugin options cache (per plugin-id).
static PLUGIN_OPTIONS_CACHE: Lazy<Mutex<HashMap<String, PluginOptionValues>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Canonical storage key for a plugin's options.
/// Today this is `plugin.source` — always `"name@marketplace"`.
pub fn get_plugin_storage_id(plugin_source: &str) -> String {
    plugin_source.to_string()
}

/// Load saved option values for a plugin, merging non-sensitive (from settings)
/// with sensitive (from secureStorage). SecureStorage wins on key collision.
///
/// Memoized per plugin_id.
pub fn load_plugin_options(
    plugin_id: &str,
    settings: &dyn SettingsProvider,
    secure_storage: &dyn SecureStorage,
) -> PluginOptionValues {
    {
        let cache = PLUGIN_OPTIONS_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(plugin_id) {
            return cached.clone();
        }
    }

    let configs = settings.get_plugin_configs();
    let non_sensitive = configs
        .get(plugin_id)
        .and_then(|c| c.options.clone())
        .unwrap_or_default();

    let sensitive: HashMap<String, Value> = secure_storage
        .read()
        .and_then(|data| data.plugin_secrets)
        .and_then(|secrets| secrets.get(plugin_id).cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();

    // secureStorage wins on collision
    let mut merged = non_sensitive;
    for (k, v) in sensitive {
        merged.insert(k, v);
    }

    let mut cache = PLUGIN_OPTIONS_CACHE.lock().unwrap();
    cache.insert(plugin_id.to_string(), merged.clone());
    merged
}

/// Clear the memoized plugin options cache.
pub fn clear_plugin_options_cache() {
    let mut cache = PLUGIN_OPTIONS_CACHE.lock().unwrap();
    cache.clear();
}

/// Save option values, splitting by `schema[key].sensitive`.
/// Non-sensitive go to settings; sensitive go to secureStorage.
/// Clears the load cache on success.
pub fn save_plugin_options(
    plugin_id: &str,
    values: &PluginOptionValues,
    schema: &PluginOptionSchema,
    settings: &dyn SettingsProvider,
    secure_storage: &dyn SecureStorage,
) -> Result<(), anyhow::Error> {
    let mut non_sensitive = PluginOptionValues::new();
    let mut sensitive: HashMap<String, String> = HashMap::new();

    for (key, value) in values.iter() {
        if schema.get(key).map_or(false, |s| s.sensitive) {
            sensitive.insert(key.clone(), value.to_string());
        } else {
            non_sensitive.insert(key.clone(), value.clone());
        }
    }

    let sensitive_keys_in_save: std::collections::HashSet<String> =
        sensitive.keys().cloned().collect();
    let non_sensitive_keys_in_save: std::collections::HashSet<String> =
        non_sensitive.keys().cloned().collect();

    // secureStorage FIRST — if keychain fails, throw before touching settings
    let existing_secure = secure_storage.read();
    let existing_in_secure = existing_secure
        .as_ref()
        .and_then(|d| d.plugin_secrets.as_ref())
        .and_then(|s| s.get(plugin_id))
        .cloned();

    let secure_scrubbed: Option<HashMap<String, String>> =
        existing_in_secure.as_ref().map(|existing| {
            existing
                .iter()
                .filter(|(k, _)| !non_sensitive_keys_in_save.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        });

    let need_secure_scrub = match (&secure_scrubbed, &existing_in_secure) {
        (Some(scrubbed), Some(existing)) => scrubbed.len() != existing.len(),
        _ => false,
    };

    if !sensitive.is_empty() || need_secure_scrub {
        let mut data = secure_storage.read().unwrap_or_default();
        let plugin_secrets = data.plugin_secrets.get_or_insert_with(HashMap::new);
        let merged: HashMap<String, String> = secure_scrubbed
            .unwrap_or_default()
            .into_iter()
            .chain(sensitive.into_iter())
            .collect();
        plugin_secrets.insert(plugin_id.to_string(), merged);
        let result = secure_storage.update(data);
        if !result.success {
            return Err(anyhow::anyhow!(
                "Failed to save sensitive plugin options for {} to secure storage",
                plugin_id
            ));
        }
        if let Some(warning) = result.warning {
            debug!("Plugin secrets save warning: {}", warning);
        }
    }

    // settings AFTER secureStorage — scrub sensitive keys
    let mut configs = settings.get_plugin_configs();
    let existing_in_settings = configs
        .get(plugin_id)
        .and_then(|c| c.options.as_ref())
        .cloned()
        .unwrap_or_default();

    let keys_to_scrub: Vec<String> = existing_in_settings
        .keys()
        .filter(|k| sensitive_keys_in_save.contains(k.as_str()))
        .cloned()
        .collect();

    if !non_sensitive.is_empty() || !keys_to_scrub.is_empty() {
        let entry = configs.entry(plugin_id.to_string()).or_default();
        let mut opts = non_sensitive;
        // Scrub sensitive keys from settings by not including them
        for k in &keys_to_scrub {
            opts.remove(k);
        }
        entry.options = Some(opts);
        settings.update_plugin_configs(configs)?;
    }

    clear_plugin_options_cache();
    Ok(())
}

/// Delete all stored option values for a plugin — both non-sensitive and sensitive.
/// Best-effort: keychain write failure is logged but doesn't throw.
pub fn delete_plugin_options(
    plugin_id: &str,
    settings: &dyn SettingsProvider,
    secure_storage: &dyn SecureStorage,
) {
    // Settings side
    let mut configs = settings.get_plugin_configs();
    if configs.remove(plugin_id).is_some() {
        if let Err(e) = settings.update_plugin_configs(configs) {
            debug!(
                "deletePluginOptions: failed to clear settings.pluginConfigs[{}]: {}",
                plugin_id, e
            );
        }
    }

    // Secure storage side — delete pluginId and prefix matches
    if let Some(data) = secure_storage.read() {
        if let Some(secrets) = &data.plugin_secrets {
            let prefix = format!("{}/", plugin_id);
            let surviving: HashMap<String, HashMap<String, String>> = secrets
                .iter()
                .filter(|(k, _)| *k != plugin_id && !k.starts_with(&prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            if surviving.len() != secrets.len() {
                let new_data = SecureStorageData {
                    plugin_secrets: if surviving.is_empty() {
                        None
                    } else {
                        Some(surviving)
                    },
                };
                let result = secure_storage.update(new_data);
                if !result.success {
                    debug!(
                        "deletePluginOptions: failed to clear pluginSecrets for {} from keychain",
                        plugin_id
                    );
                }
            }
        }
    }

    clear_plugin_options_cache();
}

/// Find option keys whose saved values don't satisfy the schema.
/// Returns the schema slice for those keys, or empty if everything validates.
pub fn get_unconfigured_options(
    plugin_source: &str,
    manifest_schema: &PluginOptionSchema,
    settings: &dyn SettingsProvider,
    secure_storage: &dyn SecureStorage,
) -> PluginOptionSchema {
    if manifest_schema.is_empty() {
        return PluginOptionSchema::new();
    }

    let saved = load_plugin_options(plugin_source, settings, secure_storage);
    if validate_user_config(&saved, manifest_schema).is_ok() {
        return PluginOptionSchema::new();
    }

    let mut unconfigured = PluginOptionSchema::new();
    for (key, field_schema) in manifest_schema.iter() {
        let single_saved: PluginOptionValues =
            [(key.clone(), saved.get(key).cloned().unwrap_or(Value::Null))]
                .into_iter()
                .collect();
        let single_schema: PluginOptionSchema =
            [(key.clone(), field_schema.clone())].into_iter().collect();
        if validate_user_config(&single_saved, &single_schema).is_err() {
            unconfigured.insert(key.clone(), field_schema.clone());
        }
    }
    unconfigured
}

/// Validate user config values against schema.
fn validate_user_config(
    values: &PluginOptionValues,
    schema: &PluginOptionSchema,
) -> Result<(), String> {
    for (key, field_schema) in schema.iter() {
        if field_schema.required {
            match values.get(key) {
                None | Some(Value::Null) => {
                    return Err(format!("Missing required field: {}", key));
                }
                Some(Value::String(s)) if s.is_empty() => {
                    return Err(format!("Empty required field: {}", key));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Substitute ${MOSSEN_PLUGIN_ROOT} and ${MOSSEN_PLUGIN_DATA} with their paths.
/// On Windows, normalizes backslashes to forward slashes.
pub fn substitute_plugin_variables(
    value: &str,
    plugin_path: &str,
    plugin_source: Option<&str>,
    get_plugin_data_dir: impl Fn(&str) -> String,
) -> String {
    let normalize = |p: &str| -> String {
        if cfg!(windows) {
            p.replace('\\', "/")
        } else {
            p.to_string()
        }
    };

    let out = value.replace("${MOSSEN_PLUGIN_ROOT}", &normalize(plugin_path));

    if let Some(source) = plugin_source {
        out.replace(
            "${MOSSEN_PLUGIN_DATA}",
            &normalize(&get_plugin_data_dir(source)),
        )
    } else {
        out
    }
}

/// Substitute ${user_config.KEY} with saved option values.
/// Returns Err on missing keys.
pub fn substitute_user_config_variables(
    value: &str,
    user_config: &PluginOptionValues,
) -> Result<String, anyhow::Error> {
    let re = regex::Regex::new(r"\$\{user_config\.([^}]+)\}").unwrap();
    let mut result = String::new();
    let mut last_end = 0;

    for caps in re.captures_iter(value) {
        let full_match = caps.get(0).unwrap();
        let key = caps.get(1).unwrap().as_str();
        result.push_str(&value[last_end..full_match.start()]);

        match user_config.get(key) {
            Some(val) if !val.is_null() => {
                result.push_str(&value_to_string(val));
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Missing required user configuration value: {}. \
                     This should have been validated before variable substitution.",
                    key
                ));
            }
        }
        last_end = full_match.end();
    }
    result.push_str(&value[last_end..]);
    Ok(result)
}

/// Content-safe variant for skill/agent prose. Sensitive keys substitute to a
/// placeholder; unknown keys stay literal (no error).
pub fn substitute_user_config_in_content(
    content: &str,
    options: &PluginOptionValues,
    schema: &PluginOptionSchema,
) -> String {
    let re = regex::Regex::new(r"\$\{user_config\.([^}]+)\}").unwrap();
    let mut result = String::new();
    let mut last_end = 0;

    for caps in re.captures_iter(content) {
        let full_match = caps.get(0).unwrap();
        let key = caps.get(1).unwrap().as_str();
        result.push_str(&content[last_end..full_match.start()]);

        if schema.get(key).map_or(false, |s| s.sensitive) {
            result.push_str(&format!(
                "[sensitive option '{}' not available in skill content]",
                key
            ));
        } else {
            match options.get(key) {
                Some(val) if !val.is_null() => {
                    result.push_str(&value_to_string(val));
                }
                _ => {
                    // Unknown keys stay literal
                    result.push_str(full_match.as_str());
                }
            }
        }
        last_end = full_match.end();
    }
    result.push_str(&content[last_end..]);
    result
}

fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
