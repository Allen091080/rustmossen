//! # Product (product.ts)
//!
//! 产品名称、URL 和远程会话相关常量。

/// Default product URL for non-custom-backend mode.
pub const DEFAULT_PRODUCT_URL: &str = "https://mossen.invalid/code";

/// Get the product URL.
/// `is_custom_backend`: whether custom backend is enabled
/// `has_configured_platform_urls`: whether hosted platform URLs are configured
/// `remote_base_url`: the configured remote base URL for custom backend
pub fn get_product_url(
    is_custom_backend: bool,
    has_configured_platform_urls: bool,
    remote_base_url: &str,
) -> String {
    if is_custom_backend && has_configured_platform_urls {
        remote_base_url.to_string()
    } else {
        DEFAULT_PRODUCT_URL.to_string()
    }
}

pub fn get_product_display_name() -> &'static str {
    "Mossen"
}

pub fn get_product_assistant_name() -> &'static str {
    "Mossen"
}

pub fn get_product_welcome_message() -> String {
    format!("Welcome to {}", get_product_display_name())
}

pub fn get_product_cli_name() -> &'static str {
    "mossen"
}

pub fn get_project_instructions_display_name() -> &'static str {
    "MOSSEN.md"
}

pub fn get_product_config_dir_name() -> &'static str {
    ".mossen"
}

pub fn get_product_config_home_display_path() -> &'static str {
    "~/.mossen"
}

pub fn get_desktop_product_name() -> &'static str {
    "Mossen Desktop"
}

// Hosted remote session URLs backed by hosted surfaces
pub const HOSTED_BASE_URL_DEFAULT: &str = "https://hosted.mossen.invalid";
pub const HOSTED_STAGING_BASE_URL_DEFAULT: &str = "https://hosted-staging.mossen.invalid";
pub const HOSTED_LOCAL_BASE_URL_DEFAULT: &str = "http://localhost:4000";

/// Get hosted base URL from env or default.
pub fn get_hosted_base_url_env() -> String {
    std::env::var("MOSSEN_HOSTED_BASE_URL").unwrap_or_else(|_| HOSTED_BASE_URL_DEFAULT.to_string())
}

/// Get hosted staging base URL from env or default.
pub fn get_hosted_staging_base_url_env() -> String {
    std::env::var("MOSSEN_HOSTED_STAGING_BASE_URL")
        .unwrap_or_else(|_| HOSTED_STAGING_BASE_URL_DEFAULT.to_string())
}

/// Get hosted local base URL from env or default.
pub fn get_hosted_local_base_url_env() -> String {
    std::env::var("MOSSEN_HOSTED_LOCAL_BASE_URL")
        .unwrap_or_else(|_| HOSTED_LOCAL_BASE_URL_DEFAULT.to_string())
}

/// Determine if we're in a staging environment for remote sessions.
/// Checks session ID format and ingress URL.
pub fn is_remote_session_staging(session_id: Option<&str>, ingress_url: Option<&str>) -> bool {
    if let Some(sid) = session_id {
        if sid.contains("_staging_") {
            return true;
        }
    }
    if let Some(url) = ingress_url {
        if url.contains("staging") {
            return true;
        }
    }
    false
}

/// Determine if we're in a local-dev environment for remote sessions.
/// Checks session ID format (e.g. `session_local_...`) and ingress URL.
pub fn is_remote_session_local(session_id: Option<&str>, ingress_url: Option<&str>) -> bool {
    if let Some(sid) = session_id {
        if sid.contains("_local_") {
            return true;
        }
    }
    if let Some(url) = ingress_url {
        if url.contains("localhost") {
            return true;
        }
    }
    false
}

/// Get the base URL for the hosted runtime based on environment.
pub fn get_hosted_base_url(
    is_custom_backend: bool,
    remote_base_url: &str,
    session_id: Option<&str>,
    ingress_url: Option<&str>,
) -> String {
    if is_custom_backend {
        return remote_base_url.to_string();
    }
    if is_remote_session_local(session_id, ingress_url) {
        return get_hosted_local_base_url_env();
    }
    if is_remote_session_staging(session_id, ingress_url) {
        return get_hosted_staging_base_url_env();
    }
    get_hosted_base_url_env()
}

/// Get the full session URL for a remote session.
///
/// The cse_→session_ translation is a temporary shim gated by
/// tengu_bridge_repl_v2_cse_shim_enabled. Worker endpoints want `cse_*`
/// but the hosted frontend currently routes on `session_*`.
pub fn get_remote_session_url(
    session_id: &str,
    is_custom_backend: bool,
    remote_base_url: &str,
    ingress_url: Option<&str>,
) -> String {
    let base_url = get_hosted_base_url(is_custom_backend, remote_base_url, Some(session_id), ingress_url);
    format!("{}/code/{}", base_url, session_id)
}
