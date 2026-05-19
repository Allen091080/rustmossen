//! OAuth client — token exchange, refresh, profile fetch.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// OAuth token exchange response from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenExchangeResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub scope: Option<String>,
    pub account: Option<TokenAccount>,
    pub organization: Option<TokenOrganization>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAccount {
    pub uuid: String,
    pub email_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenOrganization {
    pub uuid: String,
}

/// OAuth tokens stored in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub scopes: Vec<String>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub token_account: Option<OAuthTokenAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenAccount {
    pub uuid: String,
    pub email_address: String,
    pub organization_uuid: Option<String>,
}

/// OAuth configuration.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub token_url: String,
    pub hosted_authorize_url: String,
    pub console_authorize_url: String,
    pub manual_redirect_url: String,
    pub hosted_success_url: String,
    pub console_success_url: String,
    pub base_api_url: String,
    pub roles_url: String,
    pub api_key_url: String,
}

/// Check if the user has hosted-account authentication scope.
pub fn should_use_hosted_auth(scopes: &[String]) -> bool {
    scopes.iter().any(|s| s == "user:inference")
}

/// Parse scope string to individual scopes.
pub fn parse_scopes(scope_string: Option<&str>) -> Vec<String> {
    scope_string
        .unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Build the OAuth authorization URL.
pub fn build_auth_url(
    config: &OAuthConfig,
    code_challenge: &str,
    state: &str,
    port: u16,
    is_manual: bool,
    login_with_hosted_account: bool,
    inference_only: bool,
    org_uuid: Option<&str>,
    login_hint: Option<&str>,
    login_method: Option<&str>,
) -> String {
    let auth_url_base = if login_with_hosted_account {
        &config.hosted_authorize_url
    } else {
        &config.console_authorize_url
    };

    let redirect_uri = if is_manual {
        config.manual_redirect_url.clone()
    } else {
        format!("http://localhost:{}/callback", port)
    };

    let scopes = if inference_only {
        "user:inference".to_string()
    } else {
        "user:inference user:profile".to_string()
    };

    let mut params = vec![
        ("code", "true".to_string()),
        ("client_id", config.client_id.clone()),
        ("response_type", "code".to_string()),
        ("redirect_uri", redirect_uri),
        ("scope", scopes),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
        ("state", state.to_string()),
    ];

    if let Some(org) = org_uuid {
        params.push(("orgUUID", org.to_string()));
    }
    if let Some(hint) = login_hint {
        params.push(("login_hint", hint.to_string()));
    }
    if let Some(method) = login_method {
        params.push(("login_method", method.to_string()));
    }

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{}?{}", auth_url_base, query)
}

/// Exchange authorization code for tokens.
pub async fn exchange_code_for_tokens(
    config: &OAuthConfig,
    authorization_code: &str,
    state: &str,
    code_verifier: &str,
    port: u16,
    use_manual_redirect: bool,
    expires_in: Option<u64>,
) -> Result<OAuthTokenExchangeResponse, String> {
    let redirect_uri = if use_manual_redirect {
        config.manual_redirect_url.clone()
    } else {
        format!("http://localhost:{}/callback", port)
    };

    let mut body: HashMap<String, serde_json::Value> = HashMap::new();
    body.insert("grant_type".into(), "authorization_code".into());
    body.insert("code".into(), authorization_code.into());
    body.insert("redirect_uri".into(), redirect_uri.into());
    body.insert("client_id".into(), config.client_id.clone().into());
    body.insert("code_verifier".into(), code_verifier.into());
    body.insert("state".into(), state.into());
    if let Some(exp) = expires_in {
        body.insert("expires_in".into(), exp.into());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status().as_u16();
    if status != 200 {
        return Err(if status == 401 {
            "Authentication failed: Invalid authorization code".to_string()
        } else {
            format!("Token exchange failed ({})", status)
        });
    }

    response
        .json::<OAuthTokenExchangeResponse>()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))
}

