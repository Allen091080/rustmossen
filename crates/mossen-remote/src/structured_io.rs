//! # structured_io — 结构化 IO 协议
//!
//! 提供 SDK 消息的读写协议，处理控制请求/响应流、权限流程等。
//! 对应 TS `cli/structuredIO.ts` 中的 `StructuredIO` 类。

use crate::connection::ResolvedIdTracker;
use crate::transport::StdoutMessage;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing;

/// 沙箱网络访问的合成工具名称。
///
/// 通过 can_use_tool 控制请求协议转发沙箱网络权限请求时使用。
pub const SANDBOX_NETWORK_ACCESS_TOOL_NAME: &str = "SandboxNetworkAccess";

/// 最大已解析 tool_use ID 追踪数量。
const MAX_RESOLVED_TOOL_USE_IDS: usize = 1000;

/// 控制请求发送回调。
pub type OnControlRequestSentCallback = Box<dyn Fn(&ControlRequest) + Send + Sync>;

/// 控制请求解析回调。
pub type OnControlRequestResolvedCallback = Box<dyn Fn(&str) + Send + Sync>;

/// 未预期响应回调。
pub type UnexpectedResponseCallback =
    Arc<dyn Fn(Value) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

/// 控制请求（对应 TS `SDKControlRequest`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ControlRequest {
    /// 消息类型（始终为 `control_request`）。
    #[serde(rename = "type")]
    pub msg_type: String,
    /// 请求 ID。
    pub request_id: String,
    /// 请求体。
    pub request: Value,
}

/// 控制响应（对应 TS `SDKControlResponse`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ControlResponse {
    /// 消息类型（始终为 `control_response`）。
    #[serde(rename = "type")]
    pub msg_type: String,
    /// 响应体。
    pub response: ControlResponseBody,
}

/// 控制响应体。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ControlResponseBody {
    /// 响应子类型。
    pub subtype: String,
    /// 请求 ID。
    pub request_id: String,
    /// 成功时的响应载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    /// 错误时的错误信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 待处理请求。
struct PendingRequest {
    /// 原始请求。
    request: ControlRequest,
    /// 完成通知（成功值或错误）。
    tx: oneshot::Sender<Result<Value, String>>,
}

/// 结构化 IO 协议处理器。
///
/// 提供 SDK 消息的读写通道，处理控制请求/响应匹配、
/// 权限请求流程和去重逻辑。
pub struct StructuredIo {
    /// 待处理请求映射（request_id → PendingRequest）。
    pending_requests: Arc<Mutex<HashMap<String, PendingRequest>>>,
    /// 已解析的 tool_use ID 追踪器。
    resolved_tool_use_ids: Arc<Mutex<ResolvedIdTracker>>,
    /// 输入流是否已关闭。
    input_closed: Arc<RwLock<bool>>,
    /// 出站消息队列发送端。
    outbound_tx: mpsc::UnboundedSender<StdoutMessage>,
    /// 出站消息队列接收端。
    outbound_rx: Arc<Mutex<mpsc::UnboundedReceiver<StdoutMessage>>>,
    /// 未预期响应回调。
    unexpected_response_callback: Arc<RwLock<Option<UnexpectedResponseCallback>>>,
    /// 控制请求发送回调。
    on_control_request_sent: Arc<RwLock<Option<OnControlRequestSentCallback>>>,
    /// 控制请求解析回调。
    on_control_request_resolved: Arc<RwLock<Option<OnControlRequestResolvedCallback>>>,
    /// 是否重播用户消息。
    replay_user_messages: bool,
    /// 预置行缓冲。
    prepended_lines: Arc<Mutex<Vec<String>>>,
}

