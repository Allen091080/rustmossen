//! # auth_ext — services/mcp/auth.ts 中尚未翻译的 OAuth 辅助函数
//!
//! 对应 TypeScript `services/mcp/auth.ts`。本模块仅承载补充项：
//! - `normalize_oauth_error_body` — RFC 6749 错误标准化；
//! - `get_server_key` — 服务器 + 配置的稳定哈希键；
//! - `has_mcp_discovery_but_no_token` — 已发现服务但无 token 的状态判定；
//! - `revoke_server_tokens` — 撤销远端 token；
//! - `clear_server_tokens_from_local_storage` — 清理本地 token；
//! - `wrap_fetch_with_step_up_detection` — step-up 重定向探测；
//! - `read_client_secret` / `save_mcp_client_secret` / `clear_mcp_client_config` /
//!   `get_mcp_client_config` — 客户端密钥的 CRUD；
//! - `AuthenticationCancelledError`、`MossenAuthProvider` — 类型与实现。
//!
//! Rust 端凭据存储用一个 `OnceLock<RwLock<HashMap>>` 模拟 TS 的安全存储
//! (`getSecureStorage().read()`)：调用方在测试或集成层注入更具体的后端。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::auth::OAuthTokens;

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// `auth.ts` `AuthenticationCancelledError`。
#[derive(Debug, Clone, thiserror::Error)]
#[error("Authentication cancelled: {message}")]
pub struct AuthenticationCancelledError {
    pub message: String,
}

impl AuthenticationCancelledError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// 内部凭据存储 — 与 TS `getSecureStorage` 等价的进程内实现
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpOAuthEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// 关联的 client_id / client_secret 等。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// 服务器发现端点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,
}

#[derive(Debug, Default)]
struct SecureStorage {
    /// `mcpOAuth[serverKey] -> entry`
    mcp_oauth: HashMap<String, McpOAuthEntry>,
    /// `mcpClientConfig[serverName] -> JsonValue`
    mcp_client_config: HashMap<String, JsonValue>,
}

static STORAGE: OnceLock<RwLock<SecureStorage>> = OnceLock::new();

fn storage() -> &'static RwLock<SecureStorage> {
    STORAGE.get_or_init(|| RwLock::new(SecureStorage::default()))
}

// ---------------------------------------------------------------------------
// normalizeOAuthErrorBody — 错误响应标准化
// ---------------------------------------------------------------------------

/// 非标准 `invalid_grant` 同义错误码列表（来自 TS `NONSTANDARD_INVALID_GRANT_ALIASES`）。
pub const NONSTANDARD_INVALID_GRANT_ALIASES: &[&str] = &[
    "expired_token",
    "refresh_token_expired",
    "invalid_token",
    "token_expired",
    "expired_refresh_token",
];

/// `auth.ts` `normalizeOAuthErrorBody`。
///
/// 输入是 HTTP 响应的 (status, body)：
/// - 2xx 直接透传；
/// - 否则尝试 JSON 解析；
/// - 若像 OAuth token 响应则透传；
/// - 若像 `{ error, error_description? }` 错误体，则把非标准 error code
///   标准化为 `invalid_grant`，返回新的 body 与 status=400。
///
/// 返回 `(status, body)` 二元组；body 是字符串。
pub fn normalize_oauth_error_body(status: u16, text: &str) -> (u16, String) {
    if (200..300).contains(&status) {
        return (status, text.to_string());
    }
    let parsed: Result<JsonValue, _> = serde_json::from_str(text);
    let Ok(parsed) = parsed else {
        return (status, text.to_string());
    };

    if looks_like_oauth_token(&parsed) {
        return (status, text.to_string());
    }

    let Some(err_code) = parsed.get("error").and_then(|v| v.as_str()) else {
        return (status, text.to_string());
    };

    let normalized = if NONSTANDARD_INVALID_GRANT_ALIASES
        .iter()
        .any(|a| *a == err_code)
    {
        let desc = parsed
            .get("error_description")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("Server returned non-standard error code: {}", err_code));
        json!({ "error": "invalid_grant", "error_description": desc })
    } else {
        parsed
    };
    (400, normalized.to_string())
}

