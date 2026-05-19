//! # client_ext — client.ts 中尚未翻译的客户端工具
//!
//! 对应 TypeScript `services/mcp/client.ts`。本模块只翻译那些与 transport
//! 无关、调用方可独立测试的工具：
//! - 错误类型 `McpAuthError`、`McpToolCallError`、`McpSessionExpiredError`；
//! - `is_mcp_session_expired_error` — 404 + JSON-RPC -32001 的辨识；
//! - `clear_mcp_auth_cache` — 清空全局 auth 缓存（用 set）；
//! - `create_hosted_proxy_fetch` — 把 hosted proxy 头注入到 fetch；
//! - `wrap_fetch_with_timeout` — 给 fetch 加超时（注入式）；
//! - `get_mcp_server_connection_batch_size` — 并发批大小；
//! - `get_server_cache_key`、`are_mcp_configs_equal` — 缓存键 / 比较；
//! - `infer_compact_schema`、`transform_mcp_result`、`process_mcp_result` 等
//!   纯函数变换。
//!
//! 仍依赖外部 SDK 的功能（`connect_to_server`、`call_mcp_tool_*` 等）由
//! `client.rs` 主模块持有 — 这里只补单元化的纯函数与简单的状态。

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value as JsonValue};

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

/// `client.ts` `McpAuthError`。
#[derive(Debug, Clone, thiserror::Error)]
#[error("McpAuthError({server_name}): {message}")]
pub struct McpAuthError {
    pub server_name: String,
    pub message: String,
}

impl McpAuthError {
    pub fn new(server_name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            message: message.into(),
        }
    }
}

/// `client.ts` `McpSessionExpiredError`。
#[derive(Debug, Clone, thiserror::Error)]
#[error("MCP server \"{server_name}\" session expired")]
pub struct McpSessionExpiredError {
    pub server_name: String,
}

impl McpSessionExpiredError {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
        }
    }
}

/// `client.ts` `McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS` 的精确别名。
pub type McpToolCallErrorIVerifiedThisIsNotCodeOrFilepaths = McpToolCallError;

/// `client.ts` `McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS`。
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct McpToolCallError {
    pub message: String,
    pub telemetry_message: String,
    pub mcp_meta: Option<JsonValue>,
}

impl McpToolCallError {
    pub fn new(
        message: impl Into<String>,
        telemetry_message: impl Into<String>,
        mcp_meta: Option<JsonValue>,
    ) -> Self {
        Self {
            message: message.into(),
            telemetry_message: telemetry_message.into(),
            mcp_meta,
        }
    }
}

/// `client.ts` `isMcpSessionExpiredError`。
///
/// `(http_status, message)` —— `http_status == 404` 且消息中包含 JSON-RPC code
/// `-32001` 视为会话过期。Rust 端用 `String` 承载错误信息。
pub fn is_mcp_session_expired_error(http_status: Option<u16>, message: &str) -> bool {
    if http_status != Some(404) {
        return false;
    }
    message.contains("\"code\":-32001") || message.contains("\"code\": -32001")
}

// ---------------------------------------------------------------------------
// auth cache
// ---------------------------------------------------------------------------

fn auth_cache() -> &'static Mutex<HashSet<String>> {
    static C: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashSet::new()))
}

/// `client.ts` `clearMcpAuthCache`。
pub fn clear_mcp_auth_cache() {
    auth_cache().lock().unwrap().clear();
}

/// 给上层提供的入口：标记某个 server 已建立 auth。
pub fn mark_mcp_auth_cached(server_key: &str) {
    auth_cache()
        .lock()
        .unwrap()
        .insert(server_key.to_string());
}

/// 给上层提供的入口：查询某个 server 是否在 auth 缓存中。
pub fn has_mcp_auth_cached(server_key: &str) -> bool {
    auth_cache().lock().unwrap().contains(server_key)
}

// ---------------------------------------------------------------------------
// fetch 包装器（注入式）
// ---------------------------------------------------------------------------

/// 简化的 fetch 函数签名：`(url, headers, body) -> (status, body)`。
pub type FetchLike = std::sync::Arc<
    dyn for<'a> Fn(
            &'a str,
            &'a std::collections::HashMap<String, String>,
            Option<&'a [u8]>,
        ) -> futures::future::BoxFuture<'a, Result<(u16, String), String>>
        + Send
        + Sync,
