//! Preflight connectivity checks — verify backend reachability at startup.
//!
//! Mirrors TS `utils/preflightChecks.tsx`. The TS file exports a React
//! `PreflightStep` component and the `PreflightCheckResult` type. The Rust
//! port translates the logic (endpoint discovery, status-code interpretation,
//! HTTP probe) into an async pipeline that returns a structured result.

use std::time::Duration;

/// Result of a preflight connectivity probe.
#[derive(Debug, Clone)]
pub struct PreflightCheckResult {
    pub success: bool,
    pub error: Option<String>,
    pub ssl_hint: Option<String>,
}

impl PreflightCheckResult {
    pub fn ok() -> Self {
        Self {
            success: true,
            error: None,
            ssl_hint: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            ssl_hint: None,
        }
    }

    pub fn failed_with_ssl(error: impl Into<String>, ssl_hint: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error.into()),
            ssl_hint: Some(ssl_hint.into()),
        }
    }
}

/// Information about which endpoints should be probed and how.
#[derive(Debug, Clone)]
pub struct PreflightEndpoints {
    /// Whether the configured backend is a custom (BYO) backend.
    pub custom_backend: bool,
    /// URLs to probe.
    pub endpoints: Vec<String>,
    /// If set, the configuration itself is invalid — skip the probe and
    /// return this error directly.
    pub configuration_error: Option<String>,
}

/// Configuration knobs for endpoint discovery.
#[derive(Debug, Clone, Default)]
pub struct PreflightConfig {
    /// True when custom backend mode is enabled.
    pub is_custom_backend_enabled: bool,
    /// Custom backend base URL (when in custom backend mode).
    pub custom_backend_base_url: Option<String>,
    /// MOSSEN_CODE_API_BASE_URL value (for first-party path).
    pub mossen_api_base_url: Option<String>,
    /// OAuth `BASE_API_URL` value.
    pub oauth_base_api_url: Option<String>,
    /// OAuth `TOKEN_URL` value (used to derive `/v1/oauth/hello`).
    pub oauth_token_url: Option<String>,
    /// User-Agent header to send with the probe.
    pub user_agent: String,
    /// Optional extra headers (used for custom-backend auth).
    pub extra_headers: Vec<(String, String)>,
    /// Display name of the custom backend (for error messages).
    pub custom_backend_name: String,
    /// Probe timeout. Defaults to 5 seconds.
    pub timeout: Option<Duration>,
}

/// Compute the set of endpoints to probe.
///
/// Mirrors the TS `getPreflightEndpoints` helper, including its two
/// localized configuration-error messages.
pub fn get_preflight_endpoints(config: &PreflightConfig) -> PreflightEndpoints {
    // Custom backend with explicit URL — probe just that URL.
    if config.is_custom_backend_enabled {
        if let Some(url) = config.custom_backend_base_url.clone() {
            return PreflightEndpoints {
                custom_backend: true,
                endpoints: vec![url],
                configuration_error: None,
            };
        }
        return PreflightEndpoints {
            custom_backend: true,
            endpoints: Vec::new(),
            configuration_error: Some(
                "Custom backend is enabled, but MOSSEN_CODE_CUSTOM_BASE_URL is not set."
                    .to_string(),
            ),
        };
    }

    if config.mossen_api_base_url.is_none() {
        return PreflightEndpoints {
            custom_backend: false,
            endpoints: Vec::new(),
            configuration_error: Some(
                "No Mossen backend is configured. For personal edition, set MOSSEN_CODE_CUSTOM_BASE_URL and MOSSEN_CODE_CUSTOM_API_KEY (or MOSSEN_CODE_CUSTOM_AUTH_TOKEN) before starting Mossen."
                    .to_string(),
            ),
        };
    }

    let oauth_base = config.oauth_base_api_url.clone().unwrap_or_default();
    let token_origin = config
        .oauth_token_url
        .as_deref()
        .and_then(extract_origin)
        .unwrap_or_default();

    PreflightEndpoints {
        custom_backend: false,
        endpoints: vec![
            format!("{}/api/hello", oauth_base),
            format!("{}/v1/oauth/hello", token_origin),
        ],
        configuration_error: None,
    }
}

/// Extract scheme://host[:port] from a URL string.
fn extract_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let after_scheme = scheme_end + 3;
    let rest = &url[after_scheme..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    Some(format!("{}{}", &url[..after_scheme], &rest[..host_end]))
}

/// Decide whether an HTTP status code counts as "reachable".
///
/// Custom backends: anything below 500 (the bare base URL of an
/// OpenAI-compatible provider commonly returns 401/404/405 — still proves
/// the route).
/// First-party: only 200.
pub fn is_reachable_status(status: u16, custom_backend: bool) -> bool {
    if custom_backend {
        status < 500
    } else {
        status == 200
    }
}

/// Format a preflight failure message in the same shape as the TS helper.
pub fn format_preflight_error(url: &str, detail: &str, config: &PreflightConfig) -> String {
    let hostname = extract_hostname(url).unwrap_or_else(|| url.to_string());
    if config.is_custom_backend_enabled {
        format!(
            "Failed to connect to {} at {}: {}",
            config.custom_backend_name, hostname, detail
        )
    } else {
        format!("Failed to connect to {}: {}", hostname, detail)
    }
}

