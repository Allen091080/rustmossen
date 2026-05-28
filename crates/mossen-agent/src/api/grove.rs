//! # Grove Privacy Settings API
//!
//! 翻译自 `services/api/grove.ts` (358行)
//! Grove 隐私设置 API：获取/更新设置、通知配置、资格检查。

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};

/// Cache expiration: 24 hours
const GROVE_CACHE_EXPIRATION_MS: u64 = 24 * 60 * 60 * 1000;

/// Account settings from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettings {
    pub grove_enabled: Option<bool>,
    pub grove_notice_viewed_at: Option<String>,
}

/// Grove configuration from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroveConfig {
    pub grove_enabled: bool,
    #[serde(default)]
    pub domain_excluded: bool,
    #[serde(default = "default_true")]
    pub notice_is_grace_period: bool,
    pub notice_reminder_frequency: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// Result type that distinguishes between API failure and success.
#[derive(Debug, Clone)]
pub enum ApiResult<T> {
    Success(T),
    Failure,
}

impl<T> ApiResult<T> {
    pub fn is_success(&self) -> bool {
        matches!(self, ApiResult::Success(_))
    }

    pub fn data(&self) -> Option<&T> {
        match self {
            ApiResult::Success(data) => Some(data),
            ApiResult::Failure => None,
        }
    }
}

/// Grove config cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroveConfigCacheEntry {
    pub grove_enabled: bool,
    pub timestamp: u64,
}

/// Grove config cache (account_id -> entry).
pub type GroveConfigCache = HashMap<String, GroveConfigCacheEntry>;

/// Get the current Grove settings for the user account.
pub async fn get_grove_settings(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    is_essential_traffic_only: bool,
) -> ApiResult<AccountSettings> {
    if is_essential_traffic_only {
        return ApiResult::Failure;
    }

    let url = format!("{}/api/oauth/account/settings", base_api_url);

    let mut request = client.get(&url).header("User-Agent", user_agent);

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    match request.send().await {
        Ok(resp) => match resp.json::<AccountSettings>().await {
            Ok(data) => ApiResult::Success(data),
            Err(e) => {
                error!("Failed to parse grove settings: {}", e);
                ApiResult::Failure
            }
        },
        Err(e) => {
            error!("Failed to fetch grove settings: {}", e);
            ApiResult::Failure
        }
    }
}

/// Mark that the Grove notice has been viewed by the user.
pub async fn mark_grove_notice_viewed(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
) {
    let url = format!("{}/api/oauth/account/grove_notice_viewed", base_api_url);

    let mut request = client
        .post(&url)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .body("{}");

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    if let Err(e) = request.send().await {
        error!("Failed to mark grove notice viewed: {}", e);
    }
}

/// Update Grove settings for the user account.
pub async fn update_grove_settings(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    grove_enabled: bool,
) {
    let url = format!("{}/api/oauth/account/settings", base_api_url);

    let body = serde_json::json!({ "grove_enabled": grove_enabled });

    let mut request = client
        .patch(&url)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .json(&body);

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    if let Err(e) = request.send().await {
        error!("Failed to update grove settings: {}", e);
    }
}

/// Check if user is qualified for Grove (non-blocking, cache-first).
pub fn is_qualified_for_grove(
    cache: &GroveConfigCache,
    account_id: Option<&str>,
    is_consumer_subscriber: bool,
    now_ms: u64,
) -> (bool, bool) {
    // Returns (qualified, needs_background_fetch)
    if !is_consumer_subscriber {
        return (false, false);
    }

    let account_id = match account_id {
        Some(id) => id,
        None => return (false, false),
    };

    match cache.get(account_id) {
        None => {
            debug!("Grove: No cache, fetching config in background (dialog skipped this session)");
            (false, true)
        }
        Some(entry) => {
            if now_ms - entry.timestamp > GROVE_CACHE_EXPIRATION_MS {
                debug!("Grove: Cache stale, returning cached data and refreshing in background");
                (entry.grove_enabled, true)
            } else {
                debug!("Grove: Using fresh cached config");
                (entry.grove_enabled, false)
            }
        }
    }
}

