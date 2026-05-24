//! # xaa — XAA (Cross-App Access) 协议
//!
//! 对应 TypeScript `services/mcp/xaa.ts`。XAA 把单点登录的 IdP id_token
//! 通过 RFC 8693 token-exchange 转换为可访问 MCP server 的 ID-JAG，再用
//! RFC 7523 JWT Bearer 在 AS 处换出 access_token。
//!
//! Rust 端不直接执行 HTTP（避免硬绑定 reqwest 客户端到 mcp crate），而是
//! 把每一步的 “请求构造” 与 “响应解析” 暴露为纯函数，再通过 `fetch_fn`
//! 注入实际的 HTTP。

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

pub const TOKEN_EXCHANGE_GRANT: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
pub const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
pub const ID_JAG_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id-jag";
pub const ID_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id_token";

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// `xaa.ts` `XaaTokenExchangeError`。
#[derive(Debug, Clone, thiserror::Error)]
#[error("XAA: {message}")]
pub struct XaaTokenExchangeError {
    pub message: String,
    /// 若为 true，调用方应当清空 id_token 缓存。
    pub should_clear_cache: bool,
}

impl XaaTokenExchangeError {
    pub fn new(message: impl Into<String>, should_clear_cache: bool) -> Self {
        Self {
            message: message.into(),
            should_clear_cache,
        }
    }
}

// ---------------------------------------------------------------------------
// PRM / AS metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
}

/// `xaa.ts` `discoverProtectedResource` 的纯函数形态。
///
/// 调用方注入 `prm_fetch`，本函数负责：URL 验证、resource-mismatch 检查
/// （RFC 9728 §3.3）、缺字段错误。
pub async fn discover_protected_resource<F, Fut>(
    server_url: &str,
    prm_fetch: F,
) -> Result<ProtectedResourceMetadata, String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    let raw = prm_fetch(server_url.to_string())
        .await
        .map_err(|e| format!("XAA: PRM discovery failed: {}", e))?;
    let resource = raw
        .get("resource")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            "XAA: PRM discovery failed: PRM missing resource or authorization_servers".to_string()
        })?
        .to_string();
    let auth_servers: Vec<String> = raw
        .get("authorization_servers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if auth_servers.is_empty() {
        return Err(
            "XAA: PRM discovery failed: PRM missing resource or authorization_servers".into(),
        );
    }
    if normalize_url(&resource) != normalize_url(server_url) {
        return Err(format!(
            "XAA: PRM discovery failed: PRM resource mismatch: expected {}, got {}",
            server_url, resource
        ));
    }
    Ok(ProtectedResourceMetadata {
        resource,
        authorization_servers: auth_servers,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

/// `xaa.ts` `discoverAuthorizationServer`。
pub async fn discover_authorization_server<F, Fut>(
    as_url: &str,
    metadata_fetch: F,
) -> Result<AuthorizationServerMetadata, String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    let meta = metadata_fetch(as_url.to_string())
        .await
        .map_err(|e| format!("XAA: AS metadata discovery failed: {}", e))?;
    let issuer = meta
        .get("issuer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "XAA: AS metadata discovery failed: no valid metadata at {}",
                as_url
            )
        })?
        .to_string();
    let token_endpoint = meta
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            format!(
                "XAA: AS metadata discovery failed: no valid metadata at {}",
                as_url
            )
        })?
        .to_string();
    if normalize_url(&issuer) != normalize_url(as_url) {
        return Err(format!(
            "XAA: AS metadata discovery failed: issuer mismatch: expected {}, got {}",
            as_url, issuer
        ));
    }
    if !token_endpoint.starts_with("https://") {
        return Err(format!(
            "XAA: refusing non-HTTPS token endpoint: {}",
            token_endpoint
        ));
    }
    let grant_types = meta
        .get("grant_types_supported")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let auth_methods = meta
        .get("token_endpoint_auth_methods_supported")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(AuthorizationServerMetadata {
        issuer,
        token_endpoint,
        grant_types_supported: grant_types,
        token_endpoint_auth_methods_supported: auth_methods,
    })
}

// ---------------------------------------------------------------------------
// requestJwtAuthorizationGrant — Token exchange (id_token -> ID-JAG)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtAuthGrantResult {
    pub jwt_auth_grant: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequestJwtAuthGrantOpts {
    pub token_endpoint: String,
    pub audience: String,
    pub resource: String,
    pub id_token: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
}

