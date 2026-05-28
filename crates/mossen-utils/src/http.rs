//! HTTP client helpers.
//!
//! Provides user-agent construction, authentication header assembly, and
//! OAuth 401 retry logic.

use std::collections::HashMap;

use crate::env::get_env;

// ---------------------------------------------------------------------------
// User-Agent strings
// ---------------------------------------------------------------------------

/// Build the CLI user-agent string.
/// Format: `mossen-cli/{version} ({user_type}, {entrypoint})`
pub fn get_user_agent(version: &str) -> String {
    let user_type = get_env("USER_TYPE").unwrap_or_else(|| "external".to_string());
    let entrypoint = get_env("MOSSEN_CODE_ENTRYPOINT").unwrap_or_else(|| "cli".to_string());

    let mut parts = vec![];
    if let Some(sdk_ver) = get_env("MOSSEN_AGENT_SDK_VERSION") {
        parts.push(format!("agent-sdk/{sdk_ver}"));
    }
    if let Some(client_app) = get_env("MOSSEN_AGENT_SDK_CLIENT_APP") {
        parts.push(format!("client-app/{client_app}"));
    }

    let extra = if parts.is_empty() {
        String::new()
    } else {
        format!(", {}", parts.join(", "))
    };

    format!("mossen-cli/{version} ({user_type}, {entrypoint}{extra})")
}

/// Build the MCP user-agent string.
pub fn get_mcp_user_agent(version: &str) -> String {
    let mut parts = vec![];
    if let Some(entrypoint) = get_env("MOSSEN_CODE_ENTRYPOINT") {
        parts.push(entrypoint);
    }
    if let Some(sdk_ver) = get_env("MOSSEN_AGENT_SDK_VERSION") {
        parts.push(format!("agent-sdk/{sdk_ver}"));
    }
    if let Some(client_app) = get_env("MOSSEN_AGENT_SDK_CLIENT_APP") {
        parts.push(format!("client-app/{client_app}"));
    }
    let suffix = if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join(", "))
    };
    format!("mossen-code/{version}{suffix}")
}

// ---------------------------------------------------------------------------
// Authentication headers
// ---------------------------------------------------------------------------

/// Result of building authentication headers.
#[derive(Debug, Clone)]
pub struct AuthHeaders {
    pub headers: HashMap<String, String>,
    pub error: Option<String>,
}

/// Build authentication headers for API requests.
///
/// Priority:
/// 1. Custom backend auth headers (if custom backend is enabled)
/// 2. OAuth Bearer token (for hosted subscribers)
/// 3. API key header (x-api-key)
///
/// The actual token retrieval is abstracted via the `AuthProvider` trait since
/// credential storage is platform-specific.
pub fn build_auth_headers(provider: &dyn AuthProvider) -> AuthHeaders {
    if let Some(custom_headers) = provider.custom_backend_headers() {
        return AuthHeaders {
            headers: custom_headers,
            error: None,
        };
    }

    if provider.is_hosted_subscriber() {
        match provider.oauth_access_token() {
            Some(token) => {
                let mut headers = HashMap::new();
                headers.insert("Authorization".to_string(), format!("Bearer {token}"));
                // Beta header for OAuth-authenticated requests
                headers.insert("mossen-beta".to_string(), "oauth-2025-01".to_string());
                return AuthHeaders {
                    headers,
                    error: None,
                };
            }
            None => {
                return AuthHeaders {
                    headers: HashMap::new(),
                    error: Some("No OAuth token available".to_string()),
                };
            }
        }
    }

    match provider.api_key() {
        Some(key) => {
            let mut headers = HashMap::new();
            headers.insert("x-api-key".to_string(), key);
            AuthHeaders {
                headers,
                error: None,
            }
        }
        None => AuthHeaders {
            headers: HashMap::new(),
            error: Some("No API key available".to_string()),
        },
    }
}

/// Trait for providing authentication credentials.
/// Implementations live in the service layer (mossen-gateway crate).
pub trait AuthProvider: Send + Sync {
    /// Return custom backend auth headers, if custom backend is enabled.
    fn custom_backend_headers(&self) -> Option<HashMap<String, String>>;
    /// Whether the user is a hosted (Max/Pro) subscriber.
    fn is_hosted_subscriber(&self) -> bool;
    /// Return the OAuth access token, if available.
    fn oauth_access_token(&self) -> Option<String>;
    /// Return the API key, if available.
    fn api_key(&self) -> Option<String>;
}

// ---------------------------------------------------------------------------
// OAuth 401 retry
// ---------------------------------------------------------------------------

/// Wrapper that retries a request once after refreshing the OAuth token on 401.
///
/// `execute` performs the HTTP request (should re-read auth headers on retry).
/// `refresh_token` is called to refresh the OAuth token.
pub async fn with_oauth_401_retry<T, E, Req, Refresh>(
    mut execute: Req,
    refresh_token: Refresh,
) -> Result<T, E>
where
    Req: FnMut() -> futures::future::BoxFuture<'static, Result<T, E>>,
    Refresh: FnOnce() -> futures::future::BoxFuture<'static, Result<(), E>>,
    E: IsAuthError,
{
    match execute().await {
        Ok(val) => Ok(val),
        Err(err) if err.is_auth_error() => {
            refresh_token().await?;
            execute().await
        }
        Err(err) => Err(err),
    }
}

/// Trait to check if an error is an auth (401/403) error.
pub trait IsAuthError {
    fn is_auth_error(&self) -> bool;
}

/// Build the User-Agent string for `WebFetch` requests to arbitrary sites.
///
/// Mirrors TS `getWebFetchUserAgent`. `mossen_user_agent` should be the value
/// returned by `getMossenUserAgent()` (see `user_agent.rs`).
pub fn get_web_fetch_user_agent(mossen_user_agent: &str) -> String {
    format!("Mossen-User ({mossen_user_agent}; +https://mossen.invalid/)")
}

/// Public alias for [`build_auth_headers`] matching the TS `getAuthHeaders` name.
///
/// In the TS code this is a parameter-less function; in Rust the caller passes
/// the platform-specific `AuthProvider`. The wrapper makes call sites that
/// translate TS one-to-one clearer.
pub fn get_auth_headers(provider: &dyn AuthProvider) -> AuthHeaders {
    build_auth_headers(provider)
}
