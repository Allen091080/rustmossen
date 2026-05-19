//! # First Token Date API
//!
//! 翻译自 `services/api/firstTokenDate.ts` (66行)
//! 获取并存储用户首次使用 token 的日期。

use reqwest::Client;
use tracing::error;
use chrono::NaiveDateTime;

/// Configuration callback interface for fetching/saving first token date.
pub struct FirstTokenDateConfig {
    pub mossen_first_token_date: Option<String>,
}

/// Fetch the user's first hosted-usage token date and store it in config.
/// In custom backend mode this remains a no-op because hosted usage metadata
/// is not fetched from first-party services.
pub async fn fetch_and_store_mossen_first_token_date(
    client: &Client,
    base_api_url: &str,
    auth_headers: &[(String, String)],
    user_agent: &str,
    is_custom_backend: bool,
    current_first_token_date: Option<&str>,
) -> Result<Option<String>, anyhow::Error> {
    if is_custom_backend {
        return Ok(None);
    }

    // Already have a cached value
    if current_first_token_date.is_some() {
        return Ok(None);
    }

    let url = format!("{}/api/organization/mossen_first_token_date", base_api_url);

    let mut request = client
        .get(&url)
        .header("User-Agent", user_agent)
        .timeout(std::time::Duration::from_secs(10));

    for (key, value) in auth_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    let response = request.send().await?;
    let body: serde_json::Value = response.json().await?;

    let first_token_date = body
        .get("first_token_date")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Validate the date if it's not null
    if let Some(ref date_str) = first_token_date {
        // Try parsing as ISO 8601/RFC 3339
        if chrono::DateTime::parse_from_rfc3339(date_str).is_err() {
            // Try other common formats
            if NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S").is_err()
                && NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d").is_err()
            {
                error!(
                    "Received invalid first_token_date from API: {}",
                    date_str
                );
                return Ok(None);
            }
        }
    }

    Ok(first_token_date)
}
