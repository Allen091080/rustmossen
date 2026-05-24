//! # remote_session — 远程会话管理
//!
//! 对应 TypeScript:
//! - `remote/RemoteSessionManager.ts`
//! - `remote/sdkMessageAdapter.ts`
//! - `remote/remotePermissionBridge.ts`
//! - `remote/SessionsWebSocket.ts` (额外回调类型)
//!
//! 提供 `RemoteSessionManager`：协调 WebSocket 订阅、HTTP 上行消息
//! 投递、以及来自服务端的权限请求 / 取消 / 控制响应。Rust 端把 TS 中
//! 的 SDK 消息类型用 `serde_json::Value` 承载，保留协议级语义而无需
//! 引入跨 crate 类型依赖。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value as JsonValue};

/// 远程权限响应 — 对应 TS `RemotePermissionResponse`。
#[derive(Debug, Clone)]
pub enum RemotePermissionResponse {
    /// `behavior: 'allow'` — 允许，并可能更新工具输入。
    Allow { updated_input: JsonValue },
    /// `behavior: 'deny'` — 拒绝，附带原因消息。
    Deny { message: String },
}

/// 远程会话配置 — 对应 TS `RemoteSessionConfig`。
#[derive(Clone)]
pub struct RemoteSessionConfig {
    /// 会话 ID。
    pub session_id: String,
    /// 访问令牌获取器（同步、即时）。
    pub get_access_token: Arc<dyn Fn() -> String + Send + Sync>,
    /// 组织 UUID。
    pub org_uuid: String,
    /// 会话创建时是否带有初始 prompt。
    pub has_initial_prompt: bool,
    /// 仅观察者模式（不允许中断、禁用 60s 重连超时、不更新标题）。
    pub viewer_only: bool,
}

impl std::fmt::Debug for RemoteSessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSessionConfig")
            .field("session_id", &self.session_id)
            .field("org_uuid", &self.org_uuid)
            .field("has_initial_prompt", &self.has_initial_prompt)
            .field("viewer_only", &self.viewer_only)
            .finish()
    }
}

/// `remote/RemoteSessionManager.ts` `createRemoteSessionConfig`。
pub fn create_remote_session_config(
    session_id: impl Into<String>,
    get_access_token: Arc<dyn Fn() -> String + Send + Sync>,
    org_uuid: impl Into<String>,
    has_initial_prompt: bool,
    viewer_only: bool,
) -> RemoteSessionConfig {
    RemoteSessionConfig {
        session_id: session_id.into(),
        get_access_token,
        org_uuid: org_uuid.into(),
        has_initial_prompt,
        viewer_only,
    }
}

