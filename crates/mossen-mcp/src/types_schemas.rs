//! # types_schemas — types.ts 中的 Zod schema 等价
//!
//! 对应 TypeScript `services/mcp/types.ts`。TS 用 zod 给每种 MCP transport
//! 配置定义了一个 schema；Rust 端我们没有 zod，但保留同样的命名方便上层
//! 引用 — 这些 "schema" 是返回 `bool` 的校验函数，输入是任意 JSON。
//!
//! 函数命名遵循 TS：`mcp_stdio_server_config_schema` 等于
//! `McpStdioServerConfigSchema.safeParse(...).success`。
//! 复杂的字段约束（args 列表、headers map）也按 TS 行为还原。

use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// Transport schema
// ---------------------------------------------------------------------------

/// `types.ts` `TransportSchema`：`'stdio' | 'sse' | 'http' | 'ws'`。
pub fn transport_schema(v: &JsonValue) -> bool {
    matches!(
        v.as_str(),
        Some("stdio") | Some("sse") | Some("http") | Some("ws")
    )
}

// `types.ts` `Transport` 类型 — 别名。
pub type Transport = String;

// ---------------------------------------------------------------------------
// ConfigScopeSchema (重复 config.rs 中的 ConfigScope，保留 schema 入口)
// ---------------------------------------------------------------------------

pub fn config_scope_schema(v: &JsonValue) -> bool {
    matches!(
        v.as_str(),
        Some("user")
            | Some("project")
            | Some("local")
            | Some("dynamic")
            | Some("enterprise")
            | Some("hosted")
    )
}

// ---------------------------------------------------------------------------
// Per-transport server config schemas
// ---------------------------------------------------------------------------

fn ensure_string(v: &JsonValue) -> bool {
    v.is_string()
}

fn ensure_string_array(v: &JsonValue) -> bool {
    v.as_array()
        .map(|arr| arr.iter().all(|x| x.is_string()))
        .unwrap_or(false)
}

fn ensure_string_map(v: &JsonValue) -> bool {
    v.as_object()
        .map(|m| m.values().all(|v| v.is_string()))
        .unwrap_or(false)
}

/// `types.ts` `McpStdioServerConfigSchema`。
pub fn mcp_stdio_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    // type optional — default 'stdio'
    let ty_ok = match obj.get("type") {
        None => true,
        Some(t) => t.as_str() == Some("stdio"),
    };
    let cmd_ok = obj.get("command").map(ensure_string).unwrap_or(false);
    let args_ok = obj.get("args").map(ensure_string_array).unwrap_or(true);
    let env_ok = obj.get("env").map(ensure_string_map).unwrap_or(true);
    ty_ok && cmd_ok && args_ok && env_ok
}

/// `types.ts` `McpSSEServerConfigSchema`。
pub fn mcp_sse_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("sse")
        && obj.get("url").map(ensure_string).unwrap_or(false)
        && obj.get("headers").map(ensure_string_map).unwrap_or(true)
}

/// `types.ts` `McpSSEIDEServerConfigSchema`。
pub fn mcp_sse_ide_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("sse-ide")
        && obj.get("url").map(ensure_string).unwrap_or(false)
}

/// `types.ts` `McpWebSocketIDEServerConfigSchema`。
pub fn mcp_websocket_ide_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("ws-ide")
        && obj.get("url").map(ensure_string).unwrap_or(false)
}

/// `types.ts` `McpHTTPServerConfigSchema`。
pub fn mcp_http_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("http")
        && obj.get("url").map(ensure_string).unwrap_or(false)
        && obj.get("headers").map(ensure_string_map).unwrap_or(true)
}

/// `types.ts` `McpWebSocketServerConfigSchema`。
pub fn mcp_websocket_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("ws")
        && obj.get("url").map(ensure_string).unwrap_or(false)
}

/// `types.ts` `McpSdkServerConfigSchema`。
pub fn mcp_sdk_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("sdk")
        && obj.get("name").map(ensure_string).unwrap_or(false)
}

/// `types.ts` `McpHostedProxyServerConfigSchema`。
pub fn mcp_hosted_proxy_server_config_schema(v: &JsonValue) -> bool {
    let Some(obj) = v.as_object() else { return false };
    obj.get("type").and_then(|t| t.as_str()) == Some("hosted-proxy")
        && obj.get("url").map(ensure_string).unwrap_or(false)
        && obj.get("id").map(ensure_string).unwrap_or(false)
}

/// `types.ts` `McpServerConfigSchema` — 联合体。
pub fn mcp_server_config_schema(v: &JsonValue) -> bool {
    mcp_stdio_server_config_schema(v)
        || mcp_sse_server_config_schema(v)
        || mcp_sse_ide_server_config_schema(v)
        || mcp_websocket_ide_server_config_schema(v)
        || mcp_http_server_config_schema(v)
        || mcp_websocket_server_config_schema(v)
        || mcp_sdk_server_config_schema(v)
        || mcp_hosted_proxy_server_config_schema(v)
}

/// `types.ts` `McpJsonConfigSchema`：`{ mcpServers: { [name]: McpServerConfig } }`。
pub fn mcp_json_config_schema(v: &JsonValue) -> bool {
    v.get("mcpServers")
        .and_then(|m| m.as_object())
        .map(|obj| obj.values().all(mcp_server_config_schema))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// ConnectedMCPServer / FailedMCPServer / NeedsAuthMCPServer / PendingMCPServer
// / DisabledMCPServer / MCPServerConnection — 这些是数据类型，不是 schema。
// 我们用 JSON enum 表示状态。
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerConnection {
    Connected {
        name: String,
        config: JsonValue,
        capabilities: Option<JsonValue>,
    },
    Failed {
        name: String,
        config: JsonValue,
        error: String,
    },
    NeedsAuth {
        name: String,
        config: JsonValue,
    },
    Pending {
        name: String,
        config: JsonValue,
    },
    Disabled {
        name: String,
        config: JsonValue,
        reason: Option<String>,
    },
}

/// `types.ts` `SerializedTool`。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SerializedTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: JsonValue,
}

/// `types.ts` `SerializedClient`。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SerializedClient {
    pub name: String,
    pub config: JsonValue,
    pub tools: Vec<SerializedTool>,
}

/// `types.ts` `MCPCliState`。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct McpCliState {
    pub clients: Vec<McpServerConnection>,
    pub tools: Vec<SerializedTool>,
    pub commands: Vec<JsonValue>,
    pub resources: std::collections::HashMap<String, Vec<JsonValue>>,
}

// 与 TS 一一对应的别名 — TS 中是 union 的 4 个 variant，Rust 端各自暴露
// 一个 type 名（指向同一个 enum）。
pub type ConnectedMCPServer = McpServerConnection;
pub type FailedMCPServer = McpServerConnection;
pub type NeedsAuthMCPServer = McpServerConnection;
pub type PendingMCPServer = McpServerConnection;
pub type DisabledMCPServer = McpServerConnection;
pub type MCPServerConnection = McpServerConnection;

/// `types.ts` `ServerResource` — `Resource & { server: string }`。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerResource {
    pub server: String,
    pub uri: String,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_stdio_config() {
        assert!(mcp_stdio_server_config_schema(&json!({
            "type": "stdio",
            "command": "foo",
            "args": ["a", "b"],
        })));
    }

    #[test]
    fn rejects_unknown_transport() {
        assert!(!mcp_server_config_schema(&json!({
            "type": "carrier-pigeon",
            "command": "foo",
        })));
    }
}
