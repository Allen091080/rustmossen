//! # Usage API
//!
//! 翻译自 `services/api/usage.ts` (64行)
//! 提供使用量查询功能。

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Rate limit information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// A percentage from 0 to 100
    pub utilization: Option<f64>,
    /// ISO 8601 timestamp
    pub resets_at: Option<String>,
}

/// Extra usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
}

/// Utilization data from the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Utilization {
    pub five_hour: Option<RateLimit>,
    pub seven_day: Option<RateLimit>,
    pub seven_day_oauth_apps: Option<RateLimit>,
    pub seven_day_opus: Option<RateLimit>,
    pub seven_day_sonnet: Option<RateLimit>,
    pub extra_usage: Option<ExtraUsage>,
}

/// Fetch utilization data from the OAuth usage endpoint.
///
/// Returns `None` if the user is not a hosted subscriber or doesn't have profile scope,
/// or if the OAuth token is expired.
pub async fn fetch_utilization(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    is_hosted_subscriber: bool,
    has_profile_scope: bool,
    is_token_expired: bool,
) -> Result<Option<Utilization>, anyhow::Error> {
    if !is_hosted_subscriber || !has_profile_scope {
        return Ok(Some(Utilization::default()));
    }

    if is_token_expired {
        return Ok(None);
    }

    let url = format!("{}/api/oauth/usage", base_api_url);

    let mut request = client
        .get(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", user_agent)
        .timeout(std::time::Duration::from_secs(5));

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;
    let utilization: Utilization = response.json().await?;
    Ok(Some(utilization))
}
