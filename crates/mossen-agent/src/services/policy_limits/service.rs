//! Policy limits service implementation.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::types::{PolicyLimitsFetchResult, PolicyLimitsResponse, PolicyRestriction};

const CACHE_FILENAME: &str = "policy-limits.json";
const FETCH_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_RETRIES: u32 = 5;
const POLLING_INTERVAL_MS: u64 = 60 * 60 * 1000; // 1 hour
const LOADING_PROMISE_TIMEOUT_MS: u64 = 30_000;

/// Policies that fail closed when essential-traffic-only mode is active.
static ESSENTIAL_TRAFFIC_DENY_ON_MISS: Lazy<HashSet<&'static str>> =
    Lazy::new(|| {
        let mut s = HashSet::new();
        s.insert("allow_product_feedback");
        s
    });

struct PolicyLimitsState {
    session_cache: Option<HashMap<String, PolicyRestriction>>,
    polling_handle: Option<JoinHandle<()>>,
    loading_notify: Option<Arc<Notify>>,
    loading_complete: bool,
}

static STATE: Lazy<Mutex<PolicyLimitsState>> = Lazy::new(|| {
    Mutex::new(PolicyLimitsState {
        session_cache: None,
        polling_handle: None,
        loading_notify: None,
        loading_complete: false,
    })
});

/// Trait for external dependencies (auth, API provider checks).
pub trait PolicyLimitsContext: Send + Sync {
    fn get_api_provider(&self) -> String;
    fn is_first_party_base_url(&self) -> bool;
    fn get_api_key(&self) -> Option<String>;
    fn get_oauth_access_token(&self) -> Option<String>;
    fn get_oauth_scopes(&self) -> Vec<String>;
    fn get_subscription_type(&self) -> Option<String>;
    fn is_essential_traffic_only(&self) -> bool;
    fn get_config_home_dir(&self) -> PathBuf;
    fn get_base_api_url(&self) -> String;
    fn get_user_agent(&self) -> String;
}

static CONTEXT: Lazy<Mutex<Option<Arc<dyn PolicyLimitsContext>>>> =
    Lazy::new(|| Mutex::new(None));

/// Set the policy limits context (call during initialization).
pub fn set_policy_limits_context(ctx: Arc<dyn PolicyLimitsContext>) {
    *CONTEXT.lock() = Some(ctx);
}

fn get_context() -> Option<Arc<dyn PolicyLimitsContext>> {
    CONTEXT.lock().clone()
}

fn get_cache_path() -> PathBuf {
    let ctx = get_context();
    let config_dir = ctx
        .map(|c| c.get_config_home_dir())
        .unwrap_or_else(|| PathBuf::from("."));
    config_dir.join(CACHE_FILENAME)
}

/// Check if the current user is eligible for policy limits.
pub fn is_policy_limits_eligible() -> bool {
    let ctx = match get_context() {
        Some(c) => c,
        None => return false,
    };

    if ctx.get_api_provider() != "firstParty" {
        return false;
    }
    if !ctx.is_first_party_base_url() {
        return false;
    }

    if ctx.get_api_key().is_some() {
        return true;
    }

    let token = ctx.get_oauth_access_token();
    if token.is_none() {
        return false;
    }

    let scopes = ctx.get_oauth_scopes();
    if !scopes.iter().any(|s| s == "user:inference") {
        return false;
    }

    let sub_type = ctx.get_subscription_type().unwrap_or_default();
    sub_type == "enterprise" || sub_type == "team"
}

/// Initialize the loading promise for policy limits.
pub fn initialize_policy_limits_loading_promise() {
    let mut state = STATE.lock();
    if state.loading_notify.is_some() {
        return;
    }
    if is_policy_limits_eligible() {
        let notify = Arc::new(Notify::new());
        state.loading_notify = Some(notify.clone());

        // Timeout to prevent deadlocks
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(LOADING_PROMISE_TIMEOUT_MS)).await;
            notify.notify_waiters();
        });
    }
}

/// Wait for initial policy limits loading to complete.
pub async fn wait_for_policy_limits_to_load() {
    let notify = {
        let state = STATE.lock();
        if state.loading_complete {
            return;
        }
        state.loading_notify.clone()
    };
    if let Some(n) = notify {
        n.notified().await;
    }
}