fn looks_like_oauth_token(v: &JsonValue) -> bool {
    let has_access = v
        .get("access_token")
        .map(|t| t.is_string())
        .unwrap_or(false);
    let has_token_type = v.get("token_type").map(|t| t.is_string()).unwrap_or(false);
    has_access && has_token_type
}

// ---------------------------------------------------------------------------
// getServerKey / hasMcpDiscoveryButNoToken
// ---------------------------------------------------------------------------

/// `auth.ts` `getServerKey`。
///
/// 输入服务器名 + 配置 JSON（含 `type`, `url`, `headers`），输出
/// `<name>|<sha256(canonical)前16字符>`。
pub fn get_server_key(server_name: &str, server_config: &JsonValue) -> String {
    let canonical = json!({
        "type": server_config.get("type").cloned().unwrap_or(JsonValue::Null),
        "url": server_config.get("url").cloned().unwrap_or(JsonValue::Null),
        "headers": server_config.get("headers").cloned().unwrap_or(JsonValue::Object(serde_json::Map::new())),
    });
    let s = serde_json::to_string(&canonical).unwrap_or_default();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    let short: String = hex.chars().take(16).collect();
    format!("{}|{}", server_name, short)
}

/// `auth.ts` `hasMcpDiscoveryButNoToken`。
///
/// 输入服务器名 + 配置：若存储中存在该 server-key 的条目，但没有
/// access_token / refresh_token，返回 true。`xaa_enabled` 与
/// `config.oauth.xaa` 字段允许 XAA 服务跳过此判定（与 TS 一致）。
pub fn has_mcp_discovery_but_no_token(
    server_name: &str,
    server_config: &JsonValue,
    xaa_enabled: bool,
) -> bool {
    let xaa_oauth = server_config
        .get("oauth")
        .and_then(|o| o.get("xaa"))
        .is_some();
    if xaa_enabled && xaa_oauth {
        return false;
    }
    let key = get_server_key(server_name, server_config);
    let s = storage().read().unwrap();
    match s.mcp_oauth.get(&key) {
        None => false,
        Some(entry) => entry.access_token.is_none() && entry.refresh_token.is_none(),
    }
}

// ---------------------------------------------------------------------------
// revokeServerTokens / clearServerTokensFromLocalStorage
// ---------------------------------------------------------------------------

/// `auth.ts` `revokeServerTokens`。
///
/// 在存储中查找 server-key 的条目；若存在 revocation_endpoint 与 token
/// 则把请求体构造好后让调用方（注入的 `do_revoke`）真正发起 HTTP POST。
/// 这层只负责协议级流程；网络层放在 `mossen-utils::http` 里。
///
/// `preserve_step_up_state` 与 TS 一致：若为 true，仅清空 token 字段
/// 而保留发现状态。
pub async fn revoke_server_tokens<F, Fut>(
    server_name: &str,
    server_config: &JsonValue,
    preserve_step_up_state: bool,
    do_revoke: F,
) -> Result<(), String>
where
    F: Fn(RevokeRequest) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let key = get_server_key(server_name, server_config);
    let (entry, endpoint) = {
        let s = storage().read().unwrap();
        match s.mcp_oauth.get(&key).cloned() {
            None => return Ok(()),
            Some(e) => {
                let ep = e.revocation_endpoint.clone();
                (e, ep)
            }
        }
    };

    if let Some(endpoint) = endpoint {
        if let Some(refresh) = entry.refresh_token.clone() {
            do_revoke(RevokeRequest {
                server_name: server_name.to_string(),
                endpoint: endpoint.clone(),
                token: refresh,
                token_type_hint: "refresh_token".to_string(),
                client_id: entry.client_id.clone(),
                client_secret: entry.client_secret.clone(),
                access_token: entry.access_token.clone(),
            })
            .await?;
        }
        if let Some(access) = entry.access_token.clone() {
            do_revoke(RevokeRequest {
                server_name: server_name.to_string(),
                endpoint,
                token: access,
                token_type_hint: "access_token".to_string(),
                client_id: entry.client_id.clone(),
                client_secret: entry.client_secret.clone(),
                access_token: entry.access_token.clone(),
            })
            .await?;
        }
    }

    clear_server_tokens_from_local_storage(server_name, server_config, preserve_step_up_state);
    Ok(())
}

