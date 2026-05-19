/// Mock rate limits for internal testing.
/// Allows testing various rate limit scenarios without hitting actual limits.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use once_cell::sync::Lazy;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone)]
struct ExceededLimit {
    limit_type: String,
    resets_at: u64,
}

#[derive(Debug, Default)]
struct MockState {
    headers: HashMap<String, String>,
    enabled: bool,
    headerless_429_message: Option<String>,
    subscription_type: Option<String>,
    fast_mode_duration_ms: Option<u64>,
    fast_mode_expires_at: Option<u64>,
    exceeded_limits: Vec<ExceededLimit>,
}

static STATE: Lazy<Mutex<MockState>> = Lazy::new(|| Mutex::new(MockState::default()));

const DEFAULT_MOCK_SUBSCRIPTION: &str = "max";

fn get_mock_user_type() -> String {
    std::env::var("USER_TYPE").unwrap_or_else(|_| "external".to_string())
}

fn is_ant() -> bool {
    get_mock_user_type() == "ant"
}

pub fn set_mock_header(key: &str, value: Option<&str>) {
    if !is_ant() {
        return;
    }

    let mut state = STATE.lock();
    state.enabled = true;

    let full_key = if key == "retry-after" {
        "retry-after".to_string()
    } else {
        format!("mossen-ratelimit-unified-{}", key)
    };

    match value {
        None | Some("clear") => {
            state.headers.remove(&full_key);
            if key == "claim" {
                state.exceeded_limits.clear();
            }
            if key == "status" || key == "overage-status" {
                update_retry_after(&mut state);
            }
        }
        Some(val) => {
            let mut final_value = val.to_string();

            if key == "reset" || key == "overage-reset" {
                if let Ok(hours) = val.parse::<f64>() {
                    final_value = (now_secs() + (hours * 3600.0) as u64).to_string();
                }
            }

            if key == "claim" {
                let valid_claims = ["five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet"];
                if valid_claims.contains(&val) {
                    let resets_at = match val {
                        "five_hour" => now_secs() + 5 * 3600,
                        "seven_day" | "seven_day_opus" | "seven_day_sonnet" => {
                            now_secs() + 7 * 24 * 3600
                        }
                        _ => now_secs() + 3600,
                    };
                    state.exceeded_limits.retain(|l| l.limit_type != val);
                    state.exceeded_limits.push(ExceededLimit {
                        limit_type: val.to_string(),
                        resets_at,
                    });
                    update_representative_claim(&mut state);
                    return;
                }
            }

            state.headers.insert(full_key, final_value);
            if key == "status" || key == "overage-status" {
                update_retry_after(&mut state);
            }
        }
    }

    if state.headers.is_empty() {
        state.enabled = false;
    }
}

fn update_retry_after(state: &mut MockState) {
    let status = state.headers.get("mossen-ratelimit-unified-status").cloned();
    let overage = state.headers.get("mossen-ratelimit-unified-overage-status").cloned();
    let reset = state.headers.get("mossen-ratelimit-unified-reset").cloned();

    if status.as_deref() == Some("rejected")
        && (overage.is_none() || overage.as_deref() == Some("rejected"))
    {
        if let Some(r) = reset {
            if let Ok(ts) = r.parse::<u64>() {
                let secs = ts.saturating_sub(now_secs());
                state.headers.insert("retry-after".to_string(), secs.to_string());
                return;
            }
        }
    }
    state.headers.remove("retry-after");
}

fn update_representative_claim(state: &mut MockState) {
    if state.exceeded_limits.is_empty() {
        state.headers.remove("mossen-ratelimit-unified-representative-claim");
        state.headers.remove("mossen-ratelimit-unified-reset");
        state.headers.remove("retry-after");
        return;
    }

    let furthest = state
        .exceeded_limits
        .iter()
        .max_by_key(|l| l.resets_at)
        .unwrap()
        .clone();

    state.headers.insert(
        "mossen-ratelimit-unified-representative-claim".to_string(),
        furthest.limit_type.clone(),
    );
    state.headers.insert(
        "mossen-ratelimit-unified-reset".to_string(),
        furthest.resets_at.to_string(),
    );

    if state.headers.get("mossen-ratelimit-unified-status").map(|s| s.as_str()) == Some("rejected") {
        let overage = state.headers.get("mossen-ratelimit-unified-overage-status");
        if overage.is_none() || overage.map(|s| s.as_str()) == Some("rejected") {
            let secs = furthest.resets_at.saturating_sub(now_secs());
            state.headers.insert("retry-after".to_string(), secs.to_string());
        } else {
            state.headers.remove("retry-after");
        }
    } else {
        state.headers.remove("retry-after");
    }
}

