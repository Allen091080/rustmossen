//! # exec_http — HTTP Hook 执行器
//!
//! 对应 TS `utils/hooks/execHttpHook.ts`。
//! 执行 HTTP POST 请求，支持环境变量插值、SSRF 防护。

use std::collections::HashMap;
use std::time::Duration;

use regex::Regex;
use tracing::{debug, warn};

use super::ssrf_guard;

/// HTTP Hook 默认超时（10 分钟）。
const DEFAULT_HTTP_HOOK_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// HTTP Hook 执行结果。
#[derive(Debug, Clone)]
pub struct HttpHookResult {
    /// 是否成功。
    pub ok: bool,
    /// HTTP 状态码。
    pub status_code: Option<u16>,
    /// 响应体。
    pub body: String,
    /// 错误消息。
    pub error: Option<String>,
    /// 是否被中止。
    pub aborted: bool,
}

/// HTTP Hook 策略配置。
#[derive(Debug, Clone, Default)]
pub struct HttpHookPolicy {
    /// 允许的 URL 模式列表。
    pub allowed_urls: Option<Vec<String>>,
    /// 允许的环境变量列表。
    pub allowed_env_vars: Option<Vec<String>>,
}

/// 匹配 URL 模式（* 作为通配符）。
///
/// 对应 TS `urlMatchesPattern()`。
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern).replace(r"\*", ".*");
    let regex_str = format!("^{escaped}$");
    Regex::new(&regex_str)
        .map(|re| re.is_match(url))
        .unwrap_or(false)
}

/// 清理 header 值（移除 CR/LF/NUL 字节防止头注入）。
///
/// 对应 TS `sanitizeHeaderValue()`。
fn sanitize_header_value(value: &str) -> String {
    value
        .chars()
        .filter(|&c| c != '\r' && c != '\n' && c != '\0')
        .collect()
}

/// 插值环境变量（$VAR_NAME 和 ${VAR_NAME} 模式）。
///
/// 对应 TS `interpolateEnvVars()`。
fn interpolate_env_vars(
    value: &str,
    allowed_env_vars: &std::collections::HashSet<String>,
) -> String {
    let re = Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)\}|\$([A-Z_][A-Z0-9_]*)").unwrap();
    let result = re.replace_all(value, |caps: &regex::Captures| {
        let var_name = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");

        if !allowed_env_vars.contains(var_name) {
            debug!(var = var_name, "Env var not in allowlist, skipping");
            return String::new();
        }

        std::env::var(var_name).unwrap_or_default()
    });

    sanitize_header_value(&result)
}

/// 执行 HTTP Hook。
///
/// 对应 TS `execHttpHook()`。
pub async fn exec_http_hook(
    url: &str,
    headers: Option<&HashMap<String, String>>,
    allowed_env_vars: Option<&[String]>,
    json_input: &str,
    timeout_secs: Option<f64>,
    policy: &HttpHookPolicy,
) -> HttpHookResult {
    // Guard: 检查 URL 白名单
    if let Some(ref allowed_urls) = policy.allowed_urls {
        let matched = allowed_urls.iter().any(|p| url_matches_pattern(url, p));
        if !matched {
            let msg = format!(
                "HTTP hook blocked: {url} does not match any pattern in allowedHttpHookUrls"
            );
            warn!("{}", msg);
            return HttpHookResult {
                ok: false,
                status_code: None,
                body: String::new(),
                error: Some(msg),
                aborted: false,
            };
        }
    }

    // Guard: SSRF 检查（对 URL 中的主机名进行验证）
    if let Ok(parsed_url) = url::Url::parse(url) {
        if let Some(host) = parsed_url.host_str() {
            if ssrf_guard::is_blocked_address(host) {
                let err = ssrf_guard::ssrf_error(host, host);
                return HttpHookResult {
                    ok: false,
                    status_code: None,
                    body: String::new(),
                    error: Some(err.message),
                    aborted: false,
                };
            }
        }
    }

    let timeout_ms = timeout_secs
        .map(|s| (s * 1000.0) as u64)
        .unwrap_or(DEFAULT_HTTP_HOOK_TIMEOUT_MS);

    debug!(url = url, timeout_ms = timeout_ms, "Executing HTTP hook");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .unwrap_or_default();

    // 构建请求头
    let mut request_headers = HashMap::new();
    request_headers.insert("Content-Type".to_string(), "application/json".to_string());

    if let Some(hdrs) = headers {
        let hook_vars: Vec<String> = allowed_env_vars.map(|v| v.to_vec()).unwrap_or_default();
        let effective_vars: std::collections::HashSet<String> = match &policy.allowed_env_vars {
            Some(policy_vars) => hook_vars
                .into_iter()
                .filter(|v| policy_vars.contains(v))
                .collect(),
            None => hook_vars.into_iter().collect(),
        };

        for (name, value) in hdrs {
            request_headers.insert(name.clone(), interpolate_env_vars(value, &effective_vars));
        }
    }

    let mut request = client.post(url).body(json_input.to_string());
    for (name, value) in &request_headers {
        request = request.header(name.as_str(), value.as_str());
    }

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let ok = status.is_success();

            debug!(
                url = url,
                status = status.as_u16(),
                body_len = body.len(),
                "HTTP hook response"
            );

            HttpHookResult {
                ok,
                status_code: Some(status.as_u16()),
                body,
                error: None,
                aborted: false,
            }
        }
        Err(e) => {
            if e.is_timeout() {
                return HttpHookResult {
                    ok: false,
                    status_code: None,
                    body: String::new(),
                    error: None,
                    aborted: true,
                };
            }

            let error_msg = format!("HTTP hook error: {e}");
            warn!("{}", error_msg);
            HttpHookResult {
                ok: false,
                status_code: None,
                body: String::new(),
                error: Some(error_msg),
                aborted: false,
            }
        }
    }
}
