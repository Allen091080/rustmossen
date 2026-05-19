//! 结构化 IO — 对应 TS 的 cli/structuredIO.ts。
//!
//! 提供 SDK 模式下的消息读写能力，支持控制请求/响应协议。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// 最大已解析 tool_use ID 跟踪数。
const MAX_RESOLVED_TOOL_USE_IDS: usize = 1000;

/// 沙箱网络访问的合成工具名。
pub const SANDBOX_NETWORK_ACCESS_TOOL_NAME: &str = "SandboxNetworkAccess";

/// SDK 控制请求子类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum ControlRequestPayload {
    #[serde(rename = "can_use_tool")]
    CanUseTool {
        tool_name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_suggestions: Option<Vec<serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_reason: Option<String>,
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    #[serde(rename = "hook_callback")]
    HookCallback {
        callback_id: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    #[serde(rename = "elicitation")]
    Elicitation {
        mcp_server_name: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        elicitation_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_schema: Option<serde_json::Value>,
    },
    #[serde(rename = "mcp_message")]
    McpMessage {
        server_name: String,
        message: serde_json::Value,
    },
}

/// SDK 控制请求消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlRequest {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub request_id: String,
    pub request: ControlRequestPayload,
}

/// SDK 控制响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SDKControlResponse {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub response: ControlResponsePayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

/// 控制响应载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponsePayload {
    pub request_id: String,
    pub subtype: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Stdin 消息类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StdinMessage {
    #[serde(rename = "user")]
    User {
        session_id: String,
        message: UserMessagePayload,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    #[serde(rename = "control_request")]
    ControlRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        request: Option<serde_json::Value>,
    },
    #[serde(rename = "control_response")]
    ControlResponse(SDKControlResponse),
    #[serde(rename = "assistant")]
    Assistant { content: serde_json::Value },
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "keep_alive")]
    KeepAlive,
    #[serde(rename = "update_environment_variables")]
    UpdateEnvironmentVariables {
        variables: HashMap<String, String>,
    },
    #[serde(rename = "capability_recommendation_response")]
    CapabilityRecommendationResponse {
        recommendation_id: String,
        choice_id: String,
    },
}

/// 用户消息载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessagePayload {
    pub role: String,
    pub content: String,
}

/// Stdout 消息类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StdoutMessage {
    ControlRequest(SDKControlRequest),
    ControlCancelRequest {
        #[serde(rename = "type")]
        msg_type: String,
        request_id: String,
    },
    StreamEvent(serde_json::Value),
}

/// 待处理请求。
struct PendingRequest {
    request: SDKControlRequest,
    response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value>>,
}

/// 权限决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub behavior: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_modified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
}

/// 需要操作的详情 — 对应 TS 的 RequiresActionDetails。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiresActionDetails {
    pub tool_name: String,
    pub action_description: String,
    pub tool_use_id: String,
    pub request_id: String,
    pub input: serde_json::Value,
}

/// StructuredIO — 结构化消息读写器。
///
/// 对应 TS 的 StructuredIO class。
/// 提供 SDK 协议的消息解析、控制请求/响应管理。
pub struct StructuredIO {
    /// 待处理的控制请求映射。
    pending_requests: Arc<Mutex<HashMap<String, PendingRequest>>>,
    /// 已解析的 tool_use ID 集合。
    resolved_tool_use_ids: Arc<Mutex<HashSet<String>>>,
    /// 已解析 ID 顺序（用于 LRU 驱逐）。
    resolved_tool_use_order: Arc<Mutex<VecDeque<String>>>,
    /// 输入关闭标志。
    input_closed: Arc<std::sync::atomic::AtomicBool>,
    /// 前置行（用于注入用户消息）。
    prepended_lines: Arc<Mutex<Vec<String>>>,
    /// 出站消息队列。
    pub outbound: mpsc::Sender<StdoutMessage>,
    outbound_rx: Arc<Mutex<Option<mpsc::Receiver<StdoutMessage>>>>,
    /// 意外响应回调。
    unexpected_response_callback:
        Arc<RwLock<Option<Box<dyn Fn(SDKControlResponse) + Send + Sync>>>>,
    /// 控制请求发送回调。
    on_control_request_sent: Arc<RwLock<Option<Box<dyn Fn(&SDKControlRequest) + Send + Sync>>>>,
    /// 控制请求解析回调。
    on_control_request_resolved: Arc<RwLock<Option<Box<dyn Fn(&str) + Send + Sync>>>>,
    /// 是否重放用户消息。
    replay_user_messages: bool,
}

