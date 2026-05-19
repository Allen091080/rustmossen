/// Facade for rate limit header processing.
/// This isolates mock logic from production code.

use super::mock_rate_limits::{
    apply_mock_headers, check_mock_fast_mode_rate_limit, get_mock_headerless_429_message,
    get_mock_headers, is_mock_fast_mode_rate_limit_scenario, should_process_mock_limits,
};

/// Trait for model name resolution
pub trait RateLimitMockingContext: Send + Sync {
    fn get_canonical_name(&self, model: &str) -> String;
}

fn is_frontier_model(ctx: &dyn RateLimitMockingContext, model: &str) -> bool {
    let normalized = model.to_lowercase();
    let canonical = ctx.get_canonical_name(&normalized);
    canonical.contains("mossen-opus")
        || normalized == "opus"
        || normalized.starts_with("opus[")
}

/// Process headers, applying mocks if /mock-limits command is active
pub fn process_rate_limit_headers(
    headers: std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    if should_process_mock_limits() {
        return apply_mock_headers(headers);
    }
    headers
}

/// Check if we should process rate limits (either real subscriber or /mock-limits)
pub fn should_process_rate_limits(is_subscriber: bool) -> bool {
    is_subscriber || should_process_mock_limits()
}

/// Check if mock rate limits should produce a 429 error.
/// Returns Some(error_message) if a 429 should be thrown, None otherwise.
pub fn check_mock_rate_limit_error(
    ctx: &dyn RateLimitMockingContext,
    current_model: &str,
    is_fast_mode_active: Option<bool>,
) -> Option<MockRateLimitError> {
    if !should_process_mock_limits() {
        return None;
    }

    if let Some(headerless_msg) = get_mock_headerless_429_message() {
        return Some(MockRateLimitError {
            status: 429,
            message: headerless_msg,
            headers: std::collections::HashMap::new(),
        });
    }

    let mock_headers = get_mock_headers()?;

    let status = mock_headers.get("mossen-ratelimit-unified-status").map(|s| s.as_str());
    let overage_status = mock_headers
        .get("mossen-ratelimit-unified-overage-status")
        .map(|s| s.as_str());
    let rate_limit_type = mock_headers
        .get("mossen-ratelimit-unified-representative-claim")
        .map(|s| s.as_str());

    let is_frontier_limit = rate_limit_type == Some("seven_day_opus");
    let is_using_frontier = is_frontier_model(ctx, current_model);

    if is_frontier_limit && !is_using_frontier {
        return None;
    }

    // Check for mock fast mode rate limits
    if is_mock_fast_mode_rate_limit_scenario() {
        if let Some(fast_headers) = check_mock_fast_mode_rate_limit(is_fast_mode_active) {
            return Some(MockRateLimitError {
                status: 429,
                message: "Rate limit exceeded".to_string(),
                headers: fast_headers,
            });
        }
        return None;
    }

    let should_throw_429 =
        status == Some("rejected") && (overage_status.is_none() || overage_status == Some("rejected"));

    if should_throw_429 {
        return Some(MockRateLimitError {
            status: 429,
            message: "Rate limit exceeded".to_string(),
            headers: mock_headers,
        });
    }

    None
}

#[derive(Debug, Clone)]
pub struct MockRateLimitError {
    pub status: u16,
    pub message: String,
    pub headers: std::collections::HashMap<String, String>,
}

/// Check if this is a mock 429 error that shouldn't be retried
pub fn is_mock_rate_limit_error(status: u16) -> bool {
    should_process_mock_limits() && status == 429
}