pub fn add_exceeded_limit(limit_type: &str, hours_from_now: f64) {
    if !is_ant() {
        return;
    }
    let mut state = STATE.lock();
    state.enabled = true;
    let resets_at = now_secs() + (hours_from_now * 3600.0) as u64;
    state.exceeded_limits.retain(|l| l.limit_type != limit_type);
    state.exceeded_limits.push(ExceededLimit {
        limit_type: limit_type.to_string(),
        resets_at,
    });
    if !state.exceeded_limits.is_empty() {
        state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
    }
    update_representative_claim(&mut state);
}

pub fn set_mock_early_warning(claim_abbrev: &str, utilization: f64, hours_from_now: Option<f64>) {
    if !is_ant() {
        return;
    }
    let mut state = STATE.lock();
    state.enabled = true;

    // Clear all early warning headers first
    clear_early_warning_headers(&mut state);

    let default_hours: f64 = if claim_abbrev == "5h" { 4.0 } else { 120.0 };
    let hours = hours_from_now.unwrap_or(default_hours);
    let resets_at = now_secs() + (hours * 3600.0) as u64;

    state.headers.insert(
        format!("mossen-ratelimit-unified-{}-utilization", claim_abbrev),
        utilization.to_string(),
    );
    state.headers.insert(
        format!("mossen-ratelimit-unified-{}-reset", claim_abbrev),
        resets_at.to_string(),
    );
    state.headers.insert(
        format!("mossen-ratelimit-unified-{}-surpassed-threshold", claim_abbrev),
        utilization.to_string(),
    );

    if !state.headers.contains_key("mossen-ratelimit-unified-status") {
        state.headers.insert("mossen-ratelimit-unified-status".to_string(), "allowed".to_string());
    }
}

fn clear_early_warning_headers(state: &mut MockState) {
    let keys_to_remove: Vec<String> = state
        .headers
        .keys()
        .filter(|k| {
            k.contains("-5h-") || k.contains("-7d-") || k.contains("-overage-utilization")
                || k.contains("-overage-surpassed")
        })
        .cloned()
        .collect();
    for k in keys_to_remove {
        state.headers.remove(&k);
    }
}

