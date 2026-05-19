//! Remote managed settings fetch service

use sha2::{Digest, Sha256};
use std::time::Duration;
use tracing::{debug, info, warn};

use super::sync_cache::*;
use super::types::*;

const SETTINGS_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_RETRIES: u32 = 5;
const POLLING_INTERVAL_MS: u64 = 60 * 60 * 1000; // 1 hour

/// Context for remote managed settings dependency injection
pub trait RemoteManagedSettingsContext: Send + Sync {
    fn get_auth_headers(&self) -> Result<std::collections::HashMap<String, String>, String>;
    fn get_endpoint_url(&self) -> String;
    fn is_eligible(&self) -> bool;
    fn apply_settings(&self, settings: &serde_json::Value) -> Result<(), String>;
}

/// Initialize the loading promise for remote managed settings
pub fn initialize_remote_managed_settings_loading() {
    if !is_remote_managed_settings_eligible() {
        return;
    }
    debug!("Remote managed settings: loading promise initialized");
}

/// Wait for remote managed settings to be loaded
pub async fn wait_for_remote_managed_settings(timeout_ms: u64) {
    // In full implementation, this would wait for the initial load to complete
    tokio::time::sleep(Duration::from_millis(timeout_ms.min(100))).await;
}

/// Fetch remote managed settings from the server
async fn fetch_settings_once(
    ctx: &dyn RemoteManagedSettingsContext,
    etag: Option<&str>,
) -> RemoteManagedSettingsFetchResult {
    let headers = match ctx.get_auth_headers() {
        Ok(h) => h,
        Err(e) => {
            return RemoteManagedSettingsFetchResult {
                error: Some(e),
                ..Default::default()
            };
        }
    };

    let endpoint = ctx.get_endpoint_url();
    let client = reqwest::Client::new();
    let mut req = client
        .get(&endpoint)
        .timeout(Duration::from_millis(SETTINGS_TIMEOUT_MS));

    for (k, v) in &headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if let Some(etag_val) = etag {
        req = req.header("If-None-Match", etag_val);
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return RemoteManagedSettingsFetchResult {
                error: Some(format!("Network error: {}", e)),
                ..Default::default()
            };
        }
    };

    let status = response.status().as_u16();

    if status == 304 {
        return RemoteManagedSettingsFetchResult {
            success: true,
            not_modified: true,
            ..Default::default()
        };
    }

    if status != 200 {
        return RemoteManagedSettingsFetchResult {
            error: Some(format!("HTTP {}", status)),
            ..Default::default()
        };
    }

    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            return RemoteManagedSettingsFetchResult {
                error: Some(format!("Read error: {}", e)),
                ..Default::default()
            };
        }
    };

    let parsed: RemoteManagedSettingsResponse = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(e) => {
            return RemoteManagedSettingsFetchResult {
                error: Some(format!("Parse error: {}", e)),
                ..Default::default()
            };
        }
    };

    // Validate checksum
    let computed_checksum = {
        let mut hasher = Sha256::new();
        hasher.update(parsed.settings.to_string().as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    };

    if !parsed.checksum.is_empty() && computed_checksum != parsed.checksum {
        warn!("Remote settings checksum mismatch");
    }

    RemoteManagedSettingsFetchResult {
        success: true,
        settings: Some(parsed.settings),
        checksum: Some(parsed.checksum),
        not_modified: false,
        error: None,
    }
}

/// Fetch with retry
async fn fetch_settings_with_retry(
    ctx: &dyn RemoteManagedSettingsContext,
    etag: Option<&str>,
) -> RemoteManagedSettingsFetchResult {
    let mut last_result = RemoteManagedSettingsFetchResult::default();

    for attempt in 0..DEFAULT_MAX_RETRIES {
        let result = fetch_settings_once(ctx, etag).await;
        if result.success {
            return result;
        }
        last_result = result;
        if attempt < DEFAULT_MAX_RETRIES - 1 {
            let delay = Duration::from_millis(1000 * 2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }
    }

    last_result
}

/// Load remote managed settings (called at startup)
pub async fn load_remote_managed_settings(ctx: &dyn RemoteManagedSettingsContext) {
    if !ctx.is_eligible() {
        return;
    }

    let etag = get_cached_checksum();
    let result = fetch_settings_with_retry(ctx, etag.as_deref()).await;

    if result.success {
        if result.not_modified {
            debug!("Remote managed settings: not modified");
            return;
        }
        if let (Some(settings), Some(checksum)) = (result.settings, result.checksum) {
            if let Err(e) = ctx.apply_settings(&settings) {
                warn!("Failed to apply remote settings: {}", e);
                return;
            }
            set_session_cache(settings, checksum);
            info!("Remote managed settings loaded successfully");
        }
    } else if let Some(err) = result.error {
        warn!("Failed to load remote managed settings: {}", err);
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/remoteManagedSettings/index.ts` additional exports.
// ---------------------------------------------------------------------------

/// `index.ts` `initializeRemoteManagedSettingsLoadingPromise`.
pub fn initialize_remote_managed_settings_loading_promise() {
    initialize_remote_managed_settings_loading();
}

/// `index.ts` `computeChecksumFromSettings` — SHA-256 hex digest of canonical
/// JSON encoding.
pub fn compute_checksum_from_settings(settings: &serde_json::Value) -> String {
    let s = serde_json::to_string(settings).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// `index.ts` `isEligibleForRemoteManagedSettings`.
pub fn is_eligible_for_remote_managed_settings() -> bool {
    matches!(
        std::env::var("MOSSEN_REMOTE_MANAGED_SETTINGS").as_deref(),
        Ok("1" | "true" | "TRUE")
    ) || std::env::var("MOSSEN_ORGANIZATION_UUID").is_ok()
}

/// `index.ts` `waitForRemoteManagedSettingsToLoad`.
pub async fn wait_for_remote_managed_settings_to_load() {
    wait_for_remote_managed_settings(15_000).await;
}

/// Start background polling for settings changes
pub fn start_settings_polling(ctx: std::sync::Arc<dyn RemoteManagedSettingsContext>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(POLLING_INTERVAL_MS));
        interval.tick().await; // Skip initial tick

        loop {
            interval.tick().await;
            let etag = get_cached_checksum();
            let result = fetch_settings_once(ctx.as_ref(), etag.as_deref()).await;

            if result.success && !result.not_modified {
                if let (Some(settings), Some(checksum)) = (result.settings, result.checksum) {
                    if let Err(e) = ctx.apply_settings(&settings) {
                        warn!("Failed to apply polled settings: {}", e);
                        continue;
                    }
                    set_session_cache(settings, checksum);
                    info!("Remote managed settings updated via polling");
                }
            }
        }
    });
}

