//! OAuth profile fetching.

use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// OAuth profile response from the API.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProfileResponse {
    pub account: OAuthAccount,
    pub organization: OAuthOrganization,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthAccount {
    pub uuid: String,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthOrganization {
    pub uuid: String,
    pub organization_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub has_extra_usage_enabled: Option<bool>,
    pub billing_type: Option<String>,
    pub subscription_created_at: Option<String>,
}

/// Fetch OAuth profile using an API key.
pub async fn get_oauth_profile_from_api_key(
    base_api_url: &str,
    api_key: &str,
    account_uuid: &str,
) -> Result<OAuthProfileResponse, String> {
    let endpoint = format!("{}/api/mossen/profile", base_api_url);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&endpoint)
        .header("x-api-key", api_key)
        .query(&[("account_uuid", account_uuid)])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().as_u16() != 200 {
        return Err(format!("Profile fetch failed: {}", response.status()));
    }

    response
        .json::<OAuthProfileResponse>()
        .await
        .map_err(|e| format!("Failed to parse profile: {}", e))
}

/// Fetch OAuth profile using an OAuth access token.
pub async fn get_oauth_profile_from_oauth_token(
    base_api_url: &str,
    access_token: &str,
) -> Result<OAuthProfileResponse, String> {
    let endpoint = format!("{}/api/oauth/profile", base_api_url);
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&endpoint)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().as_u16() != 200 {
        return Err(format!("Profile fetch failed: {}", response.status()));
    }

    response
        .json::<OAuthProfileResponse>()
        .await
        .map_err(|e| format!("Failed to parse profile: {}", e))
}
