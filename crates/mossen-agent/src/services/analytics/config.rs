//! Shared analytics configuration — determines when analytics should be disabled.

use std::env;

/// Check if analytics operations should be disabled.
///
/// Analytics is disabled in the following cases:
/// - Test environment (NODE_ENV === 'test')
/// - Third-party cloud providers (Bedrock/Vertex)
/// - Privacy level is no-telemetry or essential-traffic
pub fn is_analytics_disabled() -> bool {
    env::var("NODE_ENV").ok().as_deref() == Some("test")
        || is_custom_backend_enabled()
        || is_env_truthy("MOSSEN_CODE_USE_BEDROCK")
        || is_env_truthy("MOSSEN_CODE_USE_VERTEX")
        || is_env_truthy("MOSSEN_CODE_USE_FOUNDRY")
        || is_telemetry_disabled()
}

/// Check if the feedback survey should be suppressed.
pub fn is_feedback_survey_disabled() -> bool {
    env::var("NODE_ENV").ok().as_deref() == Some("test") || is_telemetry_disabled()
}

fn is_env_truthy(key: &str) -> bool {
    env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn is_custom_backend_enabled() -> bool {
    env::var("MOSSEN_CODE_CUSTOM_BACKEND").ok().is_some()
        || env::var("CUSTOM_API_BASE_URL").ok().is_some()
}

fn is_telemetry_disabled() -> bool {
    is_env_truthy("MOSSEN_CODE_NO_TELEMETRY")
        || env::var("MOSSEN_CODE_PRIVACY_LEVEL")
            .ok()
            .map(|v| v == "no-telemetry" || v == "essential-traffic")
            .unwrap_or(false)
}