/// `xaa.ts` `requestJwtAuthorizationGrant`。
///
/// 调用方注入 `do_post`：(url, form_body, headers) -> (status, body)。
pub async fn request_jwt_authorization_grant<F, Fut>(
    opts: RequestJwtAuthGrantOpts,
    do_post: F,
) -> Result<JwtAuthGrantResult, XaaTokenExchangeError>
where
    F: FnOnce(String, String) -> Fut,
    Fut: std::future::Future<Output = Result<(u16, String), String>>,
{
    let mut params: Vec<(String, String)> = vec![
        ("grant_type".into(), TOKEN_EXCHANGE_GRANT.into()),
        ("requested_token_type".into(), ID_JAG_TOKEN_TYPE.into()),
        ("audience".into(), opts.audience),
        ("resource".into(), opts.resource),
        ("subject_token".into(), opts.id_token),
        ("subject_token_type".into(), ID_TOKEN_TYPE.into()),
        ("client_id".into(), opts.client_id),
    ];
    if let Some(s) = opts.client_secret {
        params.push(("client_secret".into(), s));
    }
    if let Some(s) = opts.scope {
        params.push(("scope".into(), s));
    }

    let body = form_urlencoded_serialize(&params);
    let (status, body) = do_post(opts.token_endpoint, body)
        .await
        .map_err(|e| XaaTokenExchangeError::new(e, false))?;

    if !(200..300).contains(&status) {
        // 4xx -> clear cache, 5xx -> preserve
        let should_clear = status < 500;
        return Err(XaaTokenExchangeError::new(
            format!(
                "XAA: token exchange failed: HTTP {}: {}",
                status,
                truncate(&body, 200)
            ),
            should_clear,
        ));
    }

    let parsed: JsonValue = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            return Err(XaaTokenExchangeError::new(
                "XAA: token exchange returned non-JSON (captive portal?)".to_string(),
                false,
            ))
        }
    };
    let access_token = parsed
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            XaaTokenExchangeError::new(
                "XAA: token exchange response missing access_token".to_string(),
                true,
            )
        })?;
    let issued_type = parsed
        .get("issued_token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if issued_type != ID_JAG_TOKEN_TYPE {
        return Err(XaaTokenExchangeError::new(
            format!(
                "XAA: token exchange returned unexpected issued_token_type: {}",
                issued_type
            ),
            true,
        ));
    }
    let expires_in = parsed.get("expires_in").and_then(|v| v.as_u64());
    let scope = parsed
        .get("scope")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok(JwtAuthGrantResult {
        jwt_auth_grant: access_token.to_string(),
        expires_in,
        scope,
    })
}

