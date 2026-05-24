//! # extras — 剩余 MCP 模块的薄翻译
//!
//! 把剩下零散的 TS export 补齐到 Rust 等价物。每个子模块对应一个 TS 文件，
//! 只翻译能在 Rust 端独立测试的部分；与 React/SDK 强耦合的部分（连接管理、
//! VSCode bridge 等）保留为接口签名 + 注入式钩子。

use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value as JsonValue;

// ===========================================================================
// channelAllowlist.ts
// ===========================================================================

/// `channelAllowlist.ts` `isChannelsEnabled`。
pub fn is_channels_enabled() -> bool {
    crate::channels::is_channels_enabled_default()
}

/// `channelAllowlist.ts` `isChannelAllowlisted`。
pub fn is_channel_allowlisted(plugin: &str, marketplace: &str) -> bool {
    crate::channels::get_channel_allowlist()
        .iter()
        .any(|e| e.plugin == plugin && e.marketplace == marketplace)
}

// ===========================================================================
// channelNotification.ts —— schema 校验
// ===========================================================================

/// `channelNotification.ts` `ChannelMessageNotificationSchema` 的 Rust 等价：
/// 一个判断 JSON 是否符合 schema 的函数。
pub fn channel_message_notification_schema(v: &JsonValue) -> bool {
    if v.get("method").and_then(|m| m.as_str()) != Some(crate::channels::CHANNEL_MESSAGE_METHOD) {
        return false;
    }
    let Some(params) = v.get("params") else {
        return false;
    };
    params.get("content").and_then(|c| c.as_str()).is_some()
}

/// `channelNotification.ts` `ChannelPermissionNotificationSchema`。
pub fn channel_permission_notification_schema(v: &JsonValue) -> bool {
    if v.get("method").and_then(|m| m.as_str()) != Some(crate::channels::CHANNEL_PERMISSION_METHOD)
    {
        return false;
    }
    let Some(params) = v.get("params") else {
        return false;
    };
    params.get("request_id").and_then(|c| c.as_str()).is_some()
        && matches!(
            params.get("behavior").and_then(|c| c.as_str()),
            Some("allow") | Some("deny")
        )
}

// ===========================================================================
// channelPermissions.ts —— 剩余项
// ===========================================================================

/// `channelPermissions.ts` `PERMISSION_REPLY_RE`。
pub static PERMISSION_REPLY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?P<verdict>y|yes|n|no)\s+(?P<token>[a-km-z]{5})\s*$").unwrap());

/// `channelPermissions.ts` `ChannelPermissionResponse`。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChannelPermissionResponse {
    pub request_id: String,
    pub behavior: String,
}

/// `channelPermissions.ts` `ChannelPermissionCallbacks`。
pub struct ChannelPermissionCallbacks {
    pub send_request:
        std::sync::Arc<dyn Fn(crate::channels::ChannelPermissionRequestParams) + Send + Sync>,
    pub cancel_request: std::sync::Arc<dyn Fn(String) + Send + Sync>,
}

/// `channelPermissions.ts` `createChannelPermissionCallbacks`。
pub fn create_channel_permission_callbacks(
    send_request: std::sync::Arc<
        dyn Fn(crate::channels::ChannelPermissionRequestParams) + Send + Sync,
    >,
    cancel_request: std::sync::Arc<dyn Fn(String) + Send + Sync>,
) -> ChannelPermissionCallbacks {
    ChannelPermissionCallbacks {
        send_request,
        cancel_request,
    }
}

/// 解析人类回复 `"yes tbxkq"` / `"no abckl"`。
pub fn parse_permission_reply(text: &str) -> Option<(bool, String)> {
    let cap = PERMISSION_REPLY_RE.captures(text)?;
    let verdict = cap.name("verdict")?.as_str();
    let token = cap.name("token")?.as_str().to_string();
    let allow = verdict.starts_with('y');
    Some((allow, token))
}

// ===========================================================================
// SdkControlTransport.ts
// ===========================================================================