>;

/// 描述 hosted-proxy 请求中需要附加的头与元数据。
#[derive(Debug, Clone, Default)]
pub struct HostedProxyHeaders {
    pub authorization: Option<String>,
    pub mossen_beta: Option<String>,
    pub mossen_version: Option<String>,
    pub extra: std::collections::HashMap<String, String>,
}

/// `client.ts` `createHostedProxyFetch`。
///
/// 把 hosted-proxy 头注入到调用方传入的 inner fetch。返回值类型是 `FetchLike`，
/// 调用方对每次请求都会自动得到这些头。
pub fn create_hosted_proxy_fetch(
    inner: FetchLike,
    proxy_headers: HostedProxyHeaders,
) -> FetchLike {
    std::sync::Arc::new(move |url, headers, body| {
        let mut merged = headers.clone();
        if let Some(v) = &proxy_headers.authorization {
            merged.insert("Authorization".into(), v.clone());
        }
        if let Some(v) = &proxy_headers.mossen_beta {
            merged.insert("mossen-beta".into(), v.clone());
        }
        if let Some(v) = &proxy_headers.mossen_version {
            merged.insert("mossen-version".into(), v.clone());
        }
        for (k, v) in &proxy_headers.extra {
            merged.insert(k.clone(), v.clone());
        }
        let inner = inner.clone();
        let body_owned = body.map(|b| b.to_vec());
        let url_owned = url.to_string();
        Box::pin(async move {
            inner(&url_owned, &merged, body_owned.as_deref()).await
        })
    })
}

/// `client.ts` `wrapFetchWithTimeout`。
pub fn wrap_fetch_with_timeout(inner: FetchLike, timeout: Duration) -> FetchLike {
    std::sync::Arc::new(move |url, headers, body| {
        let inner = inner.clone();
        let url_owned = url.to_string();
        let headers_owned = headers.clone();
        let body_owned = body.map(|b| b.to_vec());
        let t = timeout;
        Box::pin(async move {
            let fut = inner(&url_owned, &headers_owned, body_owned.as_deref());
            match tokio::time::timeout(t, fut).await {
                Ok(r) => r,
                Err(_) => Err(format!("fetch timed out after {:?}", t)),
            }
        })
    })
}

// ---------------------------------------------------------------------------
// 配置 / 缓存键
// ---------------------------------------------------------------------------

/// `client.ts` `getMcpServerConnectionBatchSize`。
///
/// 从 env `MCP_CONNECT_BATCH_SIZE` 读取并 clamp 到 [1, 50]，默认 10。
pub fn get_mcp_server_connection_batch_size() -> usize {
    std::env::var("MCP_CONNECT_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.clamp(1, 50))
        .unwrap_or(10)
}

/// `client.ts` `getServerCacheKey`。
///
/// `<name>|<sha256(config)[:16]>` —— 与 `auth.get_server_key` 相同的算法，
/// 但 server_key 是按 (name, type, url, headers) 哈希；这里多哈希了 `args`,
/// `env`, `command` 等字段以匹配 TS 的语义。
pub fn get_server_cache_key(name: &str, config: &JsonValue) -> String {
    let canonical = json!({
        "type": config.get("type").cloned().unwrap_or(JsonValue::Null),
        "command": config.get("command").cloned().unwrap_or(JsonValue::Null),
        "args": config.get("args").cloned().unwrap_or(JsonValue::Null),
        "env": config.get("env").cloned().unwrap_or(JsonValue::Null),
        "url": config.get("url").cloned().unwrap_or(JsonValue::Null),
        "headers": config.get("headers").cloned().unwrap_or(JsonValue::Null),
    });
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(serde_json::to_string(&canonical).unwrap_or_default().as_bytes());
    let hex = format!("{:x}", h.finalize());
    let short: String = hex.chars().take(16).collect();
    format!("{}|{}", name, short)
}

/// `client.ts` `areMcpConfigsEqual`。
pub fn are_mcp_configs_equal(a: &JsonValue, b: &JsonValue) -> bool {
    get_server_cache_key("", a) == get_server_cache_key("", b)
}

/// `client.ts` `mcpToolInputToAutoClassifierInput`。
///
/// 仅保留指定字段或者 fallback 为整个对象，用于自动分类器输入。
pub fn mcp_tool_input_to_auto_classifier_input(input: &JsonValue) -> JsonValue {
    match input {
        JsonValue::Object(obj) => {
            // Pick first ~8 fields to keep payload small.
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys.into_iter().take(8) {
                out.insert(k.clone(), obj[k].clone());
            }
            JsonValue::Object(out)
        }
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// 结果变换
// ---------------------------------------------------------------------------

/// `client.ts` `inferCompactSchema`。
///
/// 给一个值推断一个紧凑的 schema 字符串：`object{k1, k2}`, `array<T>`, 等。
pub fn infer_compact_schema(value: &JsonValue, depth: i32) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(_) => "boolean".to_string(),
        JsonValue::Number(_) => "number".to_string(),
        JsonValue::String(_) => "string".to_string(),
        JsonValue::Array(arr) => {
            if arr.is_empty() {
                "array<unknown>".to_string()
            } else if depth <= 0 {
                "array<?>".to_string()
            } else {
                format!("array<{}>", infer_compact_schema(&arr[0], depth - 1))
            }
        }
        JsonValue::Object(obj) => {
            if obj.is_empty() {
                "object{}".to_string()
            } else if depth <= 0 {
                "object{?}".to_string()
            } else {
                let mut keys: Vec<&String> = obj.keys().collect();
                keys.sort();
                let parts: Vec<String> = keys
                    .iter()
                    .map(|k| format!("{}: {}", k, infer_compact_schema(&obj[*k], depth - 1)))
                    .collect();
                format!("object{{{}}}", parts.join(", "))
            }
        }
    }
}

/// `client.ts` `MCPResultType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpResultType {
    ToolResult,
    StructuredContent,
    ContentArray,
}