impl StructuredIO {
    /// 创建新的 StructuredIO 实例。
    pub fn new(replay_user_messages: bool) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            resolved_tool_use_ids: Arc::new(Mutex::new(HashSet::new())),
            resolved_tool_use_order: Arc::new(Mutex::new(VecDeque::new())),
            input_closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            prepended_lines: Arc::new(Mutex::new(Vec::new())),
            outbound: tx,
            outbound_rx: Arc::new(Mutex::new(Some(rx))),
            unexpected_response_callback: Arc::new(RwLock::new(None)),
            on_control_request_sent: Arc::new(RwLock::new(None)),
            on_control_request_resolved: Arc::new(RwLock::new(None)),
            replay_user_messages,
        }
    }

    /// 跟踪已解析的 tool_use ID。
    async fn track_resolved_tool_use_id(&self, request: &SDKControlRequest) {
        if let ControlRequestPayload::CanUseTool { ref tool_use_id, .. } = request.request {
            let mut ids = self.resolved_tool_use_ids.lock().await;
            let mut order = self.resolved_tool_use_order.lock().await;

            ids.insert(tool_use_id.clone());
            order.push_back(tool_use_id.clone());

            // 驱逐最旧的条目
            while ids.len() > MAX_RESOLVED_TOOL_USE_IDS {
                if let Some(oldest) = order.pop_front() {
                    ids.remove(&oldest);
                }
            }
        }
    }

    /// 在输入流前注入用户消息。
    pub async fn prepend_user_message(&self, content: &str) {
        let msg = serde_json::json!({
            "type": "user",
            "session_id": "",
            "message": { "role": "user", "content": content },
            "parent_tool_use_id": null,
        });
        let line = serde_json::to_string(&msg).unwrap_or_default() + "\n";
        let mut prepended = self.prepended_lines.lock().await;
        prepended.push(line);
    }

    /// 获取待处理的权限请求。
    pub async fn get_pending_permission_requests(&self) -> Vec<SDKControlRequest> {
        let pending = self.pending_requests.lock().await;
        pending
            .values()
            .filter(|pr| {
                matches!(
                    pr.request.request,
                    ControlRequestPayload::CanUseTool { .. }
                )
            })
            .map(|pr| pr.request.clone())
            .collect()
    }

    /// 设置意外响应回调。
    pub async fn set_unexpected_response_callback(
        &self,
        callback: Box<dyn Fn(SDKControlResponse) + Send + Sync>,
    ) {
        let mut guard = self.unexpected_response_callback.write().await;
        *guard = Some(callback);
    }

    /// 注入控制响应。
    pub async fn inject_control_response(&self, response: SDKControlResponse) {
        let request_id = &response.response.request_id;
        let mut pending = self.pending_requests.lock().await;
        if let Some(pr) = pending.remove(request_id) {
            self.track_resolved_tool_use_id(&pr.request).await;

            // 发送取消请求到 SDK 消费者
            let cancel_msg = StdoutMessage::ControlCancelRequest {
                msg_type: "control_cancel_request".to_string(),
                request_id: request_id.clone(),
            };
            let _ = self.outbound.send(cancel_msg).await;

            if response.response.subtype == "error" {
                let err_msg = response.response.error.unwrap_or_default();
                let _ = pr.response_tx.send(Err(anyhow::anyhow!("{}", err_msg)));
            } else {
                let result = response.response.response.unwrap_or(serde_json::json!({}));
                let _ = pr.response_tx.send(Ok(result));
            }
        }
    }

    /// 设置控制请求发送回调。
    pub async fn set_on_control_request_sent(
        &self,
        callback: Option<Box<dyn Fn(&SDKControlRequest) + Send + Sync>>,
    ) {
        let mut guard = self.on_control_request_sent.write().await;
        *guard = callback;
    }

    /// 设置控制请求解析回调。
    pub async fn set_on_control_request_resolved(
        &self,
        callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
    ) {
        let mut guard = self.on_control_request_resolved.write().await;
        *guard = callback;
    }

    /// 处理单行输入。
    pub async fn process_line(&self, line: &str) -> Result<Option<StdinMessage>> {
        if line.is_empty() {
            return Ok(None);
        }

        let message: serde_json::Value =
            serde_json::from_str(line).context("failed to parse input line as JSON")?;

        let msg_type = message["type"].as_str().unwrap_or("");

        match msg_type {
            "keep_alive" => Ok(None),
            "update_environment_variables" => {
                if let Some(vars) = message["variables"].as_object() {
                    for (key, value) in vars {
                        if let Some(v) = value.as_str() {
                            std::env::set_var(key, v);
                        }
                    }
                    info!(
                        keys = ?vars.keys().collect::<Vec<_>>(),
                        "applied update_environment_variables"
                    );
                }
                Ok(None)
            }
            "control_response" => {
                let response: SDKControlResponse = serde_json::from_value(message.clone())?;

                // 通知命令生命周期
                if let Some(uuid) = &response.uuid {
                    info!(uuid = %uuid, "control_response: completed");
                }

                let request_id = &response.response.request_id;
                let mut pending = self.pending_requests.lock().await;

                if let Some(pr) = pending.remove(request_id) {
                    self.track_resolved_tool_use_id(&pr.request).await;

                    // 通知桥接
                    if matches!(pr.request.request, ControlRequestPayload::CanUseTool { .. }) {
                        let cb = self.on_control_request_resolved.read().await;
                        if let Some(ref callback) = *cb {
                            callback(request_id);
                        }
                    }

                    if response.response.subtype == "error" {
                        let err_msg = response.response.error.unwrap_or_default();
                        let _ = pr.response_tx.send(Err(anyhow::anyhow!("{}", err_msg)));
                    } else {
                        let result =
                            response.response.response.unwrap_or(serde_json::json!({}));
                        let _ = pr.response_tx.send(Ok(result));
                    }

                    if self.replay_user_messages {
                        let stdin_msg: StdinMessage =
                            serde_json::from_value(message)?;
                        return Ok(Some(stdin_msg));
                    }
                    return Ok(None);
                }

                // 检查是否为已解析的重复响应
                if let Some(tool_use_id) = response
                    .response
                    .response
                    .as_ref()
                    .and_then(|r| r["toolUseID"].as_str())
                {
                    let ids = self.resolved_tool_use_ids.lock().await;
                    if ids.contains(tool_use_id) {
                        info!(
                            tool_use_id = %tool_use_id,
                            "ignoring duplicate control_response for resolved tool"
                        );
                        return Ok(None);
                    }
                }

                // 意外响应回调
                let cb = self.unexpected_response_callback.read().await;
                if let Some(ref callback) = *cb {
                    callback(response);
                }
                Ok(None)
            }
            "user" | "control_request" | "assistant" | "system" => {
                let stdin_msg: StdinMessage = serde_json::from_value(message)?;
                Ok(Some(stdin_msg))
            }
            "capability_recommendation_response" => {
                let stdin_msg: StdinMessage = serde_json::from_value(message)?;
                Ok(Some(stdin_msg))
            }
            other => {
                warn!("ignoring unknown message type: {}", other);
                Ok(None)
            }
        }
    }

    /// 发送消息到 stdout。
    pub async fn write(&self, message: StdoutMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let safe = ndjson_safe_stringify(&json);
        print!("{}\n", safe);
        Ok(())
    }

    /// 发送控制请求并等待响应。
    pub async fn send_request(
        &self,
        request_payload: ControlRequestPayload,
        request_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let request_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        if self
            .input_closed
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            anyhow::bail!("Stream closed");
        }

        let message = SDKControlRequest {
            msg_type: "control_request".to_string(),
            request_id: request_id.clone(),
            request: request_payload,
        };

        // 发送消息
        let _ = self
            .outbound
            .send(StdoutMessage::ControlRequest(message.clone()))
            .await;

        // 通知回调
        if matches!(message.request, ControlRequestPayload::CanUseTool { .. }) {
            let cb = self.on_control_request_sent.read().await;
            if let Some(ref callback) = *cb {
                callback(&message);
            }
        }

        // 创建响应通道
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(
                request_id.clone(),
                PendingRequest {
                    request: message,
                    response_tx: tx,
                },
            );
        }

        // 等待响应
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(anyhow::anyhow!(
                "request channel closed before response"
            )),
        }
    }

    /// 发送 MCP 消息。
    pub async fn send_mcp_message(
        &self,
        server_name: &str,
        message: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let result = self
            .send_request(
                ControlRequestPayload::McpMessage {
                    server_name: server_name.to_string(),
                    message,
                },
                None,
            )
            .await?;
        Ok(result["mcp_response"].clone())
    }

    /// 处理 elicitation 请求。
    pub async fn handle_elicitation(
        &self,
        server_name: &str,
        message: &str,
        requested_schema: Option<serde_json::Value>,
        mode: Option<&str>,
        url: Option<&str>,
        elicitation_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let result = self
            .send_request(
                ControlRequestPayload::Elicitation {
                    mcp_server_name: server_name.to_string(),
                    message: message.to_string(),
                    mode: mode.map(|s| s.to_string()),
                    url: url.map(|s| s.to_string()),
                    elicitation_id: elicitation_id.map(|s| s.to_string()),
                    requested_schema,
                },
                None,
            )
            .await;
        result.or_else(|_| Ok(serde_json::json!({ "action": "cancel" })))
    }

    /// 获取出站接收端（用于 drain loop）。
    pub async fn take_outbound_rx(&self) -> Option<mpsc::Receiver<StdoutMessage>> {
        let mut guard = self.outbound_rx.lock().await;
        guard.take()
    }

    /// 刷新内部事件（非远程 IO 为 no-op）。
    pub async fn flush_internal_events(&self) -> Result<()> {
        Ok(())
    }

    /// 内部事件待发数。
    pub fn internal_events_pending(&self) -> usize {
        0
    }

    /// 标记输入流已关闭。
    pub async fn mark_input_closed(&self) {
        self.input_closed
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // 拒绝所有待处理请求
        let mut pending = self.pending_requests.lock().await;
        for (_, pr) in pending.drain() {
            let _ = pr.response_tx.send(Err(anyhow::anyhow!(
                "Tool permission stream closed before response received"
            )));
        }
    }
}

// ─── NDJSON 安全序列化 ─────────────────────────────────────────────────────

/// NDJSON 安全序列化 — 对应 TS 的 cli/ndjsonSafeStringify.ts。
///
/// 转义 U+2028 和 U+2029，防止在基于行分隔的传输中被错误分割。
pub fn ndjson_safe_stringify(json: &str) -> String {
    json.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 将值序列化为 NDJSON 安全字符串。
pub fn ndjson_safe_serialize(value: &serde_json::Value) -> Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(ndjson_safe_stringify(&json))
}