/// 把单条 MCP 消息发到 SDK 控制通道的回调签名。
pub type SendMcpMessageCallback = std::sync::Arc<dyn Fn(JsonValue) + Send + Sync>;

/// `SdkControlTransport.ts` `SdkControlClientTransport`。
///
/// 由调用方注入 `send` 回调；transport 本身只负责存储和暴露 close 状态。
pub struct SdkControlClientTransport {
    pub send: SendMcpMessageCallback,
    pub closed: std::sync::atomic::AtomicBool,
}

impl SdkControlClientTransport {
    pub fn new(send: SendMcpMessageCallback) -> Self {
        Self {
            send,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn send_message(&self, msg: JsonValue) {
        if !self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            (self.send)(msg);
        }
    }
    pub fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// `SdkControlTransport.ts` `SdkControlServerTransport`。
pub struct SdkControlServerTransport {
    pub send: SendMcpMessageCallback,
    pub closed: std::sync::atomic::AtomicBool,
}

impl SdkControlServerTransport {
    pub fn new(send: SendMcpMessageCallback) -> Self {
        Self {
            send,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }
    pub fn send_message(&self, msg: JsonValue) {
        if !self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            (self.send)(msg);
        }
    }
    pub fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// ===========================================================================
// vscodeSdkMcp.ts
// ===========================================================================

/// `vscodeSdkMcp.ts` `LogEventNotificationSchema`。
pub fn log_event_notification_schema(v: &JsonValue) -> bool {
    v.get("method").and_then(|m| m.as_str()) == Some("notifications/vscode/log")
        && v.get("params").is_some()
}

/// `vscodeSdkMcp.ts` `notifyVscodeFileUpdated`。
///
/// 调用方注入 `do_send`，本函数构造通知 JSON。
pub fn notify_vscode_file_updated(file_path: &str) -> JsonValue {
    serde_json::json!({
        "method": "notifications/vscode/fileUpdated",
        "params": { "filePath": file_path },
    })
}

/// `vscodeSdkMcp.ts` `setupVscodeSdkMcp`。
///
/// 返回一个 server name 给 client 注册。Rust 端不直接持有 vscode bridge —
/// 由 cli 层完成实际接线。
pub fn setup_vscode_sdk_mcp() -> &'static str {
    "vscode"
}

// ===========================================================================
// officialRegistry.ts
// ===========================================================================

static OFFICIAL_MCP_URLS: Lazy<std::sync::RwLock<HashSet<String>>> =
    Lazy::new(|| std::sync::RwLock::new(HashSet::new()));

/// `officialRegistry.ts` `prefetchOfficialMcpUrls`。
pub async fn prefetch_official_mcp_urls<F, Fut>(fetch_impl: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<String>, String>>,
{
    if let Ok(urls) = fetch_impl().await {
        let mut set = OFFICIAL_MCP_URLS.write().unwrap();
        for u in urls {
            set.insert(u);
        }
    }
}

/// `officialRegistry.ts` `isOfficialMcpUrl`。
pub fn is_official_mcp_url(url: &str) -> bool {
    OFFICIAL_MCP_URLS.read().unwrap().contains(url)
}

/// `officialRegistry.ts` `resetOfficialMcpUrlsForTesting`。
pub fn reset_official_mcp_urls_for_testing() {
    OFFICIAL_MCP_URLS.write().unwrap().clear()
}

// ===========================================================================
// MCPConnectionManager.tsx —— 仅业务逻辑入口
// ===========================================================================

/// `MCPConnectionManager.tsx` `useMcpReconnect`。
///
/// Rust 端不暴露 React hook — 我们返回一个函数指针：调用即触发 reconnect。
pub fn use_mcp_reconnect<F: Fn() + Send + Sync + 'static>(
    reconnect_impl: F,
) -> impl Fn() + Send + Sync + 'static {
    reconnect_impl
}

/// `MCPConnectionManager.tsx` `useMcpToggleEnabled`。
pub fn use_mcp_toggle_enabled<F: Fn(&str, bool) + Send + Sync + 'static>(
    toggle_impl: F,
) -> impl Fn(&str, bool) + Send + Sync + 'static {
    toggle_impl
}