impl StructuredIo {
    /// 创建新的结构化 IO 处理器。
    pub fn new(replay_user_messages: bool) -> Self {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        Self {
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            resolved_tool_use_ids: Arc::new(Mutex::new(ResolvedIdTracker::new(
                MAX_RESOLVED_TOOL_USE_IDS,
            ))),
            input_closed: Arc::new(RwLock::new(false)),
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(outbound_rx)),
            unexpected_response_callback: Arc::new(RwLock::new(None)),
            on_control_request_sent: Arc::new(RwLock::new(None)),
            on_control_request_resolved: Arc::new(RwLock::new(None)),
            replay_user_messages,
            prepended_lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 获取出站消息接收端（用于 drain 循环）。
    pub fn take_outbound_rx(&self) -> Arc<Mutex<mpsc::UnboundedReceiver<StdoutMessage>>> {
        self.outbound_rx.clone()
    }

    /// 向出站队列写入消息。
    pub fn write(&self, message: StdoutMessage) -> anyhow::Result<()> {
        self.outbound_tx
            .send(message)
            .map_err(|_| anyhow::anyhow!("outbound channel closed"))
    }

    /// 向出站队列写入 NDJSON 消息（序列化后发送）。
    pub fn write_stdout(&self, message: &StdoutMessage) -> anyhow::Result<()> {
        self.outbound_tx
            .send(message.clone())
            .map_err(|_| anyhow::anyhow!("outbound channel closed"))
    }

    /// 预置一条用户消息到输入流前端。
    pub async fn prepend_user_message(&self, content: &str) {
        let msg = serde_json::json!({
            "type": "user",
            "session_id": "",
            "message": { "role": "user", "content": content },
            "parent_tool_use_id": null,
        });
        let line = serde_json::to_string(&msg).unwrap_or_default() + "\n";
        self.prepended_lines.lock().await.push(line);
    }

    /// 获取待处理的权限请求列表。
    pub async fn get_pending_permission_requests(&self) -> Vec<ControlRequest> {
        let pending = self.pending_requests.lock().await;
        pending
            .values()
            .filter(|pr| {
                pr.request.request.get("subtype").and_then(|v| v.as_str()) == Some("can_use_tool")
            })
            .map(|pr| pr.request.clone())
            .collect()
    }

    /// 设置未预期响应回调。
    pub async fn set_unexpected_response_callback(&self, callback: UnexpectedResponseCallback) {
        *self.unexpected_response_callback.write().await = Some(callback);
    }

    /// 设置控制请求发送回调。
    pub async fn set_on_control_request_sent(&self, callback: OnControlRequestSentCallback) {
        *self.on_control_request_sent.write().await = Some(callback);
    }

    /// 设置控制请求解析回调。
    pub async fn set_on_control_request_resolved(
        &self,
        callback: OnControlRequestResolvedCallback,
    ) {
        *self.on_control_request_resolved.write().await = Some(callback);
    }

    /// 记录 tool_use ID 为已解析（防止重复处理）。
    async fn track_resolved_tool_use_id(&self, request: &ControlRequest) {
        if request.request.get("subtype").and_then(|v| v.as_str()) == Some("can_use_tool") {
            if let Some(tool_use_id) = request.request.get("tool_use_id").and_then(|v| v.as_str()) {
                self.resolved_tool_use_ids
                    .lock()
                    .await
                    .insert(tool_use_id.to_string());
            }
        }
    }

    /// 注入控制响应（由桥接层调用）。
    ///
    /// 解析待处理的权限请求，同时发送 cancel 给 SDK 消费端。
    pub async fn inject_control_response(&self, response: &ControlResponse) {
        let request_id = &response.response.request_id;
        let mut pending = self.pending_requests.lock().await;
        if let Some(pr) = pending.remove(request_id) {
            self.track_resolved_tool_use_id(&pr.request).await;
            // 发送 cancel 给 SDK 消费端
            let _ = self.write(serde_json::json!({
                "type": "control_cancel_request",
                "request_id": request_id,
            }));
            if response.response.subtype == "error" {
                let err_msg = response
                    .response
                    .error
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string());
                let _ = pr.tx.send(Err(err_msg));
            } else {
                let result = response
                    .response
                    .response
                    .clone()
                    .unwrap_or(Value::Object(serde_json::Map::new()));
                let _ = pr.tx.send(Ok(result));
            }
        }
    }

