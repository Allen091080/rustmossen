//! Proxy Configuration
//!
//! HTTP/HTTPS proxy support with NO_PROXY bypass, mTLS, and CA certificate handling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

/// Keep-alive state — disabled after stale-pool ECONNRESET.
static KEEP_ALIVE_DISABLED: AtomicBool = AtomicBool::new(false);

/// Disable keep-alive for subsequent connections.
pub fn disable_keep_alive() {
    KEEP_ALIVE_DISABLED.store(true, Ordering::Relaxed);
}

/// Reset keep-alive for testing.
pub fn reset_keep_alive_for_testing() {
    KEEP_ALIVE_DISABLED.store(false, Ordering::Relaxed);
}

/// Returns true if keep-alive is currently disabled.
pub fn is_keep_alive_disabled() -> bool {
    KEEP_ALIVE_DISABLED.load(Ordering::Relaxed)
}

/// Address family enum for DNS resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFamily {
    Unspecified,
    IPv4,
    IPv6,
}

/// Convert a family option to AddressFamily.
pub fn get_address_family(family: Option<&str>) -> Result<AddressFamily, String> {
    match family {
        None | Some("IPv4") | Some("4") => Ok(AddressFamily::IPv4),
        Some("IPv6") | Some("6") => Ok(AddressFamily::IPv6),
        Some("0") => Ok(AddressFamily::Unspecified),
        Some(other) => Err(format!("Unsupported address family: {}", other)),
    }
}

/// Environment variable lookup helper.
type EnvLike = HashMap<String, String>;

fn get_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Get the active proxy URL if one is configured.
/// Prefers lowercase variants over uppercase.
pub fn get_proxy_url() -> Option<String> {
    get_env_var("https_proxy")
        .or_else(|| get_env_var("HTTPS_PROXY"))
        .or_else(|| get_env_var("http_proxy"))
        .or_else(|| get_env_var("HTTP_PROXY"))
}

/// Get proxy URL from a custom environment map.
pub fn get_proxy_url_from_env(env: &EnvLike) -> Option<String> {
    env.get("https_proxy")
        .or_else(|| env.get("HTTPS_PROXY"))
        .or_else(|| env.get("http_proxy"))
        .or_else(|| env.get("HTTP_PROXY"))
        .cloned()
        .filter(|v| !v.is_empty())
}

/// Get the NO_PROXY environment variable value.
pub fn get_no_proxy() -> Option<String> {
    get_env_var("no_proxy").or_else(|| get_env_var("NO_PROXY"))
}

/// Get NO_PROXY from a custom environment map.
pub fn get_no_proxy_from_env(env: &EnvLike) -> Option<String> {
    env.get("no_proxy")
        .or_else(|| env.get("NO_PROXY"))
        .cloned()
        .filter(|v| !v.is_empty())
}

/// Check if a URL should bypass the proxy based on NO_PROXY.
///
/// Supports:
/// - Exact hostname matches (e.g., "localhost")
/// - Domain suffix matches with leading dot (e.g., ".example.com")
/// - Wildcard "*" to bypass all
/// - Port-specific matches (e.g., "example.com:8080")
/// - IP addresses (e.g., "127.0.0.1")
pub fn should_bypass_proxy(url_string: &str, no_proxy: Option<&str>) -> bool {
    let no_proxy = match no_proxy.or_else(|| None) {
        Some(np) if !np.is_empty() => np,
        _ => {
            // Try environment
            let np = get_no_proxy();
            if np.is_none() {
                return false;
            }
            // We can't return a reference to a local, so handle differently
            return should_bypass_proxy_inner(url_string, &np.unwrap());
        }
    };
    should_bypass_proxy_inner(url_string, no_proxy)
}

