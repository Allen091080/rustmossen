//! Hosted rate limits management

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Quota status for rate limiting
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaStatus {
    Allowed,
    AllowedWarning,
    Rejected,
}

/// Rate limit type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RateLimitType {
    FiveHour,
    SevenDay,
    SevenDayMax,
    SevenDayBalanced,
    Overage,
}

/// Current rate limit state
#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub status: QuotaStatus,
    pub rate_limit_type: Option<RateLimitType>,
    pub remaining_tokens: Option<u64>,
    pub reset_at: Option<Instant>,
    pub warning_message: Option<String>,
    pub is_overage: bool,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            status: QuotaStatus::Allowed,
            rate_limit_type: None,
            remaining_tokens: None,
            reset_at: None,
            warning_message: None,
            is_overage: false,
        }
    }
}

static CURRENT_LIMITS: once_cell::sync::Lazy<RwLock<RateLimitState>> =
    once_cell::sync::Lazy::new(|| RwLock::new(RateLimitState::default()));

/// Get current rate limit state
pub fn get_current_limits() -> RateLimitState {
    CURRENT_LIMITS.read().clone()
}

/// Update rate limits from API response headers
pub fn update_rate_limits(
    status: QuotaStatus,
    rate_limit_type: Option<RateLimitType>,
    remaining: Option<u64>,
    reset_secs: Option<u64>,
) {
    let mut limits = CURRENT_LIMITS.write();
    limits.status = status;
    limits.rate_limit_type = rate_limit_type;
    limits.remaining_tokens = remaining;
    limits.reset_at = reset_secs.map(|s| Instant::now() + Duration::from_secs(s));
}

/// Check if current status allows requests
pub fn is_request_allowed() -> bool {
    let limits = CURRENT_LIMITS.read();
    limits.status != QuotaStatus::Rejected
}

/// Process rate limit headers from API response
pub fn process_rate_limit_response(
    status_code: u16,
    headers: &std::collections::HashMap<String, String>,
) {
    if status_code == 429 {
        let mut limits = CURRENT_LIMITS.write();
        limits.status = QuotaStatus::Rejected;

        if let Some(retry_after) = headers.get("retry-after") {
            if let Ok(secs) = retry_after.parse::<u64>() {
                limits.reset_at = Some(Instant::now() + Duration::from_secs(secs));
            }
        }
        warn!("Rate limited (429)");
    }
}

/// Reset rate limit state
pub fn reset_rate_limits() {
    let mut limits = CURRENT_LIMITS.write();
    *limits = RateLimitState::default();
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/hostedLimits.ts` exports.
// ---------------------------------------------------------------------------

use std::sync::Mutex;

/// `hostedLimits.ts` `RawUtilization` shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawUtilization {
    pub model_tier: String,
    pub utilization_percent: f64,
    pub overage_eligible: bool,
}

static UTILIZATION: once_cell::sync::Lazy<RwLock<RawUtilization>> =
    once_cell::sync::Lazy::new(|| RwLock::new(RawUtilization::default()));

/// `hostedLimits.ts` `getRawUtilization`.
pub fn get_raw_utilization() -> RawUtilization {
    UTILIZATION.read().clone()
}

/// Listener invoked when hosted-limit status changes.
pub type HostedStatusListener = Box<dyn Fn(&RawUtilization) + Send + Sync + 'static>;

static STATUS_LISTENERS: once_cell::sync::Lazy<Mutex<Vec<HostedStatusListener>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Vec::new()));

/// `hostedLimits.ts` `statusListeners` — register a callback for status
/// transitions.
pub fn add_status_listener(listener: HostedStatusListener) {
    STATUS_LISTENERS.lock().unwrap().push(listener);
}

/// `hostedLimits.ts` `emitStatusChange` — notify all subscribed listeners.
pub fn emit_status_change(util: &RawUtilization) {
    let listeners = STATUS_LISTENERS.lock().unwrap();
    for l in listeners.iter() {
        l(util);
    }
}

/// `hostedLimits.ts` `checkQuotaStatus` — refresh the cached status.
pub async fn check_quota_status() {
    let util = get_raw_utilization();
    emit_status_change(&util);
}

/// `hostedLimits.ts` `extractQuotaStatusFromHeaders`.
pub fn extract_quota_status_from_headers(headers: &std::collections::HashMap<String, String>) {
    let mut util = UTILIZATION.write();
    if let Some(v) = headers.get("provider-priority-input-tokens-utilization") {
        if let Ok(p) = v.parse::<f64>() {
            util.utilization_percent = p;
        }
    }
    if let Some(v) = headers.get("provider-priority-input-tokens-overage-eligible") {
        util.overage_eligible = matches!(v.as_str(), "true" | "1");
    }
    if let Some(v) = headers.get("provider-model-tier") {
        util.model_tier = v.clone();
    }
}

/// `hostedLimits.ts` `extractQuotaStatusFromError`.
pub fn extract_quota_status_from_error(error_body: &str) {
    if error_body.contains("\"type\":\"overloaded_error\"") {
        let mut limits = CURRENT_LIMITS.write();
        limits.status = QuotaStatus::Rejected;
    }
}

/// `hostedLimits.ts` `OverageDisabledReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverageDisabledReason {
    NotEligible,
    OptedOut,
    AdminDisabled,
    SubscriptionMissing,
}

/// `hostedLimits.ts` `getRateLimitDisplayName`.
pub fn get_rate_limit_display_name(rate_limit_type: &RateLimitType) -> &'static str {
    match rate_limit_type {
        RateLimitType::FiveHour => "5-hour rate limit",
        RateLimitType::SevenDay => "7-day rate limit",
        RateLimitType::SevenDayMax => "7-day Max rate limit",
        RateLimitType::SevenDayBalanced => "7-day Balanced rate limit",
        RateLimitType::Overage => "overage rate limit",
    }
}

/// `hostedLimits.ts` `HostedLimits` shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostedLimits {
    pub utilization: RawUtilization,
    pub status: String,
}