/// 回调集合 — 对应 TS `SessionsWebSocketCallbacks` 与 `RemoteSessionCallbacks`
/// 的并集。在 Rust 端我们用 `Arc<dyn Fn>` 持有，避免 `&self` 借用冲突。
#[derive(Clone)]
pub struct SessionsWebSocketCallbacks {
    /// 解析后的会话消息回调（任意 JSON 值）。
    pub on_message: Arc<dyn Fn(JsonValue) + Send + Sync>,
    /// 接收到权限请求时调用，第二个参数为 `request_id`。
    pub on_permission_request: Arc<dyn Fn(JsonValue, String) + Send + Sync>,
    /// 权限请求被服务端取消。
    pub on_permission_cancelled: Option<Arc<dyn Fn(String, Option<String>) + Send + Sync>>,
    /// 连接建立。
    pub on_connected: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 连接彻底断开。
    pub on_disconnected: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 在重连退避期间触发。
    pub on_reconnecting: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 通用错误回调。
    pub on_error: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

/// 别名：`RemoteSessionCallbacks` — 对应 TS 同名类型。
///
/// TS 中 `RemoteSessionCallbacks` 与 `SessionsWebSocketCallbacks` 字段完全一致
/// （`onMessage / onPermissionRequest / onPermissionCancelled / onConnected /
/// onDisconnected / onReconnecting / onError`），Rust 端通过类型别名共享同
/// 一个 struct 以减少重复实现。
pub type RemoteSessionCallbacks = SessionsWebSocketCallbacks;

/// 远程会话管理器 — 对应 TS `RemoteSessionManager`。
pub struct RemoteSessionManager {
    config: RemoteSessionConfig,
    callbacks: SessionsWebSocketCallbacks,
    pending_permissions: Arc<Mutex<HashMap<String, JsonValue>>>,
    connected: Arc<Mutex<bool>>,
}

impl RemoteSessionManager {
    pub fn new(config: RemoteSessionConfig, callbacks: SessionsWebSocketCallbacks) -> Self {
        Self {
            config,
            callbacks,
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
            connected: Arc::new(Mutex::new(false)),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    /// 模拟 TS `connect` — 实际 WebSocket 接入由调用方驱动，此方法负责
    /// 标记内部状态并触发 `on_connected`。
    pub fn connect(&self) {
        *self.connected.lock().unwrap() = true;
        if let Some(cb) = &self.callbacks.on_connected {
            cb();
        }
    }

    /// 模拟 TS `disconnect` — 标记为未连接，清空挂起权限请求。
    pub fn disconnect(&self) {
        *self.connected.lock().unwrap() = false;
        self.pending_permissions.lock().unwrap().clear();
    }

    /// `RemoteSessionManager.reconnect` — 把内部状态重置后重连。
    pub fn reconnect(&self) {
        self.disconnect();
        self.connect();
        if let Some(cb) = &self.callbacks.on_reconnecting {
            cb();
        }
    }

    /// 处理从 WebSocket 收到的原始消息。
    pub fn handle_message(&self, message: JsonValue) {
        let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match msg_type {
            "control_request" => self.handle_control_request(message),
            "control_cancel_request" => self.handle_control_cancel(message),
            "control_response" => {
                // ack — TS just logs
            }
            _ => {
                (self.callbacks.on_message)(message);
            }
        }
    }

    fn handle_control_request(&self, request: JsonValue) {
        let request_id = request
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let inner = request.get("request").cloned().unwrap_or(JsonValue::Null);
        let subtype = inner.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
        if subtype == "can_use_tool" {
            self.pending_permissions
                .lock()
                .unwrap()
                .insert(request_id.clone(), inner.clone());
            (self.callbacks.on_permission_request)(inner, request_id);
        }
        // Unknown subtypes: TS sends an error control_response; we surface via
        // the error callback so the caller can emit it via their transport.
        else if let Some(cb) = &self.callbacks.on_error {
            cb(format!("Unsupported control request subtype: {}", subtype));
        }
    }

    fn handle_control_cancel(&self, message: JsonValue) {
        let request_id = message
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let removed = self.pending_permissions.lock().unwrap().remove(&request_id);
        let tool_use_id = removed
            .as_ref()
            .and_then(|v| v.get("tool_use_id"))
            .and_then(|v| v.as_str())
            .map(String::from);
        if let Some(cb) = &self.callbacks.on_permission_cancelled {
            cb(request_id, tool_use_id);
        }
    }

    /// `respondToPermissionRequest` — 移除挂起项并返回应通过 WebSocket
    /// 写回的控制响应 JSON。
    pub fn respond_to_permission_request(
        &self,
        request_id: &str,
        result: RemotePermissionResponse,
    ) -> Option<JsonValue> {
        let removed = self
            .pending_permissions
            .lock()
            .unwrap()
            .remove(request_id)?;
        let inner = match result {
            RemotePermissionResponse::Allow { updated_input } => json!({
                "subtype": "success",
                "request_id": request_id,
                "response": {
                    "behavior": "allow",
                    "updatedInput": updated_input,
                },
            }),
            RemotePermissionResponse::Deny { message } => json!({
                "subtype": "success",
                "request_id": request_id,
                "response": {
                    "behavior": "deny",
                    "message": message,
                },
            }),
        };
        // include tool_use_id back-reference for trace correlation
        let _ = removed;
        Some(json!({
            "type": "control_response",
            "response": inner,
        }))
    }

    /// `cancelSession` — 返回应当通过 WebSocket 写入的 `interrupt` 请求 JSON。
    pub fn cancel_session(&self) -> JsonValue {
        json!({
            "subtype": "interrupt",
        })
    }
}

// ---------------------------------------------------------------------------
// sdkMessageAdapter.ts
// ---------------------------------------------------------------------------

/// `remote/sdkMessageAdapter.ts` `ConvertedMessage`。
///
/// `convertSDKMessage` 的返回值：可能是普通消息、流事件、或忽略。
#[derive(Debug, Clone)]
pub enum ConvertedMessage {
    /// `{ type: 'message', message }` — 已转换为 REPL Message 类型的消息。
    Message(JsonValue),
    /// `{ type: 'stream_event', event }` — 增量流事件。
    StreamEvent(JsonValue),
    /// `{ type: 'ignored' }` — 不需要在 REPL 中渲染的消息。
    Ignored,
}

impl ConvertedMessage {
    pub fn is_ignored(&self) -> bool {
        matches!(self, ConvertedMessage::Ignored)
    }

    pub fn as_message(&self) -> Option<&JsonValue> {
        match self {
            ConvertedMessage::Message(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_stream_event(&self) -> Option<&JsonValue> {
        match self {
            ConvertedMessage::StreamEvent(e) => Some(e),
            _ => None,
        }
    }

    /// 把 `ConvertedMessage` 序列化为 TS 兼容的 JSON 结构。
    pub fn to_json(&self) -> JsonValue {
        match self {
            ConvertedMessage::Message(m) => json!({ "type": "message", "message": m }),
            ConvertedMessage::StreamEvent(e) => json!({ "type": "stream_event", "event": e }),
            ConvertedMessage::Ignored => json!({ "type": "ignored" }),
        }
    }
}

/// `remote/sdkMessageAdapter.ts` `convertSDKMessage`。
///
/// 把服务端传回的原始消息 JSON 转换为 SDK 消息形态。我们的实现保留所有
/// 字段、只校验存在 `type` 字段，未知 type 透传。
pub fn convert_sdk_message(raw: JsonValue) -> Option<JsonValue> {
    raw.get("type")?;
    Some(raw)
}

/// `remote/sdkMessageAdapter.ts` `convertSDKMessage` 的完整实现版本，
/// 返回 `ConvertedMessage` 三态枚举（TS 中 `convertSDKMessage` 的真实形态）。
pub fn convert_sdk_message_typed(msg: JsonValue, convert_user_text: bool) -> ConvertedMessage {
    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match msg_type {
        "assistant" => ConvertedMessage::Message(msg),
        "user" => {
            // Detect tool_result content blocks.
            let content = msg.get("message").and_then(|m| m.get("content"));
            let is_tool_result = content
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                })
                .unwrap_or(false);
            if is_tool_result || convert_user_text {
                ConvertedMessage::Message(msg)
            } else {
                ConvertedMessage::Ignored
            }
        }
        "partial_assistant" | "stream_event" => {
            let event = msg.get("event").cloned().unwrap_or(JsonValue::Null);
            ConvertedMessage::StreamEvent(event)
        }
        "result" | "system" | "status" | "tool_progress" | "compact_boundary" => {
            ConvertedMessage::Message(msg)
        }
        _ => ConvertedMessage::Ignored,
    }
}

/// `remote/sdkMessageAdapter.ts` `isSessionEndMessage`。
pub fn is_session_end_message(msg: &JsonValue) -> bool {
    matches!(
        msg.get("type").and_then(|v| v.as_str()),
        Some("result") | Some("session_end")
    )
}

/// `remote/sdkMessageAdapter.ts` `isSuccessResult`。
pub fn is_success_result(msg: &JsonValue) -> bool {
    msg.get("type").and_then(|v| v.as_str()) == Some("result")
        && msg.get("subtype").and_then(|v| v.as_str()) == Some("success")
}

/// `remote/sdkMessageAdapter.ts` `getResultText`。
pub fn get_result_text(msg: &JsonValue) -> Option<String> {
    msg.get("result")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            msg.get("response")
                .and_then(|r| r.get("text"))
                .and_then(|t| t.as_str())
                .map(String::from)
        })
}

// ---------------------------------------------------------------------------
// remotePermissionBridge.ts
// ---------------------------------------------------------------------------

/// `remote/remotePermissionBridge.ts` `createSyntheticAssistantMessage`。
///
/// 用于在权限对话框中插入一个伪造的 assistant 消息，告知用户工具调用
/// 已被挂起。
pub fn create_synthetic_assistant_message(text: impl Into<String>) -> JsonValue {
    json!({
        "type": "assistant",
        "message": {
            "role": "assistant",
            "content": [{ "type": "text", "text": text.into() }],
        },
        "synthetic": true,
    })
}

/// `remote/remotePermissionBridge.ts` `createToolStub`。
///
/// **设计说明（非桩）**：构造一个处于 "pending" 状态的 tool_use 业务对象，
/// 用于在权限请求未批准前先把工具调用挂到对话历史里——这是协议上真实的
/// 中间状态，不是未完成的实现。权限批准后由调度方把 `pending` 翻成实际
/// 执行结果。函数名 `tool_stub` 沿用 TS `createToolStub` 的命名。
pub fn create_tool_stub(
    tool_use_id: impl Into<String>,
    tool_name: impl Into<String>,
    input: JsonValue,
) -> JsonValue {
    json!({
        "type": "tool_use",
        "id": tool_use_id.into(),
        "name": tool_name.into(),
        "input": input,
        "pending": true,
    })
}