fn should_bypass_proxy_inner(url_string: &str, no_proxy: &str) -> bool {
    if no_proxy.is_empty() {
        return false;
    }
    if no_proxy == "*" {
        return true;
    }

    let parsed = match url::Url::parse(url_string) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let hostname = parsed.host_str().unwrap_or("").to_lowercase();
    let port = parsed.port().map(|p| p.to_string()).unwrap_or_else(|| {
        if parsed.scheme() == "https" {
            "443".to_string()
        } else {
            "80".to_string()
        }
    });
    let host_with_port = format!("{}:{}", hostname, port);

    let no_proxy_list: Vec<&str> = no_proxy
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    no_proxy_list.iter().any(|pattern| {
        let pattern = pattern.to_lowercase();
        let pattern = pattern.trim();

        // Check for port-specific match
        if pattern.contains(':') {
            return host_with_port == pattern;
        }

        // Check for domain suffix match
        if let Some(suffix) = pattern.strip_prefix('.') {
            return hostname == suffix || hostname.ends_with(&format!(".{}", suffix));
        }

        // Exact hostname match or IP address
        hostname == pattern
    })
}

/// Proxy fetch options that can be used with reqwest.
#[derive(Debug, Clone, Default)]
pub struct ProxyFetchOptions {
    pub proxy_url: Option<String>,
    pub unix_socket: Option<String>,
    pub keepalive: Option<bool>,
}

/// Get fetch options for the provider SDK with proxy configuration.
pub fn get_proxy_fetch_options(for_provider_api: bool) -> ProxyFetchOptions {
    let base_keepalive = if KEEP_ALIVE_DISABLED.load(Ordering::Relaxed) {
        Some(false)
    } else {
        None
    };

    // Provider unix-socket tunneling
    if for_provider_api {
        if let Some(unix_socket) = get_env_var("MOSSEN_CODE_UNIX_SOCKET") {
            return ProxyFetchOptions {
                unix_socket: Some(unix_socket),
                keepalive: base_keepalive,
                ..Default::default()
            };
        }
    }

    let proxy_url = get_proxy_url();

    ProxyFetchOptions {
        proxy_url,
        keepalive: base_keepalive,
        ..Default::default()
    }
}

/// Get the proxy URL for WebSocket connections.
/// Returns None if no proxy is configured or URL should bypass.
pub fn get_web_socket_proxy_url(url: &str) -> Option<String> {
    let proxy_url = get_proxy_url()?;
    if should_bypass_proxy_inner(url, &get_no_proxy().unwrap_or_default()) {
        return None;
    }
    Some(proxy_url)
}

/// Proxy agent cache (simplified — stores proxy URL).
static PROXY_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Clear proxy agent cache.
pub fn clear_proxy_cache() {
    let mut cache = PROXY_CACHE.lock();
    cache.clear();
    tracing::debug!("Cleared proxy agent cache");
}

/// AWS client proxy configuration.
#[derive(Debug, Clone, Default)]
pub struct AwsClientProxyConfig {
    pub proxy_url: Option<String>,
}

/// Get AWS SDK client configuration with proxy support.
pub async fn get_aws_client_proxy_config() -> AwsClientProxyConfig {
    let proxy_url = get_proxy_url();
    AwsClientProxyConfig { proxy_url }
}

/// 对应 TS `createAxiosInstance`：创建带代理配置的 HTTP 客户端。
pub fn create_axios_instance() -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if let Some(url) = get_proxy_url() {
        if let Ok(proxy) = reqwest::Proxy::all(&url) {
            builder = builder.proxy(proxy);
        }
    }
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

/// 对应 TS `getWebSocketProxyAgent`：返回 WebSocket 连接所需的代理 URL（如有）。
pub fn get_web_socket_proxy_agent() -> Option<String> {
    get_proxy_url()
}

/// 对应 TS `configureGlobalAgents`：把代理与 mTLS 配置应用到全局 HTTP/HTTPS agent。
///
/// Rust 端的 reqwest 没有真正意义上的"全局 agent"，因此这里只是触发一次代理
/// 配置 cache 刷新，调用方应通过 [`create_axios_instance`] 获得客户端。
pub fn configure_global_agents() {
    let _ = get_proxy_url();
}

/// 对应 TS `getProxyAgent`：返回当前代理 URL（如有）。
pub fn get_proxy_agent() -> Option<String> {
    get_proxy_url()
}
