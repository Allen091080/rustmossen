//! Sync cache — persistent on-disk cache for remote managed settings.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// On-disk sync cache state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncCache {
    pub settings: HashMap<String, serde_json::Value>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub version: Option<u64>,
    pub fetched_at: Option<u64>,
}

impl SyncCache {
    /// Load from disk.
    pub async fn load(path: &PathBuf) -> Self {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to disk.
    pub async fn save(&self, path: &PathBuf) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize sync cache: {}", e))?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        }
        tokio::fs::write(path, json)
            .await
            .map_err(|e| format!("Failed to write sync cache: {}", e))?;
        Ok(())
    }

    /// Check if the cache is stale based on a TTL.
    pub fn is_stale(&self, ttl_seconds: u64) -> bool {
        match self.fetched_at {
            Some(ts) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now - ts > ttl_seconds
            }
            None => true,
        }
    }
}

/// TS `isRemoteManagedSettingsEligible` — re-export of the module-level
/// gating predicate so consumers using `sync_cache::is_remote_managed_settings_eligible`
/// keep working alongside the TS-mirror name on the parent module.
pub fn is_remote_managed_settings_eligible() -> bool {
    super::is_remote_managed_settings_eligible()
}