/// Compute SHA-256 checksum of restrictions for ETag.
fn compute_checksum(restrictions: &HashMap<String, PolicyRestriction>) -> String {
    let sorted: std::collections::BTreeMap<&String, &PolicyRestriction> =
        restrictions.iter().collect();
    let normalized = serde_json::to_string(&sorted).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

/// Load cached restrictions from file.
fn load_cached_restrictions() -> Option<HashMap<String, PolicyRestriction>> {
    let path = get_cache_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let response: PolicyLimitsResponse = serde_json::from_str(&content).ok()?;
    Some(response.restrictions)
}

/// Save restrictions to cache file.
async fn save_cached_restrictions(
    restrictions: &HashMap<String, PolicyRestriction>,
) -> Result<(), std::io::Error> {
    let path = get_cache_path();
    let data = PolicyLimitsResponse {
        restrictions: restrictions.clone(),
    };
    let json_str = serde_json::to_string_pretty(&data).unwrap_or_default();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, json_str).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await;
    }
    Ok(())
}

/// Compute retry delay with exponential backoff.
fn get_retry_delay(attempt: u32) -> Duration {
    let base_ms = 1000u64;
    let delay = base_ms * 2u64.pow(attempt.saturating_sub(1));
    Duration::from_millis(delay.min(30_000))
}

/// Fetch policy limits from the API (single attempt).
async fn fetch_policy_limits_once(
    cached_checksum: Option<&str>,
) -> PolicyLimitsFetchResult {
    let ctx = match get_context() {
        Some(c) => c,
        None => return PolicyLimitsFetchResult::failure("No context".to_string(), true),
    };

    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&ctx.get_user_agent()) {
        headers.insert("User-Agent", v);
    }

    // Auth headers
    if let Some(api_key) = ctx.get_api_key() {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&api_key) {
            headers.insert("x-api-key", v);
        }
    } else if let Some(token) = ctx.get_oauth_access_token() {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)) {
            headers.insert("Authorization", v);
        }
    } else {
        return PolicyLimitsFetchResult::failure(
            "Authentication required for policy limits".to_string(),
            true,
        );
    }

    if let Some(checksum) = cached_checksum {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("\"{}\"", checksum)) {
            headers.insert("If-None-Match", v);
        }
    }

    let endpoint = format!("{}/api/mossen/policy_limits", ctx.get_base_api_url());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(FETCH_TIMEOUT_MS))
        .build()
        .unwrap_or_default();

    match client.get(&endpoint).headers(headers).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            match status {
                304 => PolicyLimitsFetchResult::success(None, cached_checksum.map(String::from)),
                404 => PolicyLimitsFetchResult::success(Some(HashMap::new()), None),
                200 => {
                    let body = response.text().await.unwrap_or_default();
                    match serde_json::from_str::<PolicyLimitsResponse>(&body) {
                        Ok(parsed) => {
                            PolicyLimitsFetchResult::success(Some(parsed.restrictions), None)
                        }
                        Err(_) => PolicyLimitsFetchResult::failure(
                            "Invalid policy limits format".to_string(),
                            false,
                        ),
                    }
                }
                401 | 403 => PolicyLimitsFetchResult::failure(
                    "Not authorized for policy limits".to_string(),
                    true,
                ),
                _ => PolicyLimitsFetchResult::failure(
                    format!("Unexpected status: {}", status),
                    false,
                ),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                PolicyLimitsFetchResult::failure("Policy limits request timeout".to_string(), false)
            } else if e.is_connect() {
                PolicyLimitsFetchResult::failure("Cannot connect to server".to_string(), false)
            } else {
                PolicyLimitsFetchResult::failure(e.to_string(), false)
            }
        }
    }
}

/// Fetch with retry logic and exponential backoff.
async fn fetch_with_retry(
    cached_checksum: Option<&str>,
) -> PolicyLimitsFetchResult {
    let mut last_result = PolicyLimitsFetchResult::failure("No attempts".to_string(), false);

    for attempt in 1..=(DEFAULT_MAX_RETRIES + 1) {
        last_result = fetch_policy_limits_once(cached_checksum).await;

        if last_result.success || last_result.skip_retry {
            return last_result;
        }

        if attempt > DEFAULT_MAX_RETRIES {
            return last_result;
        }

        let delay = get_retry_delay(attempt);
        tracing::debug!(
            "Policy limits: Retry {}/{} after {:?}",
            attempt,
            DEFAULT_MAX_RETRIES,
            delay
        );
        tokio::time::sleep(delay).await;
    }

    last_result
}