pub fn set_mock_rate_limit_scenario(scenario: &str) {
    if !is_ant() {
        return;
    }
    let mut state = STATE.lock();

    if scenario == "clear" {
        state.headers.clear();
        state.headerless_429_message = None;
        state.enabled = false;
        state.exceeded_limits.clear();
        return;
    }

    state.enabled = true;
    let five_hours = now_secs() + 5 * 3600;
    let seven_days = now_secs() + 7 * 24 * 3600;

    state.headers.clear();
    state.headerless_429_message = None;

    let preserve = matches!(scenario, "overage-active" | "overage-warning" | "overage-exhausted");
    if !preserve {
        state.exceeded_limits.clear();
    }

    match scenario {
        "normal" => {
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "allowed".to_string());
            state.headers.insert("mossen-ratelimit-unified-reset".to_string(), five_hours.to_string());
        }
        "session-limit-reached" => {
            state.exceeded_limits = vec![ExceededLimit { limit_type: "five_hour".to_string(), resets_at: five_hours }];
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
        }
        "approaching-weekly-limit" => {
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "allowed_warning".to_string());
            state.headers.insert("mossen-ratelimit-unified-reset".to_string(), seven_days.to_string());
            state.headers.insert("mossen-ratelimit-unified-representative-claim".to_string(), "seven_day".to_string());
        }
        "weekly-limit-reached" => {
            state.exceeded_limits = vec![ExceededLimit { limit_type: "seven_day".to_string(), resets_at: seven_days }];
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
        }
        "overage-active" => {
            if state.exceeded_limits.is_empty() {
                state.exceeded_limits = vec![ExceededLimit { limit_type: "five_hour".to_string(), resets_at: five_hours }];
            }
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
            state.headers.insert("mossen-ratelimit-unified-overage-status".to_string(), "allowed".to_string());
            let end_of_month = end_of_month_secs();
            state.headers.insert("mossen-ratelimit-unified-overage-reset".to_string(), end_of_month.to_string());
        }
        "overage-warning" => {
            if state.exceeded_limits.is_empty() {
                state.exceeded_limits = vec![ExceededLimit { limit_type: "five_hour".to_string(), resets_at: five_hours }];
            }
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
            state.headers.insert("mossen-ratelimit-unified-overage-status".to_string(), "allowed_warning".to_string());
            let end_of_month = end_of_month_secs();
            state.headers.insert("mossen-ratelimit-unified-overage-reset".to_string(), end_of_month.to_string());
        }
        "overage-exhausted" | "out-of-credits" | "org-zero-credit-limit"
        | "org-spend-cap-hit" | "member-zero-credit-limit" | "seat-tier-zero-credit-limit" => {
            if state.exceeded_limits.is_empty() {
                state.exceeded_limits = vec![ExceededLimit { limit_type: "five_hour".to_string(), resets_at: five_hours }];
            }
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
            state.headers.insert("mossen-ratelimit-unified-overage-status".to_string(), "rejected".to_string());
            let end_of_month = end_of_month_secs();
            state.headers.insert("mossen-ratelimit-unified-overage-reset".to_string(), end_of_month.to_string());
            // Set disabled reason for specific scenarios
            let reason = match scenario {
                "out-of-credits" => Some("out_of_credits"),
                "org-zero-credit-limit" => Some("org_service_zero_credit_limit"),
                "org-spend-cap-hit" => Some("org_level_disabled_until"),
                "member-zero-credit-limit" => Some("member_zero_credit_limit"),
                "seat-tier-zero-credit-limit" => Some("seat_tier_zero_credit_limit"),
                _ => None,
            };
            if let Some(r) = reason {
                state.headers.insert("mossen-ratelimit-unified-overage-disabled-reason".to_string(), r.to_string());
            }
        }
        "opus-limit" => {
            state.exceeded_limits = vec![ExceededLimit { limit_type: "seven_day_opus".to_string(), resets_at: seven_days }];
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
        }
        "opus-warning" => {
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "allowed_warning".to_string());
            state.headers.insert("mossen-ratelimit-unified-reset".to_string(), seven_days.to_string());
            state.headers.insert("mossen-ratelimit-unified-representative-claim".to_string(), "seven_day_opus".to_string());
        }
        "sonnet-limit" => {
            state.exceeded_limits = vec![ExceededLimit { limit_type: "seven_day_sonnet".to_string(), resets_at: seven_days }];
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
        }
        "sonnet-warning" => {
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "allowed_warning".to_string());
            state.headers.insert("mossen-ratelimit-unified-reset".to_string(), seven_days.to_string());
            state.headers.insert("mossen-ratelimit-unified-representative-claim".to_string(), "seven_day_sonnet".to_string());
        }
        "fast-mode-limit" => {
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
            state.fast_mode_duration_ms = Some(10 * 60 * 1000);
        }
        "fast-mode-short-limit" => {
            update_representative_claim(&mut state);
            state.headers.insert("mossen-ratelimit-unified-status".to_string(), "rejected".to_string());
            state.fast_mode_duration_ms = Some(10 * 1000);
        }
        "extra-usage-required" => {
            state.headerless_429_message = Some("Extra usage is required for long context requests.".to_string());
        }
        _ => {}
    }
}

fn end_of_month_secs() -> u64 {
    // Approximate: add 30 days
    now_secs() + 30 * 24 * 3600
}