/// 撤销请求参数 — `revokeToken` 的输入。
#[derive(Debug, Clone)]
pub struct RevokeRequest {
    pub server_name: String,
    pub endpoint: String,
    pub token: String,
    pub token_type_hint: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub access_token: Option<String>,
}

/// `auth.ts` `clearServerTokensFromLocalStorage`。
///
/// 清除本地存储中的 token 字段；若 `preserve_step_up_state == false`
/// 则删除整个条目。
pub fn clear_server_tokens_from_local_storage(
    server_name: &str,
    server_config: &JsonValue,
    preserve_step_up_state: bool,
) {
    let key = get_server_key(server_name, server_config);
    let mut s = storage().write().unwrap();
    if preserve_step_up_state {
        if let Some(entry) = s.mcp_oauth.get_mut(&key) {
            entry.access_token = None;
            entry.refresh_token = None;
            entry.expires_at = None;
        }
    } else {
        s.mcp_oauth.remove(&key);
    }
}

// ---------------------------------------------------------------------------
// performMCPOAuthFlow — 暂时只提供高层入口签名，真实跳转由 cli 层完成
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PerformMcpOAuthFlowInput {
    pub server_name: String,
    pub server_config: JsonValue,
    /// 用户取消时调用方设置为 true。
    pub user_cancelled: Arc<std::sync::atomic::AtomicBool>,
}

/// `auth.ts` `performMCPOAuthFlow`。
///
/// 直接执行需要交互式浏览器跳转，Rust 端用一个回调来注入。返回获得的
/// `OAuthTokens`。
pub async fn perform_mcp_oauth_flow<F, Fut>(
    input: PerformMcpOAuthFlowInput,
    do_flow: F,
) -> Result<OAuthTokens, String>
where
    F: FnOnce(PerformMcpOAuthFlowInput) -> Fut,
    Fut: std::future::Future<Output = Result<OAuthTokens, String>>,
{
    do_flow(input).await
}

// ---------------------------------------------------------------------------
// wrapFetchWithStepUpDetection — 检测到 step-up 时通知调用方
// ---------------------------------------------------------------------------

/// `auth.ts` `wrapFetchWithStepUpDetection`。
///
/// 接受一个 HTTP 响应的状态码 + `WWW-Authenticate` 头。若该头表明需要
/// step-up（包含 `insufficient_user_authentication`），则触发回调返回 true。
pub fn detect_step_up_response(status: u16, www_authenticate: Option<&str>) -> bool {
    if status != 401 && status != 403 {
        return false;
    }
    let Some(header) = www_authenticate else {
        return false;
    };
    header.contains("insufficient_user_authentication")
}

// ---------------------------------------------------------------------------
// MossenAuthProvider — 协议级实现（不含网络）
// ---------------------------------------------------------------------------

/// `auth.ts` `MossenAuthProvider`。
///
/// 提供 `OAuthClientProvider` 的协议级实现。网络层 / IO 由调用方注入。
/// 真实的 MCP SDK 会要求多个 trait 方法（tokens/clientInformation/...），
/// 这里我们提供同步访问器并暴露存储钩子，便于不同集成场景。
pub struct MossenAuthProvider {
    pub server_name: String,
    pub server_config: JsonValue,
}

impl MossenAuthProvider {
    pub fn new(server_name: impl Into<String>, server_config: JsonValue) -> Self {
        Self {
            server_name: server_name.into(),
            server_config,
        }
    }

    pub fn server_key(&self) -> String {
        get_server_key(&self.server_name, &self.server_config)
    }

    /// 取当前缓存的 OAuth tokens。
    pub fn tokens(&self) -> Option<OAuthTokens> {
        let s = storage().read().unwrap();
        let entry = s.mcp_oauth.get(&self.server_key())?;
        Some(OAuthTokens {
            access_token: entry.access_token.clone().unwrap_or_default(),
            token_type: entry.token_type.clone().unwrap_or_else(|| "Bearer".into()),
            refresh_token: entry.refresh_token.clone(),
            expires_in: None,
            scope: entry.scope.clone(),
        })
    }