/// Fetch Grove config from API and store in cache.
pub async fn fetch_and_store_grove_config(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    cache: &mut GroveConfigCache,
    account_id: &str,
    is_essential_traffic_only: bool,
    now_ms: u64,
) {
    let result = get_grove_notice_config(
        client,
        base_api_url,
        auth_headers,
        user_agent,
        is_essential_traffic_only,
    )
    .await;

    if let ApiResult::Success(config) = result {
        let grove_enabled = config.grove_enabled;

        // Check if cache is still fresh and unchanged
        if let Some(cached) = cache.get(account_id) {
            if cached.grove_enabled == grove_enabled
                && now_ms - cached.timestamp <= GROVE_CACHE_EXPIRATION_MS
            {
                return;
            }
        }

        cache.insert(
            account_id.to_string(),
            GroveConfigCacheEntry {
                grove_enabled,
                timestamp: now_ms,
            },
        );
    } else {
        debug!("Grove: Failed to fetch and store config");
    }
}

/// Get Grove notice configuration from the API.
pub async fn get_grove_notice_config(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    is_essential_traffic_only: bool,
) -> ApiResult<GroveConfig> {
    if is_essential_traffic_only {
        return ApiResult::Failure;
    }

    let url = format!("{}/api/mossen/grove", base_api_url);

    let mut request = client
        .get(&url)
        .header("User-Agent", user_agent)
        .timeout(std::time::Duration::from_secs(3));

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    match request.send().await {
        Ok(resp) => match resp.json::<GroveConfig>().await {
            Ok(data) => ApiResult::Success(GroveConfig {
                grove_enabled: data.grove_enabled,
                domain_excluded: data.domain_excluded,
                notice_is_grace_period: data.notice_is_grace_period,
                notice_reminder_frequency: data.notice_reminder_frequency,
            }),
            Err(e) => {
                debug!("Failed to fetch Grove notice config: {}", e);
                ApiResult::Failure
            }
        },
        Err(e) => {
            debug!("Failed to fetch Grove notice config: {}", e);
            ApiResult::Failure
        }
    }
}

/// Determines whether the Grove dialog should be shown.
/// Returns false if either API call failed (after retry).
pub fn calculate_should_show_grove(
    settings_result: &ApiResult<AccountSettings>,
    config_result: &ApiResult<GroveConfig>,
    show_if_already_viewed: bool,
    now_ms: u64,
) -> bool {
    let settings = match settings_result {
        ApiResult::Success(s) => s,
        ApiResult::Failure => return false,
    };
    let config = match config_result {
        ApiResult::Success(c) => c,
        ApiResult::Failure => return false,
    };

    let has_chosen = settings.grove_enabled.is_some();
    if has_chosen {
        return false;
    }
    if show_if_already_viewed {
        return true;
    }
    if !config.notice_is_grace_period {
        return true;
    }

    // Check if we need to remind the user
    let reminder_frequency = config.notice_reminder_frequency;
    if let (Some(frequency), Some(ref viewed_at)) =
        (reminder_frequency, &settings.grove_notice_viewed_at)
    {
        if let Ok(viewed_time) = chrono::DateTime::parse_from_rfc3339(viewed_at) {
            let days_since_viewed =
                (now_ms as i64 - viewed_time.timestamp_millis()) / (1000 * 60 * 60 * 24);
            return days_since_viewed >= frequency as i64;
        }
        return true;
    }

    // Show if never viewed before
    settings.grove_notice_viewed_at.is_none()
}

/// TS `checkGroveForNonInteractive` — issues the Grove eligibility lookup but
/// suppresses any interactive prompts. Returns whether the user is qualified
/// (cache-first; never fetches synchronously).
pub fn check_grove_for_non_interactive(
    cache: &GroveConfigCache,
    account_id: Option<&str>,
    is_consumer_subscriber: bool,
    now_ms: u64,
) -> bool {
    let (qualified, _needs_fetch) =
        is_qualified_for_grove(cache, account_id, is_consumer_subscriber, now_ms);
    qualified
}