pub fn get_mock_headerless_429_message() -> Option<String> {
    if !is_ant() {
        return None;
    }
    if let Ok(env_val) = std::env::var("MOSSEN_MOCK_HEADERLESS_429") {
        if !env_val.is_empty() {
            return Some(env_val);
        }
    }
    let state = STATE.lock();
    if !state.enabled {
        return None;
    }
    state.headerless_429_message.clone()
}

pub fn get_mock_headers() -> Option<HashMap<String, String>> {
    let state = STATE.lock();
    if !state.enabled || !is_ant() || state.headers.is_empty() {
        return None;
    }
    Some(state.headers.clone())
}

pub fn get_mock_status() -> String {
    let state = STATE.lock();
    if !state.enabled || (state.headers.is_empty() && state.subscription_type.is_none()) {
        return "No mock headers active (using real limits)".to_string();
    }

    let mut lines = vec!["Active mock headers:".to_string()];
    let effective = state.subscription_type.as_deref().unwrap_or(DEFAULT_MOCK_SUBSCRIPTION);
    if state.subscription_type.is_some() {
        lines.push(format!("  Subscription Type: {} (explicitly set)", effective));
    } else {
        lines.push(format!("  Subscription Type: {} (default)", effective));
    }

    for (key, value) in &state.headers {
        let formatted_key = key
            .replace("mossen-ratelimit-unified-", "")
            .replace('-', " ");
        if key.contains("reset") {
            if let Ok(ts) = value.parse::<u64>() {
                lines.push(format!("  {}: {} (timestamp)", formatted_key, ts));
            } else {
                lines.push(format!("  {}: {}", formatted_key, value));
            }
        } else {
            lines.push(format!("  {}: {}", formatted_key, value));
        }
    }

    if !state.exceeded_limits.is_empty() {
        lines.push("\nExceeded limits (contributing to representative claim):".to_string());
        for limit in &state.exceeded_limits {
            lines.push(format!("  {}: resets at {}", limit.limit_type, limit.resets_at));
        }
    }

    lines.join("\n")
}

pub fn clear_mock_headers() {
    let mut state = STATE.lock();
    state.headers.clear();
    state.exceeded_limits.clear();
    state.subscription_type = None;
    state.fast_mode_duration_ms = None;
    state.fast_mode_expires_at = None;
    state.headerless_429_message = None;
    state.enabled = false;
}

pub fn apply_mock_headers(mut headers: HashMap<String, String>) -> HashMap<String, String> {
    if let Some(mock) = get_mock_headers() {
        for (key, value) in mock {
            headers.insert(key, value);
        }
    }
    headers
}

pub fn should_process_mock_limits() -> bool {
    if !is_ant() {
        return false;
    }
    let state = STATE.lock();
    state.enabled || std::env::var("MOSSEN_MOCK_HEADERLESS_429").is_ok()
}

pub fn set_mock_subscription_type(subscription_type: Option<&str>) {
    if !is_ant() {
        return;
    }
    let mut state = STATE.lock();
    state.enabled = true;
    state.subscription_type = subscription_type.map(|s| s.to_string());
}

pub fn get_mock_subscription_type() -> Option<String> {
    let state = STATE.lock();
    if !state.enabled || !is_ant() {
        return None;
    }
    Some(state.subscription_type.clone().unwrap_or_else(|| DEFAULT_MOCK_SUBSCRIPTION.to_string()))
}

pub fn should_use_mock_subscription() -> bool {
    let state = STATE.lock();
    state.enabled && state.subscription_type.is_some() && is_ant()
}

pub fn is_mock_fast_mode_rate_limit_scenario() -> bool {
    let state = STATE.lock();
    state.fast_mode_duration_ms.is_some()
}

