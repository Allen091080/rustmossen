//! Remote managed settings — syncs organization-managed settings from the server.

pub mod types;
pub mod sync_cache;
pub mod sync_cache_state;
pub mod security_check;

use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use self::types::*;

/// Remote managed settings service.
pub struct RemoteManagedSettingsService {
    cache: RwLock<SyncCacheData>,
    config: RemoteManagedConfig,
    last_fetch: RwLock<Option<Instant>>,
}

/// Configuration for remote managed settings.
#[derive(Debug, Clone)]
pub struct RemoteManagedConfig {
    pub base_url: String,
    pub poll_interval: Duration,
    pub timeout: Duration,
    pub enabled: bool,
}

impl Default for RemoteManagedConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.mossen.ai".to_string(),
            poll_interval: Duration::from_secs(300),
            timeout: Duration::from_secs(10),
            enabled: true,
        }
    }
}

/// Cached sync data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncCacheData {
    pub settings: HashMap<String, serde_json::Value>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub version: Option<u64>,
}

impl RemoteManagedSettingsService {
    pub fn new(config: RemoteManagedConfig) -> Self {
        Self {
            cache: RwLock::new(SyncCacheData::default()),
            config,
            last_fetch: RwLock::new(None),
        }
    }

    /// Fetch remote settings from the server.
    pub async fn fetch(&self) -> Result<SyncCacheData, String> {
        if !self.config.enabled {
            return Ok(SyncCacheData::default());
        }

        let client = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .build()
            .map_err(|e| e.to_string())?;

        let cache = self.cache.read().await;
        let mut req = client.get(format!("{}/api/managed-settings", self.config.base_url));
        if let Some(ref etag) = cache.etag {
            req = req.header("If-None-Match", etag.as_str());
        }
        drop(cache);

        let response = req.send().await.map_err(|e| e.to_string())?;
        let status = response.status().as_u16();

        if status == 304 {
            debug!("remote-managed-settings: not modified");
            return Ok(self.cache.read().await.clone());
        }

        if status != 200 {
            return Err(format!("HTTP {}", status));
        }

        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        let settings: HashMap<String, serde_json::Value> = body
            .get("settings")
            .and_then(|s| serde_json::from_value(s.clone()).ok())
            .unwrap_or_default();

        let data = SyncCacheData {
            settings,
            etag,
            last_modified: body.get("lastModified").and_then(|v| v.as_str()).map(String::from),
            version: body.get("version").and_then(|v| v.as_u64()),
        };

        let mut cache = self.cache.write().await;
        *cache = data.clone();
        let mut last_fetch = self.last_fetch.write().await;
        *last_fetch = Some(Instant::now());

        Ok(data)
    }

    /// Get a setting value from the cache.
    pub async fn get_setting(&self, key: &str) -> Option<serde_json::Value> {
        let cache = self.cache.read().await;
        cache.settings.get(key).cloned()
    }

    /// Get all cached settings.
    pub async fn get_all_settings(&self) -> HashMap<String, serde_json::Value> {
        self.cache.read().await.settings.clone()
    }

    /// Check if the cache needs refresh.
    pub async fn needs_refresh(&self) -> bool {
        let last = self.last_fetch.read().await;
        match *last {
            Some(t) => t.elapsed() >= self.config.poll_interval,
            None => true,
        }
    }

    /// Start the background polling loop.
    pub async fn start_polling(self: std::sync::Arc<Self>) {
        let service = self;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(service.config.poll_interval).await;
                if let Err(e) = service.fetch().await {
                    warn!("remote-managed-settings: fetch failed: {}", e);
                }
            }
        });
    }
}

/// TS `index.ts` `clearRemoteManagedSettingsCache` — wipes session cache and
/// deletes the persisted settings file on disk (ENOENT errors are swallowed).
pub async fn clear_remote_managed_settings_cache() {
    sync_cache_state::reset_sync_cache();
    if let Some(path) = get_settings_path() {
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// TS `index.ts` `refreshRemoteManagedSettings` — clears caches then triggers
/// a fresh fetch. Fails open: a fetch error continues without remote settings.
pub async fn refresh_remote_managed_settings() {
    clear_remote_managed_settings_cache().await;
    if !is_remote_managed_settings_eligible() {
        return;
    }
    // The caller wires the actual `load_remote_managed_settings` invocation via
    // the agent runtime once the eligibility gate above passes.
}

/// TS gating predicate used by callers external to this module.
pub fn is_remote_managed_settings_eligible() -> bool {
    matches!(
        std::env::var("MOSSEN_REMOTE_MANAGED_SETTINGS").as_deref(),
        Ok("1" | "true" | "TRUE")
    ) || std::env::var("MOSSEN_ORGANIZATION_UUID").is_ok()
}

fn get_settings_path() -> Option<std::path::PathBuf> {
    std::env::var("MOSSEN_SETTINGS_DIR")
        .ok()
        .map(|d| std::path::PathBuf::from(d).join("remote-managed-settings.json"))
        .or_else(|| {
            dirs::home_dir().map(|h| h.join(".mossen").join("remote-managed-settings.json"))
        })
}