    /// 把 token 写入存储。
    pub fn save_tokens(&self, tokens: &OAuthTokens) {
        let mut s = storage().write().unwrap();
        let entry = s.mcp_oauth.entry(self.server_key()).or_default();
        entry.access_token = Some(tokens.access_token.clone());
        entry.token_type = Some(tokens.token_type.clone());
        entry.refresh_token = tokens.refresh_token.clone();
        entry.scope = tokens.scope.clone();
    }

    pub fn invalidate_credentials(&self, scope: InvalidateScope) {
        let mut s = storage().write().unwrap();
        let entry = s.mcp_oauth.entry(self.server_key()).or_default();
        match scope {
            InvalidateScope::Tokens => {
                entry.access_token = None;
                entry.refresh_token = None;
            }
            InvalidateScope::Client => {
                entry.client_id = None;
                entry.client_secret = None;
            }
            InvalidateScope::All => {
                *entry = McpOAuthEntry::default();
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InvalidateScope {
    Tokens,
    Client,
    All,
}

// ---------------------------------------------------------------------------
// readClientSecret / saveMcpClientSecret / clearMcpClientConfig / getMcpClientConfig
// ---------------------------------------------------------------------------

/// `auth.ts` `readClientSecret`。
///
/// 异步读取一个进程级管理的 client secret（与 TS 一致：用进程环境变量
/// 兜底）。Rust 端从 `MCP_CLIENT_SECRET` 读取；若未设置则返回空字符串。
pub async fn read_client_secret() -> String {
    std::env::var("MCP_CLIENT_SECRET").unwrap_or_default()
}

/// `auth.ts` `saveMcpClientSecret`。
pub fn save_mcp_client_secret(server_name: &str, client_id: &str, client_secret: &str) {
    let mut s = storage().write().unwrap();
    let cfg = s
        .mcp_client_config
        .entry(server_name.to_string())
        .or_insert_with(|| json!({}));
    if let Some(obj) = cfg.as_object_mut() {
        obj.insert(
            "client_id".to_string(),
            JsonValue::String(client_id.to_string()),
        );
        obj.insert(
            "client_secret".to_string(),
            JsonValue::String(client_secret.to_string()),
        );
    }
}

/// `auth.ts` `clearMcpClientConfig`。
pub fn clear_mcp_client_config(server_name: &str) {
    let mut s = storage().write().unwrap();
    s.mcp_client_config.remove(server_name);
}

/// `auth.ts` `getMcpClientConfig`。
pub fn get_mcp_client_config(server_name: &str) -> Option<JsonValue> {
    let s = storage().read().unwrap();
    s.mcp_client_config.get(server_name).cloned()
}

// ---------------------------------------------------------------------------
// 测试钩子
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        let mut s = storage().write().unwrap();
        s.mcp_oauth.clear();
        s.mcp_client_config.clear();
    }

    #[test]
    fn server_key_stable() {
        reset();
        let c = json!({"type":"http","url":"https://x","headers":{}});
        let k = get_server_key("svr", &c);
        assert!(k.starts_with("svr|"));
        // Same config -> same hash
        let k2 = get_server_key("svr", &c);
        assert_eq!(k, k2);
    }

    #[test]
    fn discovery_but_no_token_round_trip() {
        reset();
        let c = json!({"type":"http","url":"https://y","headers":{}});
        assert!(!has_mcp_discovery_but_no_token("y", &c, false));
        // Save a stub entry with no tokens
        let key = get_server_key("y", &c);
        storage()
            .write()
            .unwrap()
            .mcp_oauth
            .insert(key, McpOAuthEntry::default());
        assert!(has_mcp_discovery_but_no_token("y", &c, false));
    }

    #[test]
    fn normalize_error_aliases() {
        let body = r#"{"error":"expired_token","error_description":"old"}"#;
        let (s, b) = normalize_oauth_error_body(401, body);
        assert_eq!(s, 400);
        assert!(b.contains("invalid_grant"));
    }
}
