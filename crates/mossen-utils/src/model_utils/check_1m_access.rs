//! 1M context-access checks for Max/Balanced.
//!
//! Direct translation of `utils/model/check1mAccess.ts`.

use crate::auth::is_hosted_subscriber;
use crate::config::get_global_config;
use crate::context::is_1m_context_disabled;
use crate::custom_backend::{get_custom_backend_max_input_tokens, is_custom_backend_enabled};

/// Extra-usage disabled-reason strings that still mean "the user has the
/// feature provisioned". Matches TS `OverageDisabledReason` semantics: only
/// `out_of_credits` still counts as enabled.
fn reason_still_means_enabled(reason: &str) -> bool {
    matches!(reason, "out_of_credits")
}

/// Check if extra usage is enabled based on the cached disabled reason.
///
/// Mirrors the TS tri-state:
/// - `undefined` (no cache yet): treat as not enabled (conservative).
/// - `null` (cache says no disabled reason): extra usage is enabled.
/// - `Some(reason)`: check whether the reason still counts as provisioned.
///
/// Rust collapses `undefined` and `null` into `Option::None`. The existing
/// `GlobalConfig` mirror uses `None` as the unloaded default, so we treat
/// `None` like the conservative TS `undefined` branch and require an explicit
/// empty string to represent the "cleared by API" case.
fn is_extra_usage_enabled() -> bool {
    let cfg = get_global_config();
    match cfg.cached_extra_usage_disabled_reason.as_deref() {
        None => false,
        Some("") => true,
        Some(reason) => reason_still_means_enabled(reason),
    }
}

pub fn check_max_1m_access() -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    if is_custom_backend_enabled() {
        return get_custom_backend_max_input_tokens().unwrap_or(0) >= 1_000_000;
    }
    if is_hosted_subscriber() {
        return is_extra_usage_enabled();
    }
    true
}

pub fn check_balanced_1m_access() -> bool {
    if is_1m_context_disabled() {
        return false;
    }
    if is_custom_backend_enabled() {
        return get_custom_backend_max_input_tokens().unwrap_or(0) >= 1_000_000;
    }
    if is_hosted_subscriber() {
        return is_extra_usage_enabled();
    }
    true
}