    /// 处理输入行。
    ///
    /// 解析 NDJSON 行，匹配控制响应到待处理请求，返回需要向上层传递的消息。
    pub async fn process_line(&self, line: &str) -> Option<Value> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        let message: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Error parsing streaming input line: {}", e);
                return None;
            }
        };

        let msg_type = message.get("type")?.as_str()?;

        match msg_type {
            "keep_alive" => None,
            "update_environment_variables" => {
                // 环境变量更新（在 Rust 中通过 std::env::set_var 实现）
                if let Some(vars) = message.get("variables").and_then(|v| v.as_object()) {
                    for (key, value) in vars {
                        if let Some(val_str) = value.as_str() {
                            // SAFETY: 在单线程初始化阶段设置环境变量
                            unsafe {
                                std::env::set_var(key, val_str);
                            }
                        }
                    }
                    tracing::debug!(
                        "applied update_environment_variables: {:?}",
                        vars.keys().collect::<Vec<_>>()
                    );
                }
                None
            }
            "control_response" => self.handle_control_response(&message).await,
            "user" | "control_request" | "assistant" | "system" => Some(message),
            "capability_recommendation_response" => {
                // 能力推荐响应：在翻译层简化为日志记录
                tracing::debug!("Received capability_recommendation_response");
                None
            }
            _ => {
                tracing::warn!("Ignoring unknown message type: {}", msg_type);
                None
            }
        }
    }

    /// 处理控制响应消息。
    async fn handle_control_response(&self, message: &Value) -> Option<Value> {
        let response_body = message.get("response")?;
        let request_id = response_body.get("request_id")?.as_str()?;

        let mut pending = self.pending_requests.lock().await;
        if let Some(pr) = pending.remove(request_id) {
            self.track_resolved_tool_use_id(&pr.request).await;

            // 通知桥接层
            if pr.request.request.get("subtype").and_then(|v| v.as_str()) == Some("can_use_tool") {
                if let Some(ref cb) = *self.on_control_request_resolved.read().await {
                    cb(request_id);
                }
            }

            let subtype = response_body
                .get("subtype")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if subtype == "error" {
                let err = response_body
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                let _ = pr.tx.send(Err(err));
            } else {
                let result = response_body
                    .get("response")
                    .cloned()
                    .unwrap_or(Value::Object(serde_json::Map::new()));
                let _ = pr.tx.send(Ok(result));
            }

            if self.replay_user_messages {
                return Some(message.clone());
            }
            return None;
        }

        // 检查是否为已解析的重复响应
        let tool_use_id = response_body
            .get("response")
            .and_then(|r| r.get("toolUseID"))
            .and_then(|v| v.as_str());
        if let Some(id) = tool_use_id {
            if self.resolved_tool_use_ids.lock().await.contains(id) {
                tracing::debug!(
                    "Ignoring duplicate control_response for already-resolved toolUseID={}",
                    id
                );
                return None;
            }
        }

        // 未匹配的响应：调用未预期响应回调
        drop(pending);
        if let Some(ref cb) = *self.unexpected_response_callback.read().await {
            cb(message.clone()).await;
        }
        None
    }

    /// 发送控制请求并等待响应。
    pub async fn send_request(&self, request_body: Value) -> anyhow::Result<Value> {
        self.send_request_with_id(request_body, uuid::Uuid::new_v4().to_string())
            .await
    }

    /// 发送控制请求（指定请求 ID）并等待响应。
    pub async fn send_request_with_id(
        &self,
        request_body: Value,
        request_id: String,
    ) -> anyhow::Result<Value> {
        if *self.input_closed.read().await {
            anyhow::bail!("Stream closed");
        }

        let message = ControlRequest {
            msg_type: "control_request".to_string(),
            request_id: request_id.clone(),
            request: request_body.clone(),
        };

        let message_value = serde_json::to_value(&message)?;
        self.outbound_tx
            .send(message_value)
            .map_err(|_| anyhow::anyhow!("outbound channel closed"))?;

        // 通知控制请求发送回调
        if request_body.get("subtype").and_then(|v| v.as_str()) == Some("can_use_tool") {
            if let Some(ref cb) = *self.on_control_request_sent.read().await {
                cb(&message);
            }
        }

        let (tx, rx) = oneshot::channel();
        self.pending_requests.lock().await.insert(
            request_id.clone(),
            PendingRequest {
                request: message,
                tx,
            },
        );

        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => anyhow::bail!("{}", err),
            Err(_) => anyhow::bail!("Request channel closed"),
        }
    }

    /// 标记输入流已关闭，拒绝所有待处理请求。
    pub async fn close_input(&self) {
        *self.input_closed.write().await = true;
        let mut pending = self.pending_requests.lock().await;
        for (_, pr) in pending.drain() {
            let _ = pr.tx.send(Err(
                "Tool permission stream closed before response received".to_string(),
            ));
        }
    }

    /// 刷新内部事件。非远程 IO 为空操作。
    pub async fn flush_internal_events(&self) {
        // 默认空操作，由 RemoteIo 覆盖
    }

    /// 内部事件队列深度。非远程 IO 为 0。
    pub fn internal_events_pending(&self) -> usize {
        0
    }

    /// 消耗预置行缓冲并返回内容。
    pub async fn take_prepended_lines(&self) -> Vec<String> {
        let mut lines = self.prepended_lines.lock().await;
        std::mem::take(&mut *lines)
    }
}