/// Fetch and load policy limits with file caching.
async fn fetch_and_load_policy_limits() -> Option<HashMap<String, PolicyRestriction>> {
    if !is_policy_limits_eligible() {
        return None;
    }

    let cached_restrictions = load_cached_restrictions();
    let cached_checksum = cached_restrictions.as_ref().map(|r| compute_checksum(r));

    let result = fetch_with_retry(cached_checksum.as_deref()).await;

    if !result.success {
        if let Some(cached) = cached_restrictions {
            tracing::debug!("Policy limits: Using stale cache after fetch failure");
            STATE.lock().session_cache = Some(cached.clone());
            return Some(cached);
        }
        return None;
    }

    // 304 Not Modified
    if result.restrictions.is_none() {
        if let Some(cached) = cached_restrictions {
            STATE.lock().session_cache = Some(cached.clone());
            return Some(cached);
        }
    }

    let new_restrictions = result.restrictions.unwrap_or_default();
    let has_content = !new_restrictions.is_empty();

    if has_content {
        STATE.lock().session_cache = Some(new_restrictions.clone());
        let _ = save_cached_restrictions(&new_restrictions).await;
        return Some(new_restrictions);
    }

    // Empty restrictions (404) — delete cache file
    STATE.lock().session_cache = Some(new_restrictions.clone());
    let _ = tokio::fs::remove_file(get_cache_path()).await;
    Some(new_restrictions)
}

/// Get restrictions from session cache or file.
fn get_restrictions_from_cache() -> Option<HashMap<String, PolicyRestriction>> {
    if !is_policy_limits_eligible() {
        return None;
    }

    let state = STATE.lock();
    if let Some(ref cache) = state.session_cache {
        return Some(cache.clone());
    }
    drop(state);

    let cached = load_cached_restrictions()?;
    STATE.lock().session_cache = Some(cached.clone());
    Some(cached)
}

/// Check if a specific policy is allowed (fail-open).
pub fn is_policy_allowed(policy: &str) -> bool {
    let restrictions = get_restrictions_from_cache();
    match restrictions {
        None => {
            let ctx = get_context();
            let is_essential = ctx.map(|c| c.is_essential_traffic_only()).unwrap_or(false);
            if is_essential && ESSENTIAL_TRAFFIC_DENY_ON_MISS.contains(policy) {
                return false;
            }
            true // fail open
        }
        Some(ref r) => match r.get(policy) {
            None => true, // unknown policy = allowed
            Some(restriction) => restriction.allowed,
        },
    }
}

/// Load policy limits during CLI initialization.
pub async fn load_policy_limits() {
    if is_policy_limits_eligible() {
        let mut state = STATE.lock();
        if state.loading_notify.is_none() {
            state.loading_notify = Some(Arc::new(Notify::new()));
        }
        drop(state);
    }

    fetch_and_load_policy_limits().await;

    if is_policy_limits_eligible() {
        start_background_polling();
    }

    let mut state = STATE.lock();
    state.loading_complete = true;
    if let Some(notify) = state.loading_notify.take() {
        notify.notify_waiters();
    }
}

/// Refresh policy limits (for auth state changes).
pub async fn refresh_policy_limits() {
    clear_policy_limits_cache().await;
    if !is_policy_limits_eligible() {
        return;
    }
    fetch_and_load_policy_limits().await;
}

/// Clear all policy limits (session, persistent, stop polling).
pub async fn clear_policy_limits_cache() {
    stop_background_polling();
    {
        let mut state = STATE.lock();
        state.session_cache = None;
        state.loading_notify = None;
        state.loading_complete = false;
    }
    let _ = tokio::fs::remove_file(get_cache_path()).await;
}

/// Start background polling for policy limits.
pub fn start_background_polling() {
    let mut state = STATE.lock();
    if state.polling_handle.is_some() {
        return;
    }
    if !is_policy_limits_eligible() {
        return;
    }

    let handle = tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_millis(POLLING_INTERVAL_MS)).await;
            if !is_policy_limits_eligible() {
                return;
            }
            let _ = fetch_and_load_policy_limits().await;
        }
    });
    state.polling_handle = Some(handle);
}

/// Stop background polling.
pub fn stop_background_polling() {
    let mut state = STATE.lock();
    if let Some(handle) = state.polling_handle.take() {
        handle.abort();
    }
}

/// Test-only reset.
pub fn reset_policy_limits_for_testing() {
    stop_background_polling();
    let mut state = STATE.lock();
    state.session_cache = None;
    state.loading_notify = None;
    state.loading_complete = false;
}
