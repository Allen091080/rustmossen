//! # OAuth (oauth.ts)
//!
//! OAuth 配置常量、类型和函数。

/// OAuth config type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OauthConfigType {
    Prod,
    Staging,
    Local,
}

/// Determine OAuth config type from env vars.
pub fn get_oauth_config_type(use_local: bool, use_staging: bool) -> OauthConfigType {
    if use_local {
        return OauthConfigType::Local;
    }
    if use_staging {
        return OauthConfigType::Staging;
    }
    OauthConfigType::Prod
}

/// Get the file suffix for the current OAuth config.
pub fn file_suffix_for_oauth_config(
    custom_oauth_url: Option<&str>,
    config_type: OauthConfigType,
) -> &'static str {
    if custom_oauth_url.is_some() {
        return "-custom-oauth";
    }
    match config_type {
        OauthConfigType::Local => "-local-oauth",
        OauthConfigType::Staging => "-staging-oauth",
        OauthConfigType::Prod => "",
    }
}

pub const HOSTED_INFERENCE_SCOPE: &str = "user:inference";
pub const HOSTED_PROFILE_SCOPE: &str = "user:profile";
pub const CONSOLE_SCOPE: &str = "org:create_api_key";
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

/// Console OAuth scopes - for API key creation via Console.
pub const CONSOLE_OAUTH_SCOPES: &[&str] = &[CONSOLE_SCOPE, HOSTED_PROFILE_SCOPE];

/// Hosted OAuth scopes - for hosted subscribers (Pro/Max/Team/Enterprise).
pub const HOSTED_OAUTH_SCOPES: &[&str] = &[
    HOSTED_PROFILE_SCOPE,
    HOSTED_INFERENCE_SCOPE,
    "user:sessions:mossen_code",
    "user:mcp_servers",
    "user:file_upload",
];

/// All OAuth scopes - union of all scopes used in Mossen CLI.
/// When logging in, request all scopes in order to handle both Console -> hosted redirect.
pub fn all_oauth_scopes() -> Vec<&'static str> {
    let mut scopes = Vec::new();
    for &s in CONSOLE_OAUTH_SCOPES {
        if !scopes.contains(&s) {
            scopes.push(s);
        }
    }
    for &s in HOSTED_OAUTH_SCOPES {
        if !scopes.contains(&s) {
            scopes.push(s);
        }
    }
    scopes
}

/// OAuth configuration struct.
#[derive(Debug, Clone)]
pub struct OauthConfig {
    pub base_api_url: String,
    pub console_authorize_url: String,
    pub hosted_authorize_url: String,
    /// The hosted web origin. Separate from hosted_authorize_url because
    /// some deployments may route auth through a different path.
    pub hosted_origin: String,
    pub token_url: String,
    pub api_key_url: String,
    pub roles_url: String,
    pub console_success_url: String,
    pub hosted_success_url: String,
    pub manual_redirect_url: String,
    pub client_id: String,
    pub oauth_file_suffix: String,
    pub mcp_proxy_url: String,
    pub mcp_proxy_path: String,
}

/// Production OAuth configuration - Used in normal operation.
pub fn prod_oauth_config() -> OauthConfig {
    OauthConfig {
        base_api_url: "https://api.mossen.invalid".to_string(),
        console_authorize_url: "https://platform.mossen.invalid/oauth/authorize".to_string(),
        hosted_authorize_url: "https://platform.mossen.invalid/oauth/authorize".to_string(),
        hosted_origin: "https://platform.mossen.invalid".to_string(),
        token_url: "https://platform.mossen.invalid/v1/oauth/token".to_string(),
        api_key_url: "https://api.mossen.invalid/api/oauth/mossen_cli/create_api_key".to_string(),
        roles_url: "https://api.mossen.invalid/api/oauth/mossen_cli/roles".to_string(),
        console_success_url: "https://platform.mossen.invalid/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dmossen-code".to_string(),
        hosted_success_url: "https://platform.mossen.invalid/oauth/code/success?app=mossen-code".to_string(),
        manual_redirect_url: "https://platform.mossen.invalid/oauth/code/callback".to_string(),
        client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
        oauth_file_suffix: String::new(),
        mcp_proxy_url: "https://mcp-proxy.mossen.invalid".to_string(),
        mcp_proxy_path: "/v1/mcp/{server_id}".to_string(),
    }
}

/// Client ID Metadata Document URL for MCP OAuth (CIMD / SEP-991).
pub const MCP_CLIENT_METADATA_URL: &str =
    "https://platform.mossen.invalid/oauth/mossen-code-client-metadata";