/// `client.ts` `TransformedMCPResult`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransformedMcpResult {
    pub kind: String,
    pub content: JsonValue,
    pub is_error: bool,
}

/// `client.ts` `transformMCPResult` 的同步骨架。
///
/// 把原始 MCP 工具结果 JSON 转换为统一 `(kind, content, is_error)`。
pub fn transform_mcp_result(raw: &JsonValue) -> TransformedMcpResult {
    let is_error = raw
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if let Some(struct_content) = raw.get("structuredContent") {
        return TransformedMcpResult {
            kind: "structuredContent".into(),
            content: struct_content.clone(),
            is_error,
        };
    }
    if let Some(content) = raw.get("content") {
        return TransformedMcpResult {
            kind: "contentArray".into(),
            content: content.clone(),
            is_error,
        };
    }
    TransformedMcpResult {
        kind: "toolResult".into(),
        content: raw.clone(),
        is_error,
    }
}

/// `client.ts` `processMCPResult`。
///
/// 给 transform 出来的结果做最后一步处理：text 拼接成单字符串，object 直接
/// 序列化。返回 (display_text, is_error)。
pub fn process_mcp_result(t: &TransformedMcpResult) -> (String, bool) {
    let text = match &t.content {
        JsonValue::String(s) => s.clone(),
        JsonValue::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                if let Some(s) = item.get("text").and_then(|v| v.as_str()) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(s);
                }
            }
            if out.is_empty() {
                serde_json::to_string(&t.content).unwrap_or_default()
            } else {
                out
            }
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    (text, t.is_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_session_expired() {
        assert!(is_mcp_session_expired_error(
            Some(404),
            "{\"error\":{\"code\":-32001,\"message\":\"x\"}}"
        ));
        assert!(!is_mcp_session_expired_error(Some(500), "x"));
    }

    #[test]
    fn schema_inference_array() {
        let s = infer_compact_schema(&json!([1, 2, 3]), 2);
        assert_eq!(s, "array<number>");
    }

    #[test]
    fn transform_recognizes_structured() {
        let r = transform_mcp_result(&json!({"structuredContent": {"foo": 1}}));
        assert_eq!(r.kind, "structuredContent");
    }

    #[test]
    fn process_text_join() {
        let t = TransformedMcpResult {
            kind: "contentArray".into(),
            content: json!([{"type":"text","text":"hi"}, {"type":"text","text":"there"}]),
            is_error: false,
        };
        let (s, _) = process_mcp_result(&t);
        assert_eq!(s, "hi\nthere");
    }
}