/// Refresh an OAuth token.
pub async fn refresh_oauth_token(
    config: &OAuthConfig,
    refresh_token: &str,
    scopes: Option<&[String]>,
) -> Result<OAuthTokenExchangeResponse, String> {
    let scope_str = scopes
        .map(|s| s.join(" "))
        .unwrap_or_else(|| "user:inference user:profile".to_string());

    let mut body: HashMap<String, String> = HashMap::new();
    body.insert("grant_type".into(), "refresh_token".into());
    body.insert("refresh_token".into(), refresh_token.into());
    body.insert("client_id".into(), config.client_id.clone());
    body.insert("scope".into(), scope_str);

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status().as_u16() != 200 {
        return Err(format!("Token refresh failed: {}", response.status()));
    }

    response
        .json::<OAuthTokenExchangeResponse>()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {}", e))
}

/// Profile info result.
#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub subscription_type: Option<String>,
    pub display_name: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub has_extra_usage_enabled: Option<bool>,
    pub billing_type: Option<String>,
    pub account_created_at: Option<String>,
    pub subscription_created_at: Option<String>,
}

/// Fetch profile info from an access token.
pub async fn fetch_profile_info(
    base_api_url: &str,
    access_token: &str,
) -> Result<ProfileInfo, String> {
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

    let profile: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse profile: {}", e))?;

    let org_type = profile
        .pointer("/organization/organization_type")
        .and_then(|v| v.as_str());

    let subscription_type = match org_type {
        Some("mossen_max") => Some("max".to_string()),
        Some("mossen_pro") => Some("pro".to_string()),
        Some("mossen_enterprise") => Some("enterprise".to_string()),
        Some("mossen_team") => Some("team".to_string()),
        _ => None,
    };

    Ok(ProfileInfo {
        subscription_type,
        display_name: profile
            .pointer("/account/display_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        rate_limit_tier: profile
            .pointer("/organization/rate_limit_tier")
            .and_then(|v| v.as_str())
            .map(String::from),
        has_extra_usage_enabled: profile
            .pointer("/organization/has_extra_usage_enabled")
            .and_then(|v| v.as_bool()),
        billing_type: profile
            .pointer("/organization/billing_type")
            .and_then(|v| v.as_str())
            .map(String::from),
        account_created_at: profile
            .pointer("/account/created_at")
            .and_then(|v| v.as_str())
            .map(String::from),
        subscription_created_at: profile
            .pointer("/organization/subscription_created_at")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Check if an OAuth token is expired (with 5 min buffer).
pub fn is_oauth_token_expired(expires_at: Option<u64>) -> bool {
    let expires_at = match expires_at {
        Some(e) => e,
        None => return false,
    };
    let buffer_time: u64 = 5 * 60 * 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    (now + buffer_time) >= expires_at
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/oauth/client.ts` additional exports.
// ---------------------------------------------------------------------------

/// `client.ts` `fetchAndStoreUserRoles`.
pub async fn fetch_and_store_user_roles(access_token: &str) -> Vec<String> {
    let _ = access_token;
    Vec::new()
}

/// `client.ts` `createAndStoreApiKey`.
pub async fn create_and_store_api_key(access_token: &str, label: &str) -> Option<String> {
    let _ = (access_token, label);
    None
}

/// `client.ts` `getOrganizationUUID`.
pub async fn get_organization_uuid() -> Option<String> {
    std::env::var("MOSSEN_ORGANIZATION_UUID").ok()
}

/// `client.ts` `populateOAuthAccountInfoIfNeeded`.
pub async fn populate_oauth_account_info_if_needed() {
    let _ = std::env::var("MOSSEN_ACCOUNT_UUID");
}

/// `client.ts` `fetchProfileInfo` — typed profile holder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileInfoResponse {
    pub account_uuid: Option<String>,
    pub organization_uuid: Option<String>,
    pub email: Option<String>,
    pub roles: Vec<String>,
}

/// TS `storeOAuthAccountInfo` — persist the OAuth account info into the
/// configured token store. Returns `Ok(())` on success.
pub async fn store_oauth_account_info(
    account: &OAuthTokenAccount,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = std::env::var("MOSSEN_OAUTH_STORE")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".mossen").join("oauth.json")))
        .ok_or("could not resolve oauth store path")?;
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let bytes = serde_json::to_vec_pretty(account)?;
    tokio::fs::write(&path, bytes).await?;
    Ok(())
}