/// Staging OAuth configuration.
pub fn staging_oauth_config() -> OauthConfig {
    OauthConfig {
        base_api_url: "https://api-staging.mossen.invalid".to_string(),
        console_authorize_url: "https://platform.staging.mossen.invalid/oauth/authorize".to_string(),
        hosted_authorize_url: "https://hosted.staging.mossen.invalid/oauth/authorize".to_string(),
        hosted_origin: "https://hosted.staging.mossen.invalid".to_string(),
        token_url: "https://platform.staging.mossen.invalid/v1/oauth/token".to_string(),
        api_key_url: "https://api-staging.mossen.invalid/api/oauth/mossen_cli/create_api_key".to_string(),
        roles_url: "https://api-staging.mossen.invalid/api/oauth/mossen_cli/roles".to_string(),
        console_success_url: "https://platform.staging.mossen.invalid/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dmossen-code".to_string(),
        hosted_success_url: "https://platform.staging.mossen.invalid/oauth/code/success?app=mossen-code".to_string(),
        manual_redirect_url: "https://platform.staging.mossen.invalid/oauth/code/callback".to_string(),
        client_id: "22422756-60c9-4084-8eb7-27705fd5cf9a".to_string(),
        oauth_file_suffix: "-staging-oauth".to_string(),
        mcp_proxy_url: "https://mcp-proxy-staging.mossen.invalid".to_string(),
        mcp_proxy_path: "/v1/mcp/{server_id}".to_string(),
    }
}

/// Local dev OAuth configuration.
/// Three local dev servers: :8000 API proxy, :4000 hosted frontend, :3000 Console frontend.
pub fn local_oauth_config(
    api_base: Option<&str>,
    apps_base: Option<&str>,
    console_base: Option<&str>,
) -> OauthConfig {
    let api = api_base
        .unwrap_or("http://localhost:8000")
        .trim_end_matches('/');
    let apps = apps_base
        .unwrap_or("http://localhost:4000")
        .trim_end_matches('/');
    let console = console_base
        .unwrap_or("http://localhost:3000")
        .trim_end_matches('/');

    OauthConfig {
        base_api_url: api.to_string(),
        console_authorize_url: format!("{}/oauth/authorize", console),
        hosted_authorize_url: format!("{}/oauth/authorize", apps),
        hosted_origin: apps.to_string(),
        token_url: format!("{}/v1/oauth/token", api),
        api_key_url: format!("{}/api/oauth/mossen_cli/create_api_key", api),
        roles_url: format!("{}/api/oauth/mossen_cli/roles", api),
        console_success_url: format!(
            "{}/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dmossen-code",
            console
        ),
        hosted_success_url: format!("{}/oauth/code/success?app=mossen-code", console),
        manual_redirect_url: format!("{}/oauth/code/callback", console),
        client_id: "22422756-60c9-4084-8eb7-27705fd5cf9a".to_string(),
        oauth_file_suffix: "-local-oauth".to_string(),
        mcp_proxy_url: "http://localhost:8205".to_string(),
        mcp_proxy_path: "/v1/toolbox/shttp/mcp/{server_id}".to_string(),
    }
}

/// Allowed base URLs for MOSSEN_CODE_CUSTOM_OAUTH_URL override.
/// Only FedStart/PubSec deployments are permitted to prevent OAuth tokens
/// from being sent to arbitrary endpoints.
pub const ALLOWED_OAUTH_BASE_URLS: &[&str] = &[
    "https://beacon.staging.mossen.invalid",
    "https://mossen.fedstart.invalid",
    "https://mossen-staging.fedstart.invalid",
];

/// Get the full OAuth config, applying any overrides.
/// Default to prod config; use Mossen-named env flags for local/staging.
pub fn get_oauth_config(
    config_type: OauthConfigType,
    custom_oauth_url: Option<&str>,
    client_id_override: Option<&str>,
    local_api_base: Option<&str>,
    local_apps_base: Option<&str>,
    local_console_base: Option<&str>,
) -> Result<OauthConfig, String> {
    let mut config = match config_type {
        OauthConfigType::Local => {
            local_oauth_config(local_api_base, local_apps_base, local_console_base)
        }
        OauthConfigType::Staging => staging_oauth_config(),
        OauthConfigType::Prod => prod_oauth_config(),
    };

    // Allow overriding all OAuth URLs to point to an approved FedStart deployment.
    // Only allowlisted base URLs are accepted to prevent credential leakage.
    if let Some(oauth_base_url) = custom_oauth_url {
        let base = oauth_base_url.trim_end_matches('/');
        if !ALLOWED_OAUTH_BASE_URLS.contains(&base) {
            return Err("MOSSEN_CODE_CUSTOM_OAUTH_URL is not an approved endpoint.".to_string());
        }
        config.base_api_url = base.to_string();
        config.console_authorize_url = format!("{}/oauth/authorize", base);
        config.hosted_authorize_url = format!("{}/oauth/authorize", base);
        config.hosted_origin = base.to_string();
        config.token_url = format!("{}/v1/oauth/token", base);
        config.api_key_url = format!("{}/api/oauth/mossen_cli/create_api_key", base);
        config.roles_url = format!("{}/api/oauth/mossen_cli/roles", base);
        config.console_success_url = format!("{}/oauth/code/success?app=mossen-code", base);
        config.hosted_success_url = format!("{}/oauth/code/success?app=mossen-code", base);
        config.manual_redirect_url = format!("{}/oauth/code/callback", base);
        config.oauth_file_suffix = "-custom-oauth".to_string();
    }

    // Allow CLIENT_ID override via environment variable (e.g., for Xcode integration)
    if let Some(cid) = client_id_override {
        config.client_id = cid.to_string();
    }

    Ok(config)
}
