use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::debug;

use super::plugin_directories::get_plugins_directory;

const FLAGGED_PLUGINS_FILENAME: &str = "flagged-plugins.json";
const SEEN_EXPIRY_MS: i64 = 48 * 60 * 60 * 1000; // 48 hours

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlaggedPlugin {
    #[serde(rename = "flaggedAt")]
    pub flagged_at: String,
    #[serde(rename = "seenAt", skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlaggedPluginsFile {
    plugins: HashMap<String, FlaggedPlugin>,
}

/// Module-level cache — populated by load_flagged_plugins(), updated by writes.
static CACHE: Lazy<Mutex<Option<HashMap<String, FlaggedPlugin>>>> = Lazy::new(|| Mutex::new(None));

fn get_flagged_plugins_path() -> String {
    format!(
        "{}/{}",
        get_plugins_directory().display(),
        FLAGGED_PLUGINS_FILENAME
    )
}

fn parse_plugins_data(content: &str) -> HashMap<String, FlaggedPlugin> {
    let parsed: Result<FlaggedPluginsFile, _> = serde_json::from_str(content);
    match parsed {
        Ok(file) => file.plugins,
        Err(_) => HashMap::new(),
    }
}

async fn read_from_disk() -> HashMap<String, FlaggedPlugin> {
    match fs::read_to_string(get_flagged_plugins_path()).await {
        Ok(content) => parse_plugins_data(&content),
        Err(_) => HashMap::new(),
    }
}

async fn write_to_disk(plugins: &HashMap<String, FlaggedPlugin>) {
    let file_path = get_flagged_plugins_path();
    let temp_path = format!(
        "{}.{}.tmp",
        file_path,
        hex::encode(rand::random::<[u8; 8]>())
    );

    let plugins_dir = get_plugins_directory();
    let _ = fs::create_dir_all(&plugins_dir).await;

    let file = FlaggedPluginsFile {
        plugins: plugins.clone(),
    };
    let content = match serde_json::to_string_pretty(&file) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to serialize flagged plugins: {}", e);
            return;
        }
    };

    match fs::write(&temp_path, &content).await {
        Ok(_) => {
            if let Err(e) = fs::rename(&temp_path, &file_path).await {
                debug!("Failed to rename flagged plugins temp file: {}", e);
                let _ = fs::remove_file(&temp_path).await;
            } else {
                let mut guard = CACHE.lock().unwrap();
                *guard = Some(plugins.clone());
            }
        }
        Err(e) => {
            debug!("Failed to write flagged plugins temp file: {}", e);
            let _ = fs::remove_file(&temp_path).await;
        }
    }
}

/// Load flagged plugins from disk into the module cache.
/// Must be called (and awaited) before get_flagged_plugins() returns meaningful data.
pub async fn load_flagged_plugins() {
    let mut all = read_from_disk().await;
    let now = Utc::now().timestamp_millis();
    let mut changed = false;

    let ids_to_remove: Vec<String> = all
        .iter()
        .filter_map(|(id, entry)| {
            if let Some(ref seen_at) = entry.seen_at {
                if let Ok(seen_time) = chrono::DateTime::parse_from_rfc3339(seen_at) {
                    if now - seen_time.timestamp_millis() >= SEEN_EXPIRY_MS {
                        return Some(id.clone());
                    }
                }
            }
            None
        })
        .collect();

    for id in &ids_to_remove {
        all.remove(id);
        changed = true;
    }

    {
        let mut guard = CACHE.lock().unwrap();
        *guard = Some(all.clone());
    }

    if changed {
        write_to_disk(&all).await;
    }
}

/// Get all flagged plugins from the in-memory cache.
/// Returns empty if load_flagged_plugins() has not been called yet.
pub fn get_flagged_plugins() -> HashMap<String, FlaggedPlugin> {
    let guard = CACHE.lock().unwrap();
    guard.clone().unwrap_or_default()
}

/// Add a plugin to the flagged list.
pub async fn add_flagged_plugin(plugin_id: &str) {
    let mut cache = {
        let guard = CACHE.lock().unwrap();
        guard.clone().unwrap_or_else(|| HashMap::new())
    };
    // If cache was None, read from disk
    if CACHE.lock().unwrap().is_none() {
        cache = read_from_disk().await;
    }

    cache.insert(
        plugin_id.to_string(),
        FlaggedPlugin {
            flagged_at: Utc::now().to_rfc3339(),
            seen_at: None,
        },
    );

    write_to_disk(&cache).await;
    debug!("Flagged plugin: {}", plugin_id);
}

/// Mark flagged plugins as seen. Sets seenAt on entries that don't already have it.
/// After 48 hours from seenAt, entries are auto-cleared on next load.
pub async fn mark_flagged_plugins_seen(plugin_ids: &[String]) {
    let mut cache = {
        let guard = CACHE.lock().unwrap();
        guard.clone().unwrap_or_default()
    };
    if CACHE.lock().unwrap().is_none() {
        cache = read_from_disk().await;
    }

    let now = Utc::now().to_rfc3339();
    let mut changed = false;

    for id in plugin_ids {
        if let Some(entry) = cache.get_mut(id) {
            if entry.seen_at.is_none() {
                entry.seen_at = Some(now.clone());
                changed = true;
            }
        }
    }

    if changed {
        write_to_disk(&cache).await;
    }
}

/// Remove a plugin from the flagged list.
pub async fn remove_flagged_plugin(plugin_id: &str) {
    let mut cache = {
        let guard = CACHE.lock().unwrap();
        guard.clone().unwrap_or_default()
    };
    if CACHE.lock().unwrap().is_none() {
        cache = read_from_disk().await;
    }

    if !cache.contains_key(plugin_id) {
        return;
    }

    cache.remove(plugin_id);
    {
        let mut guard = CACHE.lock().unwrap();
        *guard = Some(cache.clone());
    }
    write_to_disk(&cache).await;
}