/// `MCPConnectionManager.tsx` `MCPConnectionManager` —— 业务逻辑只返回需要
/// 渲染的状态视图（具体 UI 由 mossen-tui）。
pub fn mcp_connection_manager_state(clients: &[JsonValue]) -> Vec<(String, String)> {
    clients
        .iter()
        .map(|c| {
            (
                c.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string(),
                c.get("type")
                    .and_then(|n| n.as_str())
                    .unwrap_or("connected")
                    .to_string(),
            )
        })
        .collect()
}

// ===========================================================================
// headersHelper.ts
// ===========================================================================

/// `headersHelper.ts` `getMcpHeadersFromHelper`。
///
/// 调用方提供 helper 命令的 stdout 文本（headers 命令的 JSON 输出）。
pub fn get_mcp_headers_from_helper(stdout: &str) -> HashMap<String, String> {
    serde_json::from_str::<HashMap<String, String>>(stdout).unwrap_or_default()
}

/// `headersHelper.ts` `getMcpServerHeaders`。
pub fn get_mcp_server_headers(config: &JsonValue) -> HashMap<String, String> {
    config
        .get("headers")
        .and_then(|h| h.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

// ===========================================================================
// mcpStringUtils.ts
// ===========================================================================

/// `mcpStringUtils.ts` `getToolNameForPermissionCheck`。
///
/// 把 `mcp__server__tool` 收缩为 `mcp__server`，让 permission 规则可以批量
/// 授权一个 server 的所有工具。
pub fn get_tool_name_for_permission_check(tool_name: &str) -> String {
    let parts: Vec<&str> = tool_name.splitn(3, "__").collect();
    if parts.len() >= 3 && parts[0] == "mcp" {
        format!("mcp__{}", parts[1])
    } else {
        tool_name.to_string()
    }
}

/// `mcpStringUtils.ts` `extractMcpToolDisplayName`。
pub fn extract_mcp_tool_display_name(tool_name: &str) -> String {
    let parts: Vec<&str> = tool_name.splitn(3, "__").collect();
    if parts.len() == 3 && parts[0] == "mcp" {
        parts[2].to_string()
    } else {
        tool_name.to_string()
    }
}

// ===========================================================================
// useManageMCPConnections.ts
// ===========================================================================

/// `useManageMCPConnections.ts` `useManageMCPConnections`。
///
/// 业务逻辑：给一组 (name, status) 返回排序后的列表（needs-auth 在前，
/// disabled 在后）。UI 渲染由 mossen-tui。
pub fn use_manage_mcp_connections(rows: Vec<(String, String)>) -> Vec<(String, String)> {
    fn priority(s: &str) -> u8 {
        match s {
            "needs-auth" => 0,
            "failed" => 1,
            "pending" => 2,
            "connected" => 3,
            "disabled" => 4,
            _ => 5,
        }
    }
    let mut r = rows;
    r.sort_by(|a, b| {
        priority(&a.1)
            .cmp(&priority(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    r
}

// ===========================================================================
// auth.ts `wrapFetchWithStepUpDetection`
// ===========================================================================

/// `auth.ts` `wrapFetchWithStepUpDetection`。
///
/// 与 [`crate::auth_ext::detect_step_up_response`] 等价 — 这是真正的入口名。
pub fn wrap_fetch_with_step_up_detection(status: u16, www_authenticate: Option<&str>) -> bool {
    crate::auth_ext::detect_step_up_response(status, www_authenticate)
}

// ===========================================================================
// utils.ts `getProjectMcpServerStatus`
// ===========================================================================

/// `utils.ts` `getProjectMcpServerStatus`。
///
/// 调用方提供：disabled 列表、enabled 列表、`enable_all_project_mcp_servers`
/// 总开关、`skip_dangerous` flag、`project_settings_enabled` flag、`non_interactive` flag。
pub fn get_project_mcp_server_status(
    server_name: &str,
    disabled: &[String],
    enabled: &[String],
    enable_all_project_mcp_servers: bool,
    has_skip_dangerous: bool,
    project_settings_enabled: bool,
    non_interactive: bool,
) -> &'static str {
    let normalized = crate::normalization::normalize_name_for_mcp(server_name);
    if disabled
        .iter()
        .any(|n| crate::normalization::normalize_name_for_mcp(n) == normalized)
    {
        return "rejected";
    }
    if enable_all_project_mcp_servers
        || enabled
            .iter()
            .any(|n| crate::normalization::normalize_name_for_mcp(n) == normalized)
    {
        return "approved";
    }
    if has_skip_dangerous && project_settings_enabled {
        return "approved";
    }
    if non_interactive && project_settings_enabled {
        return "approved";
    }
    "pending"
}

// ===========================================================================
// xaaIdpLogin.ts `IdpLoginOptions`
// ===========================================================================

/// `xaaIdpLogin.ts` `IdpLoginOptions`。
#[derive(Debug, Clone)]
pub struct IdpLoginOptions {
    pub idp_issuer: String,
    pub idp_client_id: String,
    pub idp_client_secret: Option<String>,
    pub callback_port: Option<u16>,
    pub skip_browser_open: bool,
}

// ===========================================================================
// client.ts 剩余 4 个 — 接口签名（实际网络 IO 由 client.rs/transport.rs 处理）
// ===========================================================================

/// `client.ts` `clearServerCache`。
pub async fn clear_server_cache(name: &str) {
    crate::client_ext::clear_mcp_auth_cache();
    let _ = name;
}

/// `client.ts` `ensureConnectedClient` 的协议级签名。
///
/// 调用方负责真实的重连；本函数提供 marker，让 client.rs 调度入口统一。
pub async fn ensure_connected_client<F, Fut>(name: &str, connect: F) -> Result<(), String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    connect(name.to_string()).await
}

/// `client.ts` `callIdeRpc`。
pub async fn call_ide_rpc<F, Fut>(
    method: &str,
    params: JsonValue,
    do_rpc: F,
) -> Result<JsonValue, String>
where
    F: FnOnce(String, JsonValue) -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    do_rpc(method.to_string(), params).await
}

/// `client.ts` `reconnectMcpServerImpl`。
pub async fn reconnect_mcp_server_impl<F, Fut>(
    name: &str,
    config: JsonValue,
    do_reconnect: F,
) -> Result<(), String>
where
    F: FnOnce(String, JsonValue) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    do_reconnect(name.to_string(), config).await
}

/// `client.ts` `getMcpToolsCommandsAndResources`。
///
/// 汇总当前所有 MCP 客户端的 tools / commands / resources。`fetch_for_client`
/// 接受 client name 并返回三元组。
pub async fn get_mcp_tools_commands_and_resources<F, Fut>(
    clients: &[String],
    fetch_for_client: F,
) -> (
    Vec<JsonValue>,
    Vec<JsonValue>,
    HashMap<String, Vec<JsonValue>>,
)
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = (Vec<JsonValue>, Vec<JsonValue>, Vec<JsonValue>)>,
{
    let mut tools = Vec::new();
    let mut commands = Vec::new();
    let mut resources: HashMap<String, Vec<JsonValue>> = HashMap::new();
    for client in clients {
        let (t, c, r) = fetch_for_client(client.clone()).await;
        tools.extend(t);
        commands.extend(c);
        if !r.is_empty() {
            resources.insert(client.clone(), r);
        }
    }
    (tools, commands, resources)
}

/// `client.ts` `prefetchAllMcpResources`。
pub async fn prefetch_all_mcp_resources<F, Fut>(
    clients: &[String],
    fetch_resources: F,
) -> HashMap<String, Vec<JsonValue>>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Vec<JsonValue>>,
{
    let mut out = HashMap::new();
    for c in clients {
        out.insert(c.clone(), fetch_resources(c.clone()).await);
    }
    out
}

/// `client.ts` `transformResultContent`。
///
/// 把 raw MCP content array → 纯文本表示（用于显示）。
pub fn transform_result_content(content: &JsonValue) -> String {
    match content {
        JsonValue::String(s) => s.clone(),
        JsonValue::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|b| {
                    let ty = b.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match ty {
                        "text" => b.get("text").and_then(|v| v.as_str()).map(String::from),
                        "image" => Some("[image]".to_string()),
                        "resource" => b
                            .get("resource")
                            .and_then(|r| r.get("uri"))
                            .and_then(|u| u.as_str())
                            .map(|u| format!("[resource: {}]", u)),
                        _ => Some(format!("[{}]", ty)),
                    }
                })
                .collect();
            parts.join("\n")
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// `client.ts` `callMCPToolWithUrlElicitationRetry`。
///
/// 当工具返回 `requires_elicitation`/`error_code = -32042` 时，让调用方
/// 触发 elicitation 流程后重试一次。Rust 端用注入 closure 表达这一控制流。
pub async fn call_mcp_tool_with_url_elicitation_retry<C, CF, E, EF>(
    call_tool: C,
    elicit: E,
) -> Result<JsonValue, String>
where
    C: Fn() -> CF,
    CF: std::future::Future<Output = Result<JsonValue, JsonValue>>,
    E: FnOnce(JsonValue) -> EF,
    EF: std::future::Future<Output = Result<(), String>>,
{
    let first = call_tool().await;
    let err = match first {
        Ok(ok) => return Ok(ok),
        Err(e) => e,
    };
    let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != -32042 {
        return Err(err.to_string());
    }
    let params = err.get("data").cloned().unwrap_or(JsonValue::Null);
    elicit(params).await.map_err(|m| m)?;
    call_tool()
        .await
        .map_err(|e| serde_json::to_string(&e).unwrap_or_default())
}

/// `MCPConnectionManager.tsx` `MCPConnectionManager` 业务逻辑别名。
pub fn mcp_connection_manager(clients: &[JsonValue]) -> Vec<(String, String)> {
    mcp_connection_manager_state(clients)
}

/// `InProcessTransport.ts` `createLinkedTransportPair`。
///
/// 创建一对内联 transport：发给 a 的消息出现在 b 的 inbox，反之亦然。
pub fn create_linked_transport_pair() -> (InProcessTransport, InProcessTransport) {
    let (a_to_b_tx, a_to_b_rx) = tokio::sync::mpsc::unbounded_channel::<JsonValue>();
    let (b_to_a_tx, b_to_a_rx) = tokio::sync::mpsc::unbounded_channel::<JsonValue>();
    (
        InProcessTransport {
            outbound: a_to_b_tx,
            inbound: std::sync::Arc::new(tokio::sync::Mutex::new(b_to_a_rx)),
        },
        InProcessTransport {
            outbound: b_to_a_tx,
            inbound: std::sync::Arc::new(tokio::sync::Mutex::new(a_to_b_rx)),
        },
    )
}

/// `InProcessTransport.ts` `InProcessTransport`。
pub struct InProcessTransport {
    pub outbound: tokio::sync::mpsc::UnboundedSender<JsonValue>,
    pub inbound:
        std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<JsonValue>>>,
}

impl InProcessTransport {
    pub fn send(&self, msg: JsonValue) -> Result<(), String> {
        self.outbound.send(msg).map_err(|e| e.to_string())
    }
    pub async fn recv(&self) -> Option<JsonValue> {
        let mut rx = self.inbound.lock().await;
        rx.recv().await
    }
}

/// `envExpansion.ts` `expandEnvVarsInString`。
///
/// 展开字符串中形如 `${VAR}` 或 `$VAR` 的变量引用。提供 `lookup` 闭包查询变量值；
/// 找不到时保留原文。
pub fn expand_env_vars_in_string<F>(input: &str, lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('{') => {
                chars.next();
                let mut name = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc == '}' {
                        chars.next();
                        break;
                    }
                    name.push(pc);
                    chars.next();
                }
                match lookup(&name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push_str("${");
                        out.push_str(&name);
                        out.push('}');
                    }
                }
            }
            Some(c2) if c2.is_ascii_alphabetic() || *c2 == '_' => {
                let mut name = String::new();
                while let Some(&pc) = chars.peek() {
                    if pc.is_ascii_alphanumeric() || pc == '_' {
                        name.push(pc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match lookup(&name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('$');
                        out.push_str(&name);
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

// ===========================================================================
// client.ts —— 剩余的高层连接入口（注入式骨架）
// ===========================================================================

/// `client.ts` `setupSdkMcpClients`。
///
/// 对每个 SDK MCP server 配置启动连接；调用方注入 `do_setup`。
pub async fn setup_sdk_mcp_clients<F, Fut>(
    configs: &[JsonValue],
    do_setup: F,
) -> Vec<Result<JsonValue, String>>
where
    F: Fn(JsonValue) -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    let mut out = Vec::new();
    for cfg in configs {
        out.push(do_setup(cfg.clone()).await);
    }
    out
}

/// `client.ts` `connectToServer`（memoized 在 TS 中；Rust 端直接代理）。
pub async fn connect_to_server<F, Fut>(
    name: &str,
    config: JsonValue,
    do_connect: F,
) -> Result<JsonValue, String>
where
    F: FnOnce(String, JsonValue) -> Fut,
    Fut: std::future::Future<Output = Result<JsonValue, String>>,
{
    do_connect(name.to_string(), config).await
}

/// `client.ts` `fetchToolsForClient`（memoized LRU 在 TS 中；Rust 端代理）。
pub async fn fetch_tools_for_client<F, Fut>(name: &str, do_fetch: F) -> Vec<JsonValue>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Vec<JsonValue>>,
{
    do_fetch(name.to_string()).await
}

/// `client.ts` `fetchResourcesForClient`。
pub async fn fetch_resources_for_client<F, Fut>(name: &str, do_fetch: F) -> Vec<JsonValue>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Vec<JsonValue>>,
{
    do_fetch(name.to_string()).await
}

/// `client.ts` `fetchCommandsForClient`。
pub async fn fetch_commands_for_client<F, Fut>(name: &str, do_fetch: F) -> Vec<JsonValue>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Vec<JsonValue>>,
{
    do_fetch(name.to_string()).await
}

// ===========================================================================
// types.ts —— McpStdioServerConfig 等类型别名
// ===========================================================================

pub type McpHTTPServerConfig = crate::config::HttpServerConfig;
pub type McpHostedProxyServerConfig = crate::config::HostedProxyServerConfig;
pub type McpSdkServerConfig = crate::config::SdkServerConfig;
pub type McpSSEIDEServerConfig = crate::config::SseIdeServerConfig;
pub type McpSSEServerConfig = crate::config::SseServerConfig;
pub type McpStdioServerConfig = crate::config::StdioServerConfig;
pub type McpWebSocketIDEServerConfig = crate::config::WsIdeServerConfig;
pub type McpWebSocketServerConfig = crate::config::WsServerConfig;
pub use crate::config::McpJsonConfig;
pub use crate::config::McpServerConfig;
pub use crate::config::ScopedMcpServerConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_reply_parses() {
        let (allow, token) = parse_permission_reply(" yes tbxkq ").unwrap();
        assert!(allow);
        assert_eq!(token, "tbxkq");
        let (allow2, token2) = parse_permission_reply("no abcde").unwrap();
        assert!(!allow2);
        assert_eq!(token2, "abcde");
        assert!(parse_permission_reply("yes !!!!").is_none());
    }

    #[test]
    fn tool_name_for_permission() {
        assert_eq!(
            get_tool_name_for_permission_check("mcp__myserver__doThing"),
            "mcp__myserver"
        );
        assert_eq!(get_tool_name_for_permission_check("Bash"), "Bash");
    }

    #[test]
    fn display_name_extraction() {
        assert_eq!(extract_mcp_tool_display_name("mcp__svr__tool"), "tool");
    }
}