// ---------------------------------------------------------------------------
// exchangeJwtAuthGrant — JWT Bearer (ID-JAG -> access_token)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaaTokenResult {
    pub access_token: String,
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaaResult {
    #[serde(flatten)]
    pub token: XaaTokenResult,
    /// AS 发现路径上的 issuer URL。
    pub authorization_server_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XaaAuthMethod {
    ClientSecretBasic,
    ClientSecretPost,
}

#[derive(Debug, Clone)]
pub struct ExchangeJwtAuthGrantOpts {
    pub token_endpoint: String,
    pub assertion: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_method: XaaAuthMethod,
    pub scope: Option<String>,
}

/// `xaa.ts` `exchangeJwtAuthGrant`。
pub async fn exchange_jwt_auth_grant<F, Fut>(
    opts: ExchangeJwtAuthGrantOpts,
    do_post: F,
) -> Result<XaaTokenResult, XaaTokenExchangeError>
where
    F: FnOnce(String, String, Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<(u16, String), String>>,
{
    let mut params: Vec<(String, String)> = vec![
        ("grant_type".into(), JWT_BEARER_GRANT.into()),
        ("assertion".into(), opts.assertion.clone()),
    ];
    if let Some(s) = opts.scope {
        params.push(("scope".into(), s));
    }

    let mut auth_header: Option<String> = None;
    match opts.auth_method {
        XaaAuthMethod::ClientSecretBasic => {
            let raw = format!(
                "{}:{}",
                url_encode(&opts.client_id),
                url_encode(&opts.client_secret)
            );
            auth_header = Some(format!("Basic {}", base64_encode(raw.as_bytes())));
        }
        XaaAuthMethod::ClientSecretPost => {
            params.push(("client_id".into(), opts.client_id));
            params.push(("client_secret".into(), opts.client_secret));
        }
    }

    let body = form_urlencoded_serialize(&params);
    let (status, body) = do_post(opts.token_endpoint, body, auth_header)
        .await
        .map_err(|e| XaaTokenExchangeError::new(e, false))?;

    if !(200..300).contains(&status) {
        let should_clear = status < 500;
        return Err(XaaTokenExchangeError::new(
            format!(
                "XAA: JWT bearer exchange failed: HTTP {}: {}",
                status,
                truncate(&body, 200)
            ),
            should_clear,
        ));
    }

    let parsed: JsonValue = serde_json::from_str(&body).map_err(|_| {
        XaaTokenExchangeError::new("XAA: JWT bearer response was not JSON".to_string(), false)
    })?;
    let access_token = parsed
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            XaaTokenExchangeError::new(
                "XAA: JWT bearer response missing access_token".to_string(),
                true,
            )
        })?
        .to_string();
    let token_type = parsed
        .get("token_type")
        .and_then(|v| v.as_str())
        .unwrap_or("Bearer")
        .to_string();
    Ok(XaaTokenResult {
        access_token,
        token_type,
        expires_in: parsed.get("expires_in").and_then(|v| v.as_u64()),
        scope: parsed
            .get("scope")
            .and_then(|v| v.as_str())
            .map(String::from),
        refresh_token: parsed
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

// ---------------------------------------------------------------------------
// performCrossAppAccess — 编排
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct XaaConfig {
    pub server_url: String,
    pub idp_issuer: String,
    pub idp_token_endpoint: String,
    pub idp_client_id: String,
    pub idp_client_secret: Option<String>,
    pub as_client_id: String,
    pub as_client_secret: String,
    pub auth_method: XaaAuthMethod,
    pub id_token: String,
    pub scope: Option<String>,
}

/// `xaa.ts` `performCrossAppAccess`。
///
/// 用注入的两个 do_post（IdP + AS）一次完成 PRM + token-exchange + JWT-bearer
/// 的完整流程，返回最终 access_token + AS issuer URL。
pub async fn perform_cross_app_access<F1, Fut1, F2, Fut2, F3, Fut3>(
    cfg: XaaConfig,
    prm_fetch: F1,
    do_idp_post: F2,
    do_as_post: F3,
) -> Result<XaaResult, XaaTokenExchangeError>
where
    F1: FnOnce(String) -> Fut1,
    Fut1: std::future::Future<Output = Result<JsonValue, String>>,
    F2: FnOnce(String, String) -> Fut2,
    Fut2: std::future::Future<Output = Result<(u16, String), String>>,
    F3: FnOnce(String, String, Option<String>) -> Fut3,
    Fut3: std::future::Future<Output = Result<(u16, String), String>>,
{
    let prm = discover_protected_resource(&cfg.server_url, prm_fetch)
        .await
        .map_err(|m| XaaTokenExchangeError::new(m, false))?;
    let authorization_server_url = prm.authorization_servers.first().cloned().ok_or_else(|| {
        XaaTokenExchangeError::new("PRM yielded no authorization_servers".to_string(), false)
    })?;

    let grant = request_jwt_authorization_grant(
        RequestJwtAuthGrantOpts {
            token_endpoint: cfg.idp_token_endpoint.clone(),
            audience: authorization_server_url.clone(),
            resource: cfg.server_url.clone(),
            id_token: cfg.id_token.clone(),
            client_id: cfg.idp_client_id.clone(),
            client_secret: cfg.idp_client_secret.clone(),
            scope: cfg.scope.clone(),
        },
        do_idp_post,
    )
    .await?;

    let token = exchange_jwt_auth_grant(
        ExchangeJwtAuthGrantOpts {
            token_endpoint: cfg.idp_token_endpoint.clone(),
            assertion: grant.jwt_auth_grant,
            client_id: cfg.as_client_id,
            client_secret: cfg.as_client_secret,
            auth_method: cfg.auth_method,
            scope: cfg.scope,
        },
        do_as_post,
    )
    .await?;

    Ok(XaaResult {
        token,
        authorization_server_url,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip trailing slash + lower-case scheme/host for issuer comparisons.
fn normalize_url(s: &str) -> String {
    let trimmed = s.trim_end_matches('/').to_string();
    trimmed.to_lowercase()
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn form_urlencoded_serialize(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHA[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_slash_and_case() {
        assert_eq!(normalize_url("HTTPS://EX/"), "https://ex");
        assert_eq!(normalize_url("https://ex"), "https://ex");
    }

    #[test]
    fn form_serialize_escapes() {
        let s = form_urlencoded_serialize(&[("a".into(), "b c".into())]);
        assert_eq!(s, "a=b%20c");
    }

    #[test]
    fn base64_round_trip_short() {
        assert_eq!(base64_encode(b"abc"), "YWJj");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"a"), "YQ==");
    }
}