fn extract_hostname(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let host_end = rest.find(['/', ':', '?', '#']).unwrap_or(rest.len());
    Some(rest[..host_end].to_string())
}

/// Run preflight checks against the configured endpoints.
///
/// Returns the first failure (if any) or success. Mirrors the TS
/// `checkEndpoints` helper. Uses `reqwest` because that's the HTTP client
/// already in the workspace deps.
pub async fn check_endpoints(config: &PreflightConfig) -> PreflightCheckResult {
    let endpoints_info = get_preflight_endpoints(config);
    if let Some(err) = endpoints_info.configuration_error {
        return PreflightCheckResult::failed(err);
    }

    let timeout = config.timeout.unwrap_or_else(|| Duration::from_secs(5));
    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => return PreflightCheckResult::failed(format!("Connectivity check error: {e}")),
    };

    for url in &endpoints_info.endpoints {
        let mut req = client.get(url).header("User-Agent", &config.user_agent);
        if endpoints_info.custom_backend {
            for (k, v) in &config.extra_headers {
                req = req.header(k.as_str(), v.as_str());
            }
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if !is_reachable_status(status, endpoints_info.custom_backend) {
                    return PreflightCheckResult::failed(format_preflight_error(
                        url,
                        &format!("Status {status}"),
                        config,
                    ));
                }
            }
            Err(e) => {
                let detail = e.to_string();
                let ssl_hint = if detail.to_lowercase().contains("cert")
                    || detail.to_lowercase().contains("tls")
                    || detail.to_lowercase().contains("ssl")
                {
                    Some(
                        "TLS handshake failed — check your CA certs (NODE_EXTRA_CA_CERTS / MOSSEN_CODE_CLIENT_CERT) or corporate proxy."
                            .to_string(),
                    )
                } else {
                    None
                };
                let error = format_preflight_error(url, &detail, config);
                if let Some(hint) = ssl_hint {
                    return PreflightCheckResult::failed_with_ssl(error, hint);
                }
                return PreflightCheckResult::failed(error);
            }
        }
    }

    PreflightCheckResult::ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_custom_backend_with_url() {
        let cfg = PreflightConfig {
            is_custom_backend_enabled: true,
            custom_backend_base_url: Some("https://example.com".into()),
            ..Default::default()
        };
        let ep = get_preflight_endpoints(&cfg);
        assert!(ep.custom_backend);
        assert_eq!(ep.endpoints, vec!["https://example.com".to_string()]);
        assert!(ep.configuration_error.is_none());
    }

    #[test]
    fn endpoints_custom_backend_missing_url() {
        let cfg = PreflightConfig {
            is_custom_backend_enabled: true,
            ..Default::default()
        };
        let ep = get_preflight_endpoints(&cfg);
        assert!(ep.endpoints.is_empty());
        assert!(ep.configuration_error.is_some());
    }

    #[test]
    fn endpoints_custom_backend_missing_url_points_to_local_config() {
        let cfg = PreflightConfig {
            is_custom_backend_enabled: true,
            ..Default::default()
        };
        let ep = get_preflight_endpoints(&cfg);
        let message = ep
            .configuration_error
            .expect("missing custom backend URL should produce a configuration error");

        assert!(message.contains("MOSSEN_CODE_CUSTOM_BASE_URL"));
        assert!(!message.contains("api.mossen.invalid"));
        assert!(!message.contains("mossen.ai/login"));
        assert!(!message.contains("Please login"));
    }

    #[test]
    fn endpoints_no_mossen_api_base() {
        let cfg = PreflightConfig::default();
        let ep = get_preflight_endpoints(&cfg);
        assert!(ep.configuration_error.is_some());
    }

    #[test]
    fn endpoints_no_backend_points_to_personal_config_without_hosted_login() {
        let cfg = PreflightConfig::default();
        let ep = get_preflight_endpoints(&cfg);
        let message = ep
            .configuration_error
            .expect("missing backend should produce a configuration error");

        assert!(message.contains("MOSSEN_CODE_CUSTOM_BASE_URL"));
        assert!(message.contains("MOSSEN_CODE_CUSTOM_API_KEY"));
        assert!(message.contains("MOSSEN_CODE_CUSTOM_AUTH_TOKEN"));
        assert!(!message.contains("api.mossen.invalid"));
        assert!(!message.contains("mossen.ai/login"));
        assert!(!message.contains("Please login"));
    }

    #[test]
    fn reachable_status_logic() {
        assert!(is_reachable_status(200, false));
        assert!(!is_reachable_status(404, false));
        assert!(is_reachable_status(404, true));
        assert!(!is_reachable_status(500, true));
    }

    #[test]
    fn extract_origin_basic() {
        assert_eq!(
            extract_origin("https://example.com/v1/oauth/token"),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn extract_hostname_basic() {
        assert_eq!(
            extract_hostname("https://example.com:443/api"),
            Some("example.com".to_string())
        );
    }
}

/// 对应 TS `PreflightStep`：单个 preflight 步骤的描述结构。
#[derive(Debug, Clone)]
pub struct PreflightStep {
    pub label: String,
    pub status: PreflightStepStatus,
    pub message: Option<String>,
}

/// preflight 步骤的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightStepStatus {
    Pending,
    Running,
    Success,
    Failure,
}