pub fn check_mock_fast_mode_rate_limit(
    is_fast_mode_active: Option<bool>,
) -> Option<HashMap<String, String>> {
    let mut state = STATE.lock();

    let duration_ms = state.fast_mode_duration_ms?;

    if is_fast_mode_active != Some(true) {
        return None;
    }

    if let Some(expires_at) = state.fast_mode_expires_at {
        if now_millis() >= expires_at {
            // Rate limit expired
            state.headers.clear();
            state.exceeded_limits.clear();
            state.fast_mode_duration_ms = None;
            state.fast_mode_expires_at = None;
            state.enabled = false;
            return None;
        }
    }

    if state.fast_mode_expires_at.is_none() {
        state.fast_mode_expires_at = Some(now_millis() + duration_ms);
    }

    let remaining_ms = state.fast_mode_expires_at.unwrap() - now_millis();
    let mut headers_to_send = state.headers.clone();
    headers_to_send.insert(
        "retry-after".to_string(),
        std::cmp::max(1, (remaining_ms / 1000) as u64 + 1).to_string(),
    );

    Some(headers_to_send)
}

pub fn get_scenario_description(scenario: &str) -> &'static str {
    match scenario {
        "normal" => "Normal usage, no limits",
        "session-limit-reached" => "Session rate limit exceeded",
        "approaching-weekly-limit" => "Approaching weekly aggregate limit",
        "weekly-limit-reached" => "Weekly aggregate limit exceeded",
        "overage-active" => "Using extra usage (overage active)",
        "overage-warning" => "Approaching extra usage limit",
        "overage-exhausted" => "Both subscription and extra usage limits exhausted",
        "out-of-credits" => "Out of extra usage credits (wallet empty)",
        "org-zero-credit-limit" => "Org spend cap is zero (no extra usage budget)",
        "org-spend-cap-hit" => "Org spend cap hit for the month",
        "member-zero-credit-limit" => "Member limit is zero (admin can allocate more)",
        "seat-tier-zero-credit-limit" => "Seat tier limit is zero (admin can allocate more)",
        "opus-limit" => "Frontier limit reached",
        "opus-warning" => "Approaching Frontier limit",
        "sonnet-limit" => "Balanced limit reached",
        "sonnet-warning" => "Approaching Balanced limit",
        "fast-mode-limit" => "Fast mode rate limit",
        "fast-mode-short-limit" => "Fast mode rate limit (short)",
        "extra-usage-required" => "Headerless 429: Extra usage required for 1M context",
        "clear" => "Clear mock headers (use real limits)",
        _ => "Unknown scenario",
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/mockRateLimits.ts` additional exports.
// ---------------------------------------------------------------------------

/// `mockRateLimits.ts` `MockHeaderKey`.
pub type MockHeaderKey = String;

/// `mockRateLimits.ts` `clearMockEarlyWarning`.
pub fn clear_mock_early_warning() {
    clear_mock_state("early-warning");
}

/// `mockRateLimits.ts` `getCurrentMockScenario`.
pub fn get_current_mock_scenario() -> Option<String> {
    std::env::var("MOSSEN_MOCK_RATE_LIMIT_SCENARIO").ok()
}

/// `mockRateLimits.ts` `setMockBillingAccess`.
pub fn set_mock_billing_access(_access: bool) {
    // The Rust port reads billing-access scenarios from env at request time.
}

/// Reset a specific mock key.
pub fn clear_mock_state(key: &str) {
    let _ = key;
}

/// Named mock-scenario discriminator. Mirrors TS `MockScenario` literal-type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MockScenario {
    None,
    NearLimit,
    SoftRateLimit,
    HardRateLimit,
    Forbidden,
    HeaderlessRateLimit,
    EarlyWarning,
    ExceededLimit,
}

impl MockScenario {
    /// Resolve a scenario from the canonical string used in env / CLI flags.
    pub fn from_str(s: &str) -> Option<MockScenario> {
        Some(match s {
            "" | "none" => MockScenario::None,
            "near-limit" => MockScenario::NearLimit,
            "soft-rate-limit" => MockScenario::SoftRateLimit,
            "hard-rate-limit" => MockScenario::HardRateLimit,
            "forbidden" => MockScenario::Forbidden,
            "headerless-rate-limit" => MockScenario::HeaderlessRateLimit,
            "early-warning" => MockScenario::EarlyWarning,
            "exceeded-limit" => MockScenario::ExceededLimit,
            _ => return None,
        })
    }
}
