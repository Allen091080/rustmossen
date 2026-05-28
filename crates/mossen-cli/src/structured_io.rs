//! 结构化 IO — 对应 TS 的 cli/structuredIO.ts。
//!
//! 提供 SDK 模式下的消息读写能力，支持控制请求/响应协议。

use crate::bootstrap::{
    get_allowed_setting_sources, get_chrome_flag_override, get_client_type,
    get_flag_settings_inline, get_flag_settings_path, get_initial_main_loop_model,
    get_inline_plugins, get_invoked_skills, get_is_remote_mode, get_main_loop_model_override,
    get_model_strings, get_model_usage, get_plan_slug_cache, get_registered_hooks,
    get_session_bypass_permissions_mode, get_session_source, get_total_api_duration,
    get_total_cost_usd, get_total_tool_duration, get_turn_hook_count, get_turn_hook_duration_ms,
    get_use_cowork_plugins, has_unknown_model_cost, set_main_loop_model_override,
};
use crate::stream_json_render_events::{
    StreamJsonRenderEventEmitter, STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION,
    STREAM_JSON_RENDER_EVENT_THROTTLE_MS, STREAM_JSON_RENDER_EVENT_TYPE,
    STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION, STREAM_JSON_RENDER_FRAME_TYPE,
    STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION, STREAM_JSON_RENDER_SNAPSHOT_TYPE,
};
use crate::stream_json_terminal_renderer::{
    STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION, STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
    STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS, STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION,
    STREAM_JSON_RENDER_PATCH_TYPE,
};
use anyhow::{Context, Result};
use mossen_agent::context::{
    auto_compact_threshold, effective_context_window, error_threshold, warning_threshold,
};
use mossen_agent::mcp::runtime_status as mcp_runtime_status;
use mossen_agent::services::compact::pending_compact_request::{
    clear_pending_compact_request, enqueue_pending_compact_request, get_pending_compact_request,
    CompactMode, COMPACT_REQUEST_TIMEOUT,
};
use mossen_agent::services::config::{doctor as config_doctor, profiles as config_profiles};
use mossen_agent::services::lsp::diagnostic_registry::get_pending_lsp_diagnostic_count;
use mossen_agent::services::root::pending_clear_request::{
    enqueue_pending_clear_request, get_pending_clear_request, CLEAR_REQUEST_TIMEOUT,
};
use mossen_agent::services::root::runtime_status::snapshot_agent_runtime_status;
use mossen_agent::services::root::slash_command_capabilities::{
    format_available_stream_json_slash_commands, get_stream_json_slash_command_capabilities,
    get_stream_json_slash_command_capability, get_stream_json_slash_command_capability_manifest,
    normalize_stream_json_slash_command, serialize_stream_json_slash_command_capability,
    CommandStatus, SlashCommandCapability, STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION,
};
use mossen_agent::types::PermissionMode;
use mossen_commands::access::{PERMISSION_ALLOW_RULES_ENV, PERMISSION_DENY_RULES_ENV};
use mossen_commands::context::{
    CommandContext, CommandCostModelUsage, CommandCostSnapshot, CommandResult, Directive,
};
use mossen_utils::git_diff::fetch_git_diff;
use mossen_utils::model_utils::{
    get_context_window_for_model as model_context_window_for_model, is_model_alias, MODEL_ALIASES,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// 最大已解析 tool_use ID 跟踪数。
const MAX_RESOLVED_TOOL_USE_IDS: usize = 1000;
const PERMISSION_MODE_ENV: &str = "MOSSEN_PERMISSION_MODE";
const TERMINAL_APPROVAL_ACTION_APPROVE_ONCE: &str = "approve_once";
const TERMINAL_APPROVAL_ACTION_REJECT: &str = "reject";
const TERMINAL_APPROVAL_ACTION_EDIT_COMMAND: &str = "edit_command";
const TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION: &str = "approve_for_session";
const TERMINAL_APPROVAL_INPUT_PREVIEW_MAX_CHARS: usize = 160;

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
    UpdateEnvironmentVariables { variables: HashMap<String, String> },
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
    ControlResponse(SDKControlResponse),
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
#[serde(rename_all = "camelCase")]
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
    /// Shared render-event reducer for stream-json control responses.
    render_event_emitter: Option<Arc<Mutex<StreamJsonRenderEventEmitter>>>,
}

impl Clone for StructuredIO {
    fn clone(&self) -> Self {
        Self {
            pending_requests: self.pending_requests.clone(),
            resolved_tool_use_ids: self.resolved_tool_use_ids.clone(),
            resolved_tool_use_order: self.resolved_tool_use_order.clone(),
            input_closed: self.input_closed.clone(),
            prepended_lines: self.prepended_lines.clone(),
            outbound: self.outbound.clone(),
            outbound_rx: self.outbound_rx.clone(),
            unexpected_response_callback: self.unexpected_response_callback.clone(),
            on_control_request_sent: self.on_control_request_sent.clone(),
            on_control_request_resolved: self.on_control_request_resolved.clone(),
            replay_user_messages: self.replay_user_messages,
            render_event_emitter: self.render_event_emitter.clone(),
        }
    }
}

impl StructuredIO {
    /// 创建新的 StructuredIO 实例。
    pub fn new(replay_user_messages: bool) -> Self {
        Self::with_render_event_emitter(replay_user_messages, None)
    }

    pub fn new_with_render_event_emitter(
        replay_user_messages: bool,
        render_event_emitter: Arc<Mutex<StreamJsonRenderEventEmitter>>,
    ) -> Self {
        Self::with_render_event_emitter(replay_user_messages, Some(render_event_emitter))
    }

    fn with_render_event_emitter(
        replay_user_messages: bool,
        render_event_emitter: Option<Arc<Mutex<StreamJsonRenderEventEmitter>>>,
    ) -> Self {
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
            render_event_emitter,
        }
    }

    /// 跟踪已解析的 tool_use ID。
    async fn track_resolved_tool_use_id(&self, request: &SDKControlRequest) {
        if let ControlRequestPayload::CanUseTool {
            ref tool_use_id, ..
        } = request.request
        {
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

    async fn notify_control_request_resolved(&self, request: &SDKControlRequest, request_id: &str) {
        if matches!(request.request, ControlRequestPayload::CanUseTool { .. }) {
            let cb = self.on_control_request_resolved.read().await;
            if let Some(ref callback) = *cb {
                callback(request_id);
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
            .filter(|pr| matches!(pr.request.request, ControlRequestPayload::CanUseTool { .. }))
            .map(|pr| pr.request.clone())
            .collect()
    }

    /// Resolve the single pending terminal approval request with a supported action.
    ///
    /// Session approval returns an SDK-compatible session-scoped permission update.
    pub async fn resolve_pending_permission_with_approval_action(
        &self,
        action_id: &str,
    ) -> Result<Option<SDKControlResponse>> {
        self.resolve_pending_permission_with_approval_action_input(action_id, None)
            .await
    }

    /// Resolve the single pending terminal approval request with an optional
    /// edited input payload. `edit_command` only succeeds when this payload is
    /// supplied; otherwise it stays fail-closed.
    pub async fn resolve_pending_permission_with_approval_action_input(
        &self,
        action_id: &str,
        edited_input: Option<serde_json::Value>,
    ) -> Result<Option<SDKControlResponse>> {
        let pending_permissions = self.get_pending_permission_requests().await;
        let request = match pending_permissions.len() {
            0 => return Ok(None),
            1 => pending_permissions
                .into_iter()
                .next()
                .expect("single request"),
            count => {
                anyhow::bail!(
                    "ambiguous terminal approval action: {count} pending permission requests"
                )
            }
        };

        let decision =
            permission_decision_for_approval_action(action_id, Some(&request), edited_input)?;
        let response = permission_decision_control_response(request.request_id.clone(), decision)?;
        self.inject_control_response(response.clone()).await;
        Ok(Some(response))
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
        let request_id = response.response.request_id.clone();
        let pending_request = {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&request_id)
        };
        if let Some(pr) = pending_request {
            self.track_resolved_tool_use_id(&pr.request).await;
            self.notify_control_request_resolved(&pr.request, &request_id)
                .await;

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

                let request_id = response.response.request_id.clone();
                let pending_request = {
                    let mut pending = self.pending_requests.lock().await;
                    pending.remove(&request_id)
                };

                if let Some(pr) = pending_request {
                    self.track_resolved_tool_use_id(&pr.request).await;
                    self.notify_control_request_resolved(&pr.request, &request_id)
                        .await;

                    if response.response.subtype == "error" {
                        let err_msg = response.response.error.unwrap_or_default();
                        let _ = pr.response_tx.send(Err(anyhow::anyhow!("{}", err_msg)));
                    } else {
                        let result = response.response.response.unwrap_or(serde_json::json!({}));
                        let _ = pr.response_tx.send(Ok(result));
                    }

                    if self.replay_user_messages {
                        let stdin_msg: StdinMessage = serde_json::from_value(message)?;
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
            "control_request" => {
                if self.handle_slash_command_control_request(&message).await? {
                    return Ok(None);
                }
                if self
                    .handle_terminal_approval_action_control_request(&message)
                    .await?
                {
                    return Ok(None);
                }
                if self
                    .handle_compact_conversation_control_request(&message)
                    .await?
                {
                    return Ok(None);
                }
                let stdin_msg: StdinMessage = serde_json::from_value(message)?;
                Ok(Some(stdin_msg))
            }
            "user" | "assistant" | "system" => {
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

    async fn handle_slash_command_control_request(
        &self,
        message: &serde_json::Value,
    ) -> Result<bool> {
        let Some(request) = message.get("request") else {
            return Ok(false);
        };
        if request.get("subtype").and_then(|v| v.as_str()) != Some("slash_command") {
            return Ok(false);
        }

        let request_id = control_request_id(message);
        let (raw_command, args) = parse_slash_command_request(request);
        let command = normalize_stream_json_slash_command(&raw_command);

        if command.is_empty() {
            self.emit_slash_command_error(
                request_id,
                "unknown",
                "unsupported_slash_command: missing command".to_string(),
            )
            .await;
            return Ok(true);
        }

        match command.as_str() {
            "help" => {
                if !args.is_empty() {
                    self.emit_slash_command_error(
                        request_id,
                        &command,
                        "unsupported_slash_command_args: help".to_string(),
                    )
                    .await;
                    return Ok(true);
                }
                self.emit_slash_command_success(request_id, slash_help_response())
                    .await;
            }
            "capabilities" => {
                if !args.is_empty() {
                    self.emit_slash_command_error(
                        request_id,
                        &command,
                        "unsupported_slash_command_args: capabilities".to_string(),
                    )
                    .await;
                    return Ok(true);
                }
                self.emit_slash_command_success(request_id, slash_capabilities_response())
                    .await;
            }
            "status" => {
                if !args.is_empty() {
                    self.emit_slash_command_error(
                        request_id,
                        &command,
                        "unsupported_slash_command_args: status".to_string(),
                    )
                    .await;
                    return Ok(true);
                }
                self.emit_slash_command_success(request_id, self.slash_status_response().await)
                    .await;
            }
            "model" => match slash_model_response(&args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "profile" => match slash_profile_response(&args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "mcp" => match slash_mcp_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "ide" => match slash_ide_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "init" => match slash_init_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "login" | "logout" => match slash_auth_response(&command, &args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "diff" => match slash_diff_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "context" => match slash_context_response(&args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "config" => match slash_config_response(&args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "doctor" => match self.slash_doctor_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "approvals" => match self.slash_approvals_response(&args).await {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "cost" | "hooks" | "memory" | "skills" | "plugin" | "agents" => {
                match slash_readonly_runtime_inventory_response(&command, &args) {
                    Ok(response) => self.emit_slash_command_success(request_id, response).await,
                    Err(error) => {
                        self.emit_slash_command_error(request_id, &command, error)
                            .await;
                    }
                }
            }
            "permissions" => {
                if let Some(response) = slash_permissions_response(&args) {
                    self.emit_slash_command_success(request_id, response).await;
                } else {
                    self.emit_slash_command_error(
                        request_id,
                        &command,
                        "unsupported_slash_command_args: permissions".to_string(),
                    )
                    .await;
                }
            }
            "plan" => match slash_plan_response(&args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "compact" => match build_compact_slash_response(request_id.clone(), &args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            "clear" => match build_clear_slash_response(request_id.clone(), &args) {
                Ok(response) => self.emit_slash_command_success(request_id, response).await,
                Err(error) => {
                    self.emit_slash_command_error(request_id, &command, error)
                        .await;
                }
            },
            _ => {
                let error = match get_stream_json_slash_command_capability(&command) {
                    Some(capability) if matches!(&capability.status, CommandStatus::Blocked) => {
                        format!(
                            "blocked_slash_command: {}{}",
                            command,
                            capability
                                .reason
                                .as_deref()
                                .map(|reason| format!(" ({reason})"))
                                .unwrap_or_default()
                        )
                    }
                    Some(_) => format!(
                        "unavailable_slash_command: {} (runtime state is not attached to StructuredIO yet)",
                        command
                    ),
                    None => format!("unsupported_slash_command: {command}"),
                };
                self.emit_slash_command_error(request_id, &command, error)
                    .await;
            }
        }

        Ok(true)
    }

    async fn slash_approvals_response(
        &self,
        args: &[String],
    ) -> std::result::Result<serde_json::Value, String> {
        let action = args
            .first()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "status".to_string());
        if args.len() > 1 {
            return Err("unsupported_slash_command_args: approvals".to_string());
        }
        if !matches!(
            action.as_str(),
            "status" | "summary" | "pending" | "history"
        ) {
            return Err(format!(
                "unsupported_slash_command_args: approvals {action}"
            ));
        }

        let pending_requests = self.get_pending_permission_requests().await;
        let pending = pending_requests
            .iter()
            .map(terminal_approval_pending_request_entry)
            .collect::<Vec<_>>();
        let runtime = snapshot_agent_runtime_status();

        Ok(serde_json::json!({
            "subtype": "slash_command_result",
            "command": "approvals",
            "status": "completed",
            "approvals": {
                "action": action,
                "pendingCount": pending.len(),
                "pending": pending,
                "historyAvailable": true,
                "historySource": "runtime_status_snapshot",
                "historyEntriesIncluded": false,
                "fullHistoryIncluded": false,
                "decisions": {
                    "total": runtime.total_permission_decisions,
                    "mode": runtime.permission_mode_decisions,
                    "gate": runtime.permission_gate_decisions,
                    "notRequired": runtime.permission_not_required_decisions,
                    "allows": runtime.permission_allows,
                    "allowAlways": runtime.permission_allow_always,
                    "denies": runtime.permission_denies,
                },
                "lastDecision": {
                    "toolName": runtime.last_permission_tool_name,
                    "source": runtime.last_permission_source,
                    "decision": runtime.last_permission_decision,
                },
                "actions": terminal_approval_action_options(),
                "availableActions": [
                    TERMINAL_APPROVAL_ACTION_APPROVE_ONCE,
                    TERMINAL_APPROVAL_ACTION_REJECT,
                    TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION,
                    TERMINAL_APPROVAL_ACTION_EDIT_COMMAND,
                ],
                "actionControlSubtype": "terminal_approval_action",
                "resolveViaControlRequest": true,
                "mutationSupported": false,
                "rawPayloadsRedacted": true,
                "inputsRedacted": true,
                "descriptionsRedacted": true,
                "pathsRedacted": true,
            },
        }))
    }

    async fn slash_doctor_response(
        &self,
        args: &[String],
    ) -> std::result::Result<serde_json::Value, String> {
        let action = args
            .first()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "status".to_string());
        if args.len() > 1 {
            return Err("unsupported_slash_command_args: doctor".to_string());
        }
        if !matches!(
            action.as_str(),
            "status" | "summary" | "runtime" | "render" | "slash" | "mcp"
        ) {
            return Err(format!("unsupported_slash_command_args: doctor {action}"));
        }

        let status_response = self.slash_status_response().await;
        let runtime = status_response
            .get("runtime")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let render = runtime
            .get("render")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let agent = runtime
            .get("agent")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let slash = runtime
            .get("slash")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mcp_response = slash_mcp_response(&[]).await?;
        let mcp = mcp_response
            .get("mcp")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let memory_runtime = slash_memory_runtime_snapshot();
        let memory_compact = memory_runtime
            .get("compact")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let model_config = config_doctor::model_config_doctor_snapshot();

        let render_ready = json_bool(&render, "event_stream")
            && json_bool(&render, "snapshot_stream")
            && json_bool(&render, "frame_stream")
            && json_bool(&render, "patch_stream")
            && json_bool(&render, "draw_plan_stream")
            && json_bool(&render, "draw_executor")
            && json_bool(&render, "terminal_frontend");
        let last_error_present = agent
            .get("lastError")
            .and_then(|value| value.as_str())
            .is_some();
        let failed_mcp_count = json_u64(&mcp, "failedCount");
        let needs_auth_mcp_count = json_u64(&mcp, "needsAuthCount");
        let input_closed = json_bool(&runtime, "input_closed");
        let model_config_status = model_config.status.as_str();
        let health_status = if !render_ready
            || last_error_present
            || input_closed
            || failed_mcp_count > 0
            || matches!(model_config_status, "missing" | "warning")
        {
            "warning"
        } else {
            "normal"
        };

        Ok(serde_json::json!({
            "subtype": "slash_command_result",
            "command": "doctor",
            "status": "completed",
            "doctor": {
                "action": action,
                "healthStatus": health_status,
                "analysisDepth": "runtime_health_snapshot",
                "externalChecksRun": false,
                "networkChecksRun": false,
                "installChecksRun": false,
                "slowChecksSkipped": true,
                "blockingChecksSkipped": true,
                "runtime": {
                    "protocol": "stream_json",
                    "inputClosed": input_closed,
                    "pendingControlRequests": json_u64(&runtime, "pending_control_requests"),
                    "resolvedToolUseIds": json_u64(&runtime, "resolved_tool_use_ids"),
                    "pendingCompactRequest": json_bool(&runtime, "pending_compact_request"),
                    "pendingClearRequest": json_bool(&runtime, "pending_clear_request"),
                    "permissionMode": runtime.get("permission_mode").cloned().unwrap_or(serde_json::Value::Null),
                },
                "modelConfig": model_config,
                "slash": {
                    "manifestVersion": slash
                        .get("manifest_version")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    "wiredCommandCount": slash
                        .get("implemented_commands")
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0),
                    "doctorCommandWired": true,
                },
                "render": {
                    "ready": render_ready,
                    "eventStream": json_bool(&render, "event_stream"),
                    "snapshotStream": json_bool(&render, "snapshot_stream"),
                    "frameStream": json_bool(&render, "frame_stream"),
                    "patchStream": json_bool(&render, "patch_stream"),
                    "drawPlanStream": json_bool(&render, "draw_plan_stream"),
                    "drawExecutor": json_bool(&render, "draw_executor"),
                    "drawRuntimeQueue": json_bool(&render, "draw_runtime_queue"),
                    "drawRuntimeCoalescing": json_bool(&render, "draw_runtime_coalescing"),
                    "drawRuntimeResizeAware": json_bool(&render, "draw_runtime_resize_aware"),
                    "manualScrollHold": json_bool(&render, "draw_runtime_manual_scroll_hold"),
                    "manualScrollDeadlineSuppression": json_bool(
                        &render,
                        "draw_runtime_manual_scroll_deadline_suppression"
                    ),
                    "synchronizedUpdateFailClosed": json_bool(
                        &render,
                        "terminal_synchronized_update_fail_closed"
                    ),
                    "terminalFrontend": json_bool(&render, "terminal_frontend"),
                    "scrollbackTranscript": json_bool(&render, "terminal_scrollback_transcript"),
                    "viewportCollisionGuard": json_bool(&render, "terminal_viewport_collision_guard"),
                    "dynamicTopStack": json_bool(&render, "terminal_dynamic_top_stack"),
                    "criticalRegionTopPriority": json_bool(&render, "terminal_critical_region_top_priority"),
                },
                "agent": {
                    "activeDialogues": json_u64(&agent, "activeDialogues"),
                    "totalDialoguesStarted": json_u64(&agent, "totalDialoguesStarted"),
                    "totalDialoguesCompleted": json_u64(&agent, "totalDialoguesCompleted"),
                    "totalDialoguesFailed": json_u64(&agent, "totalDialoguesFailed"),
                    "totalToolCallsStarted": json_u64(&agent, "totalToolCallsStarted"),
                    "totalToolCallsCompleted": json_u64(&agent, "totalToolCallsCompleted"),
                    "totalToolCallsFailed": json_u64(&agent, "totalToolCallsFailed"),
                    "totalToolCallsDenied": json_u64(&agent, "totalToolCallsDenied"),
                    "totalPermissionDecisions": json_u64(&agent, "totalPermissionDecisions"),
                    "lastErrorPresent": last_error_present,
                    "lastErrorRedacted": true,
                },
                "mcp": {
                    "managerInstalled": json_bool(&mcp, "managerInstalled"),
                    "serverCount": json_u64(&mcp, "serverCount"),
                    "connectedCount": json_u64(&mcp, "connectedCount"),
                    "pendingCount": json_u64(&mcp, "pendingCount"),
                    "failedCount": failed_mcp_count,
                    "needsAuthCount": needs_auth_mcp_count,
                    "disabledCount": json_u64(&mcp, "disabledCount"),
                    "toolCount": json_u64(&mcp, "toolCount"),
                    "promptCount": json_u64(&mcp, "promptCount"),
                    "resourceCount": json_u64(&mcp, "resourceCount"),
                    "serverDetailsIncluded": false,
                    "rawConfigRedacted": true,
                    "errorDetailsRedacted": true,
                },
                "memory": memory_runtime,
                "compact": {
                    "autoCompactEnabled": json_bool(&memory_compact, "autoCompactEnabled"),
                    "sessionMemoryCompactEnabled": json_bool(
                        &memory_compact,
                        "sessionMemoryCompactEnabled"
                    ),
                    "pendingCompactRequest": json_bool(&runtime, "pending_compact_request"),
                    "slashBridge": true,
                    "contentIncluded": false,
                    "pathsRedacted": true,
                },
                "redaction": {
                    "pathsRedacted": true,
                    "secretsRedacted": true,
                    "envValuesRedacted": true,
                    "rawConfigRedacted": true,
                    "errorDetailsRedacted": true,
                    "installPathsRedacted": true,
                },
                "mutationSupported": false,
            },
        }))
    }

    async fn slash_status_response(&self) -> serde_json::Value {
        let pending_control_requests = self.pending_requests.lock().await.len();
        let resolved_tool_use_ids = self.resolved_tool_use_ids.lock().await.len();
        let input_closed = self.input_closed.load(std::sync::atomic::Ordering::Relaxed);
        let permission_mode = current_session_permission_mode().1;
        let compact = pending_compact_status();
        let clear = pending_clear_status();
        let pending_compact_request = compact
            .get("pending")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let pending_clear_request = clear
            .get("pending")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let mut draw_contract = serde_json::json!({
            "regions": ["status", "active", "footer"],
            "preferred_strategy": "patch_regions",
            "replace_whole_screen": false,
            "stable_scroll": true,
            "region_hashes": true,
            "changed_region_ids": true,
            "skip_unchanged_regions": true,
            "frame_hash_excludes_sequence": true,
            "patch_operations": true,
            "skip_duplicate_frames": true,
            "ansi_safe_lines": true,
            "max_patch_line_cells": STREAM_JSON_RENDER_PATCH_MAX_LINE_CELLS,
            "preserve_prompt_cursor": true,
            "anchored_draw_plan": true,
            "cursor_save_restore": true,
            "clear_stale_region_lines": true,
            "drop_superseded_frames": true,
            "synchronized_update": true,
            "absolute_row_moves": true,
            "line_wrap_guard": true,
            "no_newline_writes": true,
            "coalesced_runtime_queue": true,
            "throttle_deadline_flush": true,
            "resize_before_pending_flush": true,
            "manual_scroll_preserves_active_update": true,
            "terminal_frontend_emit_mode": true,
            "ndjson_ansi_isolation": true,
            "terminal_frontend_log_isolation": true,
            "terminal_scrollback_transcript_commit": true,
            "terminal_scrollback_append_once": true,
            "independent_approval_region": true,
            "approval_blocks_active_log": true,
            "approval_draw_plan_blocking_region": true,
            "independent_command_region": true,
            "command_output_summary_only": true,
            "independent_diff_region": true,
            "diff_collapsed_by_default": true,
            "independent_error_region": true,
            "layered_error_region": true,
            "independent_final_summary_region": true,
            "final_summary_terminal_region": true,
        });
        if let Some(map) = draw_contract.as_object_mut() {
            map.insert(
                "terminal_top_bottom_collision_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_independent_widget_suppresses_duplicate_active".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_retired_region_clear".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "manual_scroll_critical_draw_bypass".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "manual_scroll_deadline_suppression".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "manual_scroll_no_busy_retry".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "synchronized_update_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "independent_slash_result_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_bounded_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_lifecycle_retirement".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_preview_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_region_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_lifecycle_event_retire_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_region_render_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_region_patch_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_patch_idempotency_guards".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_patch_line_safety".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_patch_top_stack_layout".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "slash_result_event_patch_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_noncritical_widget_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_noncritical_scrollback_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_completion_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_manual_scroll_pending_supersession".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_viewport_width_adaptation_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_viewport_profile_report".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_footer_viewport_variants".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_viewport_line_variant_selection".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_anchored_region_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ops_prebudgeted_region_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_retired_region_clear_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_clear_ops_prebudgeted_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_noncritical_top_line_draw_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ops_noncritical_top_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cumulative_noncritical_top_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_noncritical_top_total_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_physical_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_write_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_clear_visible_rows_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_clear_rows_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_executor_budget_hard_caps".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_declaration_hard_cap".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_executor_zero_copy_budgeting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_lines_zero_copy_budgeting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_borrowed_patch_inputs".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_region_lines_no_preclone".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_owned_pending_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_pending_draw_plan_move_on_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_last_report_snapshot".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_report_counters".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_json".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_last_report_json_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_soak".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_diagnostics_export_env".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_diagnostics_json_file".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_diagnostics_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_manual_scroll_preserved_counter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_manual_scroll_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_manual_scroll_hold_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_resize_scroll_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_scroll_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_tail_hold_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_tail_hold_until_restore".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_teardown_release_counter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_teardown_release_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_teardown_release_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_resize_teardown_release_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_teardown_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_resize_interrupt_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_interrupt_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_bypass_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_bypasses_manual_scroll".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_reject_no_execute".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_approve_executes_and_renders"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_always_allow_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_always_allow_executes_and_renders"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_edit_command_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_edit_command_executes_updated_input"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_edit_cancel_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_edit_cancel_rejects_without_execute"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_resize_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_resize_approve_latest_viewport"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_active_scroll_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_survives_active_scroll_reject"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_reject".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_approve_executes"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_edit_command_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_edit_command_executes_updated_input"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_always_allow_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_always_allow_executes"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_output_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_manual_scroll_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_acceptance_gate_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w305".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w307".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w308".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w309".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w310".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w311".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w312".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w313".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w314".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w315".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w316".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w317".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w318".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w319".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w321".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w322".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_no_fullscreen_clear".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_no_fullscreen_clear_w104_w320".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_pagedown_live_tail_matrix_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_pagedown_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_wheel_down_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_output_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_mouse_scroll_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_output_resize_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_resize_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_output_resize_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_mouse_resize_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_interrupt_manual_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_interrupt_mouse_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_resize_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_interrupt_manual_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_interrupt_mouse_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_resize_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_end_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_end_live_tail_matrix_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_end_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_interrupt_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_interrupt_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_interrupt_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_cleanup_balance_pty_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_completion_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_scroll_resize_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_status_visible".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat_stops_after_finish".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_before_content".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_transcript_final_assistant_dedupe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_transcript_scrollback_appends_once".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_heartbeat_survives_metadata_update".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_no_empty_activity_during_slow_first_token".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_seeded_model_before_sdk_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_initial_status_model_not_unknown".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat_replace_active".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_metadata_no_redundant_waiting_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_text_first".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_no_byte_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_duplicate_final_assistant_preview_stable".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_assistant_no_byte_summary_flash".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_stable_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_no_row_growth".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_noncritical_scrollback_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_completion_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_draw_plan_only_dispatch".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_omits_snapshot_frame_patch_dispatch".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_immediate_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_bypasses_superseded_sequence".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_suppresses_duplicate_scrollback_append".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_event_pending_gate".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_burst_coalesced_before_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_pending_released_after_handle".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_event_pending_gate".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_burst_coalesced_before_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_pending_released_after_handle".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_manual_scroll_latest_state_coalescing".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_manual_scroll_opposite_state_supersedes_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_event_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_bypasses_low_priority_render_events".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_drops_superseded_low_priority_backlog".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_preserves_resize_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_reports_resize_follow_up".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_releases_manual_scroll_end".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_flushes_manual_scroll_pending_draw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_fairness_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_yields_to_sdk_and_permission".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_teardown_releases_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_teardown_flushes_pending_draw_after_release".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_soft_wrap_materialization_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_soft_wrap_budget_before_allocation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_soft_wrap_streaming_sanitizer".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_soft_wrap_sanitize_without_full_line_clone".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_op_execution_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_op_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cursor_restore_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_truncated_cursor_restore".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cursor_restore_on_write_error".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_synchronized_update_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_truncated_synchronized_update_close".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_text_byte_write_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_text_byte_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "semantic_color_plain_text_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "no_color_env_plain_text_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_grapheme_cluster_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_complex_unicode_width_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ascii_glyph_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_ascii_mode_policy".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ansi_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_osc_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_inline_control_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_carriage_return_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_backspace_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_control_char_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_tab_width_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_newline_write_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_bidi_control_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_format_control_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_directional_spoof_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_soft_wrap".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_dynamic_top_stack".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_event_pump".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_raw_mode_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_early_input_isolation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_ctrl_c_interrupt".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_interrupt_cancels_turn".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_interrupt_unblocks_approval".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_interrupt_cancels_tool_execution".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_shell_process_group_termination".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_bash_task_lifecycle".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_render_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_status_panel".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_expanded_panel".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_plan_status_panel".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "independent_plan_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_plan_progress_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_summary_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "independent_file_change_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_diff_separation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_file_change_context".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_error_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_error_detail_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_top_stack_clip_diagnostics".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_visible_top_budget_report".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_critical_region_top_priority".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_resize_events".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_manual_scroll_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_native_mouse_scroll".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture_opt_in".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture_default_off".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_key_release_filter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_bracketed_paste_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_edit_command_paste".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_semantic_colors".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_plain_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_no_color_env_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_dumb_terminal_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_clicolor_zero_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_grapheme_cluster_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_complex_unicode_width_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ascii_glyph_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_ascii_mode_policy".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ansi_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_osc_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_inline_control_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_carriage_return_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_backspace_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_control_char_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_tab_width_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_newline_write_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_bidi_control_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_format_control_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_directional_spoof_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_reset_after_line".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_reset_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_write_error_reset".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_rich_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_model_mode_reasoning".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_context_usage".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_width_variants".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_command_history".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_verification_results".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_residual_risks".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_bounded_command_history".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_preview_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_log_collapse_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_stream_tail_buffer".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_stream_chunk_accounting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_bounded_tail_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_file_summary_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_hunk_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_collapsed_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unified_diff_file_sections".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_file_grouped_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_per_file_hunk_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_expand_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_expanded_preview_budgets".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_expansion_immediate_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_keymap_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_hint_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_hint_overflow_count".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_full_hints_snapshot".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_contextual_interaction_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_key_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_model".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_focus_navigation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_action".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_control_request".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_updated_input".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_edit_command_input".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_edit_command_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "permission_decision_updated_input_execution".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_action".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_activation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_enter_select".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_shortcut_actions".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_intent_model".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_bridge_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_approve_once_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_reject_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_interactive_gate_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_decision_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_submitted_nonblocking".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_submitted_retires_blocking_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_allow_always_session_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_interactive_gate_scoped_allow_always".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_interactive_gate_exact_command_rule".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_rule_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_rule_updates".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_input_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_bounded_input_preview".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        let ordering = serde_json::json!({
            "monotonic_event_sequence": true,
            "source_message_sequence": true,
            "event_index_in_source": true,
            "emitted_at_ms": true,
        });
        let mut render = serde_json::json!({
            "event_stream": true,
            "event_type": STREAM_JSON_RENDER_EVENT_TYPE,
            "schema_version": STREAM_JSON_RENDER_EVENT_SCHEMA_VERSION,
            "snapshot_stream": true,
            "snapshot_type": STREAM_JSON_RENDER_SNAPSHOT_TYPE,
            "snapshot_schema_version": STREAM_JSON_RENDER_SNAPSHOT_SCHEMA_VERSION,
            "frame_stream": true,
            "frame_type": STREAM_JSON_RENDER_FRAME_TYPE,
            "frame_schema_version": STREAM_JSON_RENDER_FRAME_SCHEMA_VERSION,
            "patch_stream": true,
            "patch_type": STREAM_JSON_RENDER_PATCH_TYPE,
            "patch_schema_version": STREAM_JSON_RENDER_PATCH_SCHEMA_VERSION,
            "draw_plan_stream": true,
            "draw_plan_type": STREAM_JSON_RENDER_DRAW_PLAN_TYPE,
            "draw_plan_schema_version": STREAM_JSON_RENDER_DRAW_PLAN_SCHEMA_VERSION,
            "draw_executor": true,
            "draw_executor_backend": "crossterm",
            "draw_runtime_queue": true,
            "draw_runtime_coalescing": true,
            "draw_runtime_resize_aware": true,
            "draw_runtime_manual_scroll_hold": true,
            "draw_runtime_manual_scroll_critical_bypass": true,
            "terminal_frontend": true,
            "terminal_frontend_emit": "terminal",
            "terminal_frontend_transport_isolated": true,
            "terminal_frontend_log_isolated": true,
            "terminal_scrollback_transcript": true,
            "terminal_scrollback_soft_wrap": true,
            "terminal_approval_widget": true,
            "terminal_command_widget": true,
            "terminal_diff_widget": true,
            "terminal_error_widget": true,
            "terminal_final_summary_widget": true,
            "terminal_viewport_collision_guard": true,
            "terminal_dynamic_top_stack": true,
            "terminal_critical_region_top_priority": true,
            "draw_contract": draw_contract,
            "raw_sdk_messages": true,
            "throttle_ms": STREAM_JSON_RENDER_EVENT_THROTTLE_MS,
            "ordering": ordering,
        });
        if let Some(map) = render.as_object_mut() {
            map.insert(
                "terminal_retired_region_clear".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_event_pump".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_synchronized_update_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "draw_executor_error_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "draw_runtime_noncritical_scrollback_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_completion_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "draw_runtime_manual_scroll_deadline_suppression".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "draw_runtime_manual_scroll_no_busy_retry".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_plan_widget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_plan_status_panel".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_bounded_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_lifecycle_retirement".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_preview_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_region_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_lifecycle_event_retire_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_region_render_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_region_patch_payload".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_patch_idempotency_guards".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_patch_line_safety".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_patch_top_stack_layout".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_slash_result_event_patch_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_patch_manual_scroll_policy".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_noncritical_scrollback_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_completion_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_patch_pending_supersession_policy".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_viewport_width_adaptation_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_viewport_profile_report".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_footer_viewport_variants".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_viewport_line_variant_selection".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_anchored_region_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ops_prebudgeted_region_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_retired_region_clear_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_clear_ops_prebudgeted_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_noncritical_top_line_draw_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ops_noncritical_top_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cumulative_noncritical_top_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_noncritical_top_total_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_physical_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_write_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_clear_visible_rows_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_clear_rows_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_executor_budget_hard_caps".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_declaration_hard_cap".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_executor_zero_copy_budgeting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_lines_zero_copy_budgeting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_borrowed_patch_inputs".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_plan_region_lines_no_preclone".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_owned_pending_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_pending_draw_plan_move_on_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_last_report_snapshot".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_report_counters".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_json".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_last_report_json_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_soak".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_diagnostics_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_diagnostics_export_env".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_diagnostics_json_file".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_diagnostics_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_manual_scroll_preserved_counter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_manual_scroll_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_manual_scroll_hold_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_oneshot_resize_scroll_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_scroll_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_tail_hold_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_tail_hold_until_restore".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_draw_runtime_teardown_release_counter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_teardown_release_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_teardown_release_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_resize_teardown_release_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_teardown_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_resize_interrupt_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_resize_interrupt_latest_viewport".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_bypass_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_bypasses_manual_scroll".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_reject_no_execute".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_approve_executes_and_renders"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_always_allow_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_always_allow_executes_and_renders"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_edit_command_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_edit_command_executes_updated_input"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_edit_cancel_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_edit_cancel_rejects_without_execute"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_resize_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_resize_approve_latest_viewport"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_approval_active_scroll_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_survives_active_scroll_reject"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_reject_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_reject".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_approve_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_approve_executes"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_edit_command_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_edit_command_executes_updated_input"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_approval_always_allow_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_approval_mouse_scroll_always_allow_executes"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_output_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_manual_scroll_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_acceptance_gate_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w305".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w307".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w308".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w309".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w310".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w311".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w312".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w313".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w314".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w315".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w316".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w317".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w318".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w319".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w321".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_matrix_w288_w322".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_no_fullscreen_clear".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_product_external_pty_no_fullscreen_clear_w104_w320".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_pagedown_live_tail_matrix_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_pagedown_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_wheel_down_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_output_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_mouse_scroll_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_output_resize_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_resize_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_output_resize_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_output_mouse_resize_hold_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_interrupt_manual_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_interrupt_mouse_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_resize_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_interrupt_manual_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_interrupt_mouse_scroll_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_resize_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_mouse_resize_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_end_live_tail_release_after_approval"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_command_end_live_tail_matrix_after_approval_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_command_end_live_tail_release_after_approval_matrix"
                    .to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_interrupt_diagnostics_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_interrupt_no_stuck_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_interrupt_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_cleanup_balance_pty_contract".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_completion_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_external_process_scroll_resize_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_status_visible".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat_stops_after_finish".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_pty_smoke".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_before_content".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_slow_first_token_interrupt_cleanup_balanced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_transcript_final_assistant_dedupe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_transcript_scrollback_appends_once".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_heartbeat_survives_metadata_update".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_no_empty_activity_during_slow_first_token".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_seeded_model_before_sdk_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_initial_status_model_not_unknown".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_status_heartbeat_replace_active".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_metadata_no_redundant_waiting_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_text_first".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_no_byte_summary".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_duplicate_final_assistant_preview_stable".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_final_assistant_no_byte_summary_flash".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_stable_line_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_assistant_activity_no_row_growth".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_noncritical_scrollback_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_render_completion_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_draw_plan_only_dispatch".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_omits_snapshot_frame_patch_dispatch".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_immediate_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_bypasses_superseded_sequence".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_suppresses_duplicate_scrollback_append".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_event_pending_gate".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_burst_coalesced_before_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_resize_pending_released_after_handle".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_event_pending_gate".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_burst_coalesced_before_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scroll_pending_released_after_handle".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_manual_scroll_latest_state_coalescing".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_manual_scroll_opposite_state_supersedes_pending".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_event_queue".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_bypasses_low_priority_render_events".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_drops_superseded_low_priority_backlog".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_preserves_resize_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_reports_resize_follow_up".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_releases_manual_scroll_end".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_priority_drain_flushes_manual_scroll_pending_draw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_fairness_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_priority_yields_to_sdk_and_permission".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_teardown_releases_manual_scroll_hold".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_teardown_flushes_pending_draw_after_release".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_soft_wrap_materialization_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_soft_wrap_budget_before_allocation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_scrollback_soft_wrap_streaming_sanitizer".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_soft_wrap_sanitize_without_full_line_clone".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_op_execution_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_op_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cursor_restore_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_truncated_cursor_restore".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_cursor_restore_on_write_error".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_synchronized_update_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_budget_truncated_synchronized_update_close".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_text_byte_write_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_text_byte_budget_executor_enforced".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_background_task_expanded_panel".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_summary_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_diff_separation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_file_change_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_file_change_context".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_error_expansion_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_error_detail_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_top_stack_clip_diagnostics".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_visible_top_budget_report".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_raw_mode_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_early_input_isolation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_ctrl_c_interrupt".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_interrupt_cancels_turn".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_interrupt_unblocks_approval".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_resize_events".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_manual_scroll_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_native_mouse_scroll".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture_opt_in".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_mouse_capture_default_off".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_key_release_filter".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_bracketed_paste_capture".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_frontend_edit_command_paste".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_semantic_colors".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_plain_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_no_color_env_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_dumb_terminal_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_color_clicolor_zero_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_grapheme_cluster_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_complex_unicode_width_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ascii_glyph_fallback".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_ascii_mode_policy".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_ansi_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_osc_control_sequence_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_inline_control_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_carriage_return_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_backspace_progress_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_control_char_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_tab_width_normalization".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_newline_write_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_bidi_control_strip".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unicode_format_control_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_directional_spoof_guard".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_reset_after_line".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_reset_fail_safe".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_style_write_error_reset".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_rich_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_model_mode_reasoning".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_context_usage".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_status_bar_width_variants".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_command_history".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_verification_results".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_residual_risks".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_final_summary_bounded_command_history".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_preview_lines".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_log_collapse_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_stream_tail_buffer".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_stream_chunk_accounting".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_bounded_tail_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_file_summary_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_hunk_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_collapsed_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_unified_diff_file_sections".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_file_grouped_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_per_file_hunk_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_expand_controls".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_command_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_diff_expand_collapse".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_expanded_preview_budgets".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_expansion_immediate_redraw".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_keymap_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_hint_budget".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_hint_overflow_count".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_footer_full_hints_snapshot".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_contextual_interaction_metadata".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_widget_key_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_hints".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_model".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_focus_navigation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_action".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_control_request".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_updated_input".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_edit_command_input".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_edit_command_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "permission_decision_updated_input_execution".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_action".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_activation".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_enter_select".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_shortcut_actions".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_action_intent_model".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_decision_bridge_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_approve_once_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_reject_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_interactive_gate_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_local_decision_submit".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_submitted_nonblocking".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_submitted_retires_blocking_region".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_allow_always_session_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_interactive_gate_scoped_allow_always".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_interactive_gate_exact_command_rule".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_rule_bridge".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_session_rule_updates".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_edit_command_fail_closed".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_input_preview".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert(
                "terminal_approval_bounded_input_preview".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        serde_json::json!({
            "subtype": "slash_command_result",
            "command": "status",
            "status": "completed",
            "runtime": {
                "protocol": "stream_json",
                "input_closed": input_closed,
                "pending_control_requests": pending_control_requests,
                "resolved_tool_use_ids": resolved_tool_use_ids,
                "pending_compact_request": pending_compact_request,
                "pending_clear_request": pending_clear_request,
                "permission_mode": permission_mode.as_str(),
                "queues": {
                    "pending_control_requests": pending_control_requests,
                    "resolved_tool_use_ids": resolved_tool_use_ids,
                },
                "compact": compact,
                "clear": clear,
                "permissions": {
                    "mode": permission_mode.as_str(),
                    "mode_label": permission_mode_label(permission_mode),
                },
                "slash": {
                    "manifest_version": STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION,
                    "implemented_commands": wired_stream_json_slash_commands(),
                },
                "render": render,
                "agent": snapshot_agent_runtime_status(),
            },
        })
    }

    async fn emit_slash_command_success(&self, request_id: String, response: serde_json::Value) {
        let _ = self
            .outbound
            .send(StdoutMessage::ControlResponse(SDKControlResponse {
                msg_type: "control_response".to_string(),
                response: ControlResponsePayload {
                    request_id: request_id.clone(),
                    subtype: "slash_command_result".to_string(),
                    response: Some(response.clone()),
                    error: None,
                },
                uuid: None,
            }))
            .await;
        self.emit_slash_command_render_items(&request_id, &response, None)
            .await;
    }

    async fn emit_slash_command_error(&self, request_id: String, command: &str, error: String) {
        let response = serde_json::json!({
            "subtype": "slash_command_result",
            "command": command,
            "status": "error",
        });
        let _ = self
            .outbound
            .send(StdoutMessage::ControlResponse(SDKControlResponse {
                msg_type: "control_response".to_string(),
                response: ControlResponsePayload {
                    request_id: request_id.clone(),
                    subtype: "error".to_string(),
                    response: Some(response.clone()),
                    error: Some(error.clone()),
                },
                uuid: None,
            }))
            .await;
        self.emit_slash_command_render_items(&request_id, &response, Some(&error))
            .await;
    }

    async fn emit_slash_command_render_items(
        &self,
        request_id: &str,
        response: &serde_json::Value,
        error: Option<&str>,
    ) {
        let Some(render_event_emitter) = self.render_event_emitter.as_ref() else {
            return;
        };
        let render_items = {
            let mut emitter = render_event_emitter.lock().await;
            emitter.emit_slash_command_result_items(request_id, response, error)
        };
        for item in render_items {
            if self
                .outbound
                .send(StdoutMessage::StreamEvent(item))
                .await
                .is_err()
            {
                break;
            }
        }
    }

    async fn handle_terminal_approval_action_control_request(
        &self,
        message: &serde_json::Value,
    ) -> Result<bool> {
        let Some(request) = message.get("request") else {
            return Ok(false);
        };
        if request.get("subtype").and_then(|v| v.as_str()) != Some("terminal_approval_action") {
            return Ok(false);
        }

        let request_id = control_request_id(message);
        let action_id = terminal_approval_action_id_from_request(request);
        if action_id.is_empty() {
            self.emit_terminal_approval_action_error(
                request_id,
                "unknown",
                "unsupported_terminal_approval_action: missing action".to_string(),
            )
            .await;
            return Ok(true);
        }

        let edited_input = terminal_approval_edited_input_from_request(request);
        match self
            .resolve_pending_permission_with_approval_action_input(&action_id, edited_input)
            .await
        {
            Ok(Some(response)) => {
                let resolved_request_id = response.response.request_id.clone();
                let decision = response.response.response.clone();
                self.emit_terminal_approval_action_success(
                    request_id,
                    &action_id,
                    resolved_request_id,
                    decision,
                )
                .await;
            }
            Ok(None) => {
                self.emit_terminal_approval_action_error(
                    request_id,
                    &action_id,
                    "no_pending_terminal_approval_request".to_string(),
                )
                .await;
            }
            Err(error) => {
                self.emit_terminal_approval_action_error(request_id, &action_id, error.to_string())
                    .await;
            }
        }

        Ok(true)
    }

    async fn emit_terminal_approval_action_success(
        &self,
        request_id: String,
        action_id: &str,
        resolved_request_id: String,
        decision: Option<serde_json::Value>,
    ) {
        let _ = self
            .outbound
            .send(StdoutMessage::ControlResponse(SDKControlResponse {
                msg_type: "control_response".to_string(),
                response: ControlResponsePayload {
                    request_id,
                    subtype: "terminal_approval_action_result".to_string(),
                    response: Some(serde_json::json!({
                        "subtype": "terminal_approval_action_result",
                        "action": action_id,
                        "status": "submitted",
                        "resolved_request_id": resolved_request_id,
                        "decision": decision,
                    })),
                    error: None,
                },
                uuid: None,
            }))
            .await;
    }

    async fn emit_terminal_approval_action_error(
        &self,
        request_id: String,
        action_id: &str,
        error: String,
    ) {
        let _ = self
            .outbound
            .send(StdoutMessage::ControlResponse(SDKControlResponse {
                msg_type: "control_response".to_string(),
                response: ControlResponsePayload {
                    request_id,
                    subtype: "error".to_string(),
                    response: Some(serde_json::json!({
                        "subtype": "terminal_approval_action_result",
                        "action": action_id,
                        "status": "error",
                    })),
                    error: Some(error),
                },
                uuid: None,
            }))
            .await;
    }

    async fn handle_compact_conversation_control_request(
        &self,
        message: &serde_json::Value,
    ) -> Result<bool> {
        let Some(request) = message.get("request") else {
            return Ok(false);
        };
        if request.get("subtype").and_then(|v| v.as_str()) != Some("compact_conversation") {
            return Ok(false);
        }

        let request_id = control_request_id(message);
        let mode = request
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");
        let dry_run = request
            .get("dry_run")
            .or_else(|| request.get("dryRun"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let custom_instructions = request
            .get("custom_instructions")
            .or_else(|| request.get("customInstructions"))
            .or_else(|| request.get("instructions"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let (status, reason) = if mode != "manual" {
            (
                "blocked",
                Some(format!(
                    "unsupported compact mode '{}'; only manual is supported",
                    mode
                )),
            )
        } else {
            match enqueue_pending_compact_request(
                request_id.clone(),
                CompactMode::Manual,
                dry_run,
                custom_instructions,
            ) {
                Ok(()) => ("queued", None),
                Err(err) => ("blocked", Some(err)),
            }
        };

        let mut response = serde_json::json!({
            "status": status,
            "dry_run": dry_run,
        });
        if let Some(reason) = reason {
            response["reason"] = serde_json::Value::String(reason);
        }

        let _ = self
            .outbound
            .send(StdoutMessage::ControlResponse(SDKControlResponse {
                msg_type: "control_response".to_string(),
                response: ControlResponsePayload {
                    request_id,
                    subtype: "compact_conversation".to_string(),
                    response: Some(response),
                    error: None,
                },
                uuid: None,
            }))
            .await;

        Ok(true)
    }

    /// 发送消息到 stdout。
    pub async fn write(&self, message: StdoutMessage) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let safe = ndjson_safe_stringify(&json);
        println!("{}", safe);
        Ok(())
    }

    /// 发送控制请求并等待响应。
    pub async fn send_request(
        &self,
        request_payload: ControlRequestPayload,
        request_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let request_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        if self.input_closed.load(std::sync::atomic::Ordering::Relaxed) {
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
            Err(_) => Err(anyhow::anyhow!("request channel closed before response")),
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

fn permission_decision_for_approval_action(
    action_id: &str,
    request: Option<&SDKControlRequest>,
    edited_input: Option<serde_json::Value>,
) -> Result<PermissionDecision> {
    match action_id.trim() {
        TERMINAL_APPROVAL_ACTION_APPROVE_ONCE => Ok(PermissionDecision {
            behavior: "allow".to_string(),
            message: None,
            updated_input: None,
            user_modified: None,
            decision_reason: None,
            updated_permissions: None,
            suggestions: None,
            blocked_path: None,
        }),
        TERMINAL_APPROVAL_ACTION_REJECT => Ok(PermissionDecision {
            behavior: "deny".to_string(),
            message: Some("Rejected from terminal approval action".to_string()),
            updated_input: None,
            user_modified: None,
            decision_reason: None,
            updated_permissions: None,
            suggestions: None,
            blocked_path: None,
        }),
        TERMINAL_APPROVAL_ACTION_EDIT_COMMAND => {
            let request = request.context(
                "terminal approval action 'edit_command' requires pending permission request",
            )?;
            Ok(PermissionDecision {
                behavior: "allow".to_string(),
                message: None,
                updated_input: Some(terminal_edit_command_updated_input_for_request(
                    request,
                    edited_input,
                )?),
                user_modified: Some(true),
                decision_reason: Some(serde_json::json!({
                    "type": "terminal_approval_action",
                    "action": TERMINAL_APPROVAL_ACTION_EDIT_COMMAND,
                    "userModified": true,
                })),
                updated_permissions: None,
                suggestions: None,
                blocked_path: None,
            })
        }
        TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION => {
            let request = request.context(
                "terminal approval action 'approve_for_session' requires pending permission request",
            )?;
            Ok(PermissionDecision {
                behavior: "allow".to_string(),
                message: None,
                updated_input: None,
                user_modified: None,
                decision_reason: Some(serde_json::json!({
                    "type": "terminal_approval_action",
                    "action": TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION,
                    "scope": "session",
                })),
                updated_permissions: Some(terminal_session_permission_updates_for_request(
                    request,
                )?),
                suggestions: None,
                blocked_path: None,
            })
        }
        other => anyhow::bail!("unsupported terminal approval action '{other}'"),
    }
}

fn terminal_edit_command_updated_input_for_request(
    request: &SDKControlRequest,
    edited_input: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let ControlRequestPayload::CanUseTool {
        tool_name, input, ..
    } = &request.request
    else {
        anyhow::bail!("terminal edit command approval requires a can_use_tool request")
    };

    let edited_input = edited_input
        .context("terminal approval action 'edit_command' requires updatedInput or command")?;

    match edited_input {
        serde_json::Value::String(command) => {
            if !matches!(tool_name.as_str(), "Bash" | "PowerShell" | "Execute") {
                anyhow::bail!(
                    "terminal edit command string input only supports shell tools; pass updatedInput object"
                );
            }
            let command = terminal_permission_rule_text(&command);
            if command.is_empty() {
                anyhow::bail!("terminal edit command cannot submit an empty command");
            }
            let mut next_input = input.clone();
            match next_input {
                serde_json::Value::Object(ref mut object) => {
                    object.insert("command".to_string(), serde_json::Value::String(command));
                    Ok(next_input)
                }
                _ => Ok(serde_json::json!({ "command": command })),
            }
        }
        serde_json::Value::Object(object) => {
            if object.is_empty() {
                anyhow::bail!("terminal edit command updatedInput cannot be empty");
            }
            Ok(serde_json::Value::Object(object))
        }
        _ => {
            anyhow::bail!("terminal edit command updatedInput must be an object or command string")
        }
    }
}

fn terminal_session_permission_updates_for_request(
    request: &SDKControlRequest,
) -> Result<Vec<serde_json::Value>> {
    let ControlRequestPayload::CanUseTool {
        tool_name,
        input,
        permission_suggestions,
        ..
    } = &request.request
    else {
        anyhow::bail!("terminal session approval requires a can_use_tool request")
    };

    let suggested_updates =
        terminal_session_permission_updates_from_suggestions(permission_suggestions.as_deref());
    if !suggested_updates.is_empty() {
        return Ok(suggested_updates);
    }

    Ok(vec![terminal_session_allow_rule_update(
        tool_name,
        terminal_session_rule_content_for_input(tool_name, input),
    )])
}

fn terminal_session_permission_updates_from_suggestions(
    suggestions: Option<&[serde_json::Value]>,
) -> Vec<serde_json::Value> {
    suggestions
        .unwrap_or(&[])
        .iter()
        .filter_map(terminal_session_permission_update_from_suggestion)
        .collect()
}

fn terminal_session_permission_update_from_suggestion(
    suggestion: &serde_json::Value,
) -> Option<serde_json::Value> {
    let update_type = suggestion.get("type").and_then(|value| value.as_str())?;
    if update_type != "addRules" {
        return None;
    }
    let behavior = suggestion
        .get("behavior")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if behavior != "allow" {
        return None;
    }
    let rules = suggestion
        .get("rules")
        .and_then(|value| value.as_array())?
        .iter()
        .filter_map(terminal_session_permission_rule_from_value)
        .collect::<Vec<_>>();
    if rules.is_empty() {
        return None;
    }

    Some(serde_json::json!({
        "type": "addRules",
        "destination": "session",
        "behavior": "allow",
        "rules": rules,
    }))
}

fn terminal_session_permission_rule_from_value(
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    let tool_name = value.get("toolName").and_then(|value| value.as_str())?;
    let tool_name = terminal_permission_rule_text(tool_name);
    if tool_name.is_empty() {
        return None;
    }
    let mut rule = serde_json::json!({
        "toolName": tool_name,
    });
    if let Some(rule_content) = value.get("ruleContent").and_then(|value| value.as_str()) {
        let rule_content = terminal_permission_rule_text(rule_content);
        if !rule_content.is_empty() {
            rule["ruleContent"] = serde_json::Value::String(rule_content);
        }
    }
    Some(rule)
}

fn terminal_session_allow_rule_update(
    tool_name: &str,
    rule_content: Option<String>,
) -> serde_json::Value {
    let mut rule = serde_json::json!({
        "toolName": terminal_permission_rule_text(tool_name),
    });
    if let Some(rule_content) = rule_content {
        let rule_content = terminal_permission_rule_text(&rule_content);
        if !rule_content.is_empty() {
            rule["ruleContent"] = serde_json::Value::String(rule_content);
        }
    }

    serde_json::json!({
        "type": "addRules",
        "destination": "session",
        "behavior": "allow",
        "rules": [rule],
    })
}

fn terminal_session_rule_content_for_input(
    tool_name: &str,
    input: &serde_json::Value,
) -> Option<String> {
    match tool_name {
        "Bash" | "PowerShell" | "Execute" => input
            .get("command")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn terminal_approval_pending_request_entry(request: &SDKControlRequest) -> serde_json::Value {
    let ControlRequestPayload::CanUseTool {
        tool_name,
        input,
        permission_suggestions,
        blocked_path,
        decision_reason,
        tool_use_id,
        agent_id,
        description,
    } = &request.request
    else {
        return serde_json::json!({
            "requestId": request.request_id,
            "unsupportedRequestType": true,
        });
    };

    let (input_preview, input_preview_truncated) =
        terminal_approval_pending_input_preview(tool_name, input);
    serde_json::json!({
        "requestId": request.request_id,
        "toolName": terminal_permission_rule_text(tool_name),
        "toolUseId": tool_use_id,
        "agentId": agent_id,
        "descriptionPresent": description
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        "descriptionRedacted": description.is_some(),
        "blockedPathPresent": blocked_path.is_some(),
        "permissionSuggestionsPresent": permission_suggestions
            .as_ref()
            .map(|values| !values.is_empty())
            .unwrap_or(false),
        "decisionReasonPresent": decision_reason.is_some(),
        "inputPreview": input_preview,
        "inputPreviewTruncated": input_preview_truncated,
        "inputRedacted": true,
        "rawInputIncluded": false,
        "availableActions": [
            TERMINAL_APPROVAL_ACTION_APPROVE_ONCE,
            TERMINAL_APPROVAL_ACTION_REJECT,
            TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION,
            TERMINAL_APPROVAL_ACTION_EDIT_COMMAND,
        ],
    })
}

fn terminal_approval_pending_input_preview(
    tool_name: &str,
    input: &serde_json::Value,
) -> (String, bool) {
    let raw_preview = match tool_name {
        "Bash" | "PowerShell" | "Execute" => input
            .get("command")
            .and_then(|value| value.as_str())
            .map(terminal_permission_rule_text)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "command input redacted".to_string()),
        _ => match input {
            serde_json::Value::Object(object) => format!("structured input: {} keys", object.len()),
            serde_json::Value::Array(values) => format!("array input: {} items", values.len()),
            serde_json::Value::String(value) => {
                if value.trim().is_empty() {
                    "string input redacted".to_string()
                } else {
                    "string input present".to_string()
                }
            }
            serde_json::Value::Null => "no input".to_string(),
            _ => "scalar input present".to_string(),
        },
    };
    truncate_terminal_approval_preview(&raw_preview, TERMINAL_APPROVAL_INPUT_PREVIEW_MAX_CHARS)
}

fn truncate_terminal_approval_preview(raw: &str, max_chars: usize) -> (String, bool) {
    let mut preview = String::new();
    let mut truncated = false;
    for (index, ch) in raw.chars().enumerate() {
        if index >= max_chars {
            truncated = true;
            break;
        }
        preview.push(ch);
    }
    (preview, truncated)
}

fn terminal_approval_action_options() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": TERMINAL_APPROVAL_ACTION_APPROVE_ONCE,
            "label": "Approve once",
            "submitsDecision": true,
            "updatesSessionRules": false,
            "requiresUpdatedInput": false,
        }),
        serde_json::json!({
            "id": TERMINAL_APPROVAL_ACTION_REJECT,
            "label": "Reject",
            "submitsDecision": true,
            "updatesSessionRules": false,
            "requiresUpdatedInput": false,
        }),
        serde_json::json!({
            "id": TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION,
            "label": "Approve for session",
            "submitsDecision": true,
            "updatesSessionRules": true,
            "requiresUpdatedInput": false,
        }),
        serde_json::json!({
            "id": TERMINAL_APPROVAL_ACTION_EDIT_COMMAND,
            "label": "Edit command",
            "submitsDecision": true,
            "updatesSessionRules": false,
            "requiresUpdatedInput": true,
        }),
    ]
}

fn terminal_permission_rule_text(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn permission_decision_control_response(
    request_id: String,
    decision: PermissionDecision,
) -> Result<SDKControlResponse> {
    Ok(SDKControlResponse {
        msg_type: "control_response".to_string(),
        response: ControlResponsePayload {
            request_id,
            subtype: "success".to_string(),
            response: Some(serde_json::to_value(decision)?),
            error: None,
        },
        uuid: None,
    })
}

fn control_request_id(message: &serde_json::Value) -> String {
    message
        .get("request_id")
        .or_else(|| message.get("control_request_id"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn terminal_approval_action_id_from_request(request: &serde_json::Value) -> String {
    request
        .get("action")
        .or_else(|| request.get("action_id"))
        .or_else(|| request.get("actionId"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn terminal_approval_edited_input_from_request(
    request: &serde_json::Value,
) -> Option<serde_json::Value> {
    request
        .get("updatedInput")
        .or_else(|| request.get("updated_input"))
        .cloned()
        .or_else(|| {
            request
                .get("updatedCommand")
                .or_else(|| request.get("updated_command"))
                .or_else(|| request.get("command"))
                .and_then(|value| value.as_str())
                .map(|command| serde_json::Value::String(command.to_string()))
        })
}

fn parse_slash_command_request(request: &serde_json::Value) -> (String, Vec<String>) {
    let raw_command = request
        .get("command")
        .or_else(|| request.get("name"))
        .or_else(|| request.get("input"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let mut pieces = raw_command.split_whitespace();
    let command = pieces.next().unwrap_or("").to_string();
    let mut args = pieces.map(str::to_string).collect::<Vec<_>>();
    args.extend(parse_slash_args(request));
    (command, args)
}

fn parse_slash_args(request: &serde_json::Value) -> Vec<String> {
    let Some(raw_args) = request
        .get("args")
        .or_else(|| request.get("arguments"))
        .or_else(|| request.get("args_raw"))
        .or_else(|| request.get("argsRaw"))
    else {
        return Vec::new();
    };

    match raw_args {
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::trim))
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        serde_json::Value::String(value) => value
            .split_whitespace()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn slash_help_response() -> serde_json::Value {
    let commands = get_stream_json_slash_command_capabilities()
        .iter()
        .map(slash_help_command_entry)
        .collect::<Vec<_>>();
    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "help",
        "status": "completed",
        "commands": commands,
        "streamJsonCapabilities": {
            "manifestVersion": STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION,
            "available": format_available_stream_json_slash_commands(),
            "wired": wired_stream_json_slash_commands(),
        },
    })
}

fn slash_help_command_entry(capability: &SlashCommandCapability) -> serde_json::Value {
    serde_json::json!({
        "name": capability.command.as_str(),
        "title": capability.title.as_str(),
        "summary": capability.summary.as_str(),
        "supported": is_wired_stream_json_slash_command(&capability.command),
        "status": &capability.status,
        "read_only": capability.read_only,
        "requires_confirmation": capability.requires_confirmation,
        "accepted_args": &capability.accepted_args,
        "aliases": &capability.aliases,
        "capability": serialize_stream_json_slash_command_capability(capability),
    })
}

fn slash_capabilities_response() -> serde_json::Value {
    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "capabilities",
        "status": "completed",
        "manifestVersion": STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION,
        "capabilities": get_stream_json_slash_command_capability_manifest(),
        "handler": {
            "implemented_commands": wired_stream_json_slash_commands(),
            "unwired_known_commands_return_error": true,
        },
    })
}

fn slash_model_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    if args.is_empty()
        || (args.len() == 1
            && matches!(
                args[0].as_str(),
                "status" | "current" | "show" | "list" | "options"
            ))
    {
        return Ok(slash_model_summary_response("status", None));
    }

    let requested_model = if matches!(args.first().map(String::as_str), Some("set" | "use")) {
        args.iter()
            .skip(1)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let requested_model = terminal_permission_rule_text(&requested_model);
    if requested_model.is_empty() {
        return Err("unsupported_slash_command_args: model (missing model)".to_string());
    }

    if matches!(
        requested_model.as_str(),
        "reset" | "default" | "clear" | "none"
    ) {
        let previous = get_main_loop_model_override();
        set_main_loop_model_override(None);
        return Ok(slash_model_summary_response("reset", previous));
    }

    if requested_model.len() > 160 {
        return Err("unsupported_slash_command_args: model (model name too long)".to_string());
    }

    let profile_match = config_profiles::list_all_profiles()
        .into_iter()
        .find(|profile| profile.name == requested_model);
    if let Some(profile) = profile_match {
        let previous = get_main_loop_model_override();
        config_profiles::set_session_active_profile(&profile.name)
            .map_err(|error| format!("slash_command_model_profile_set_failed: {error}"))?;
        config_profiles::apply_profile_to_custom_backend_env(&profile);
        set_main_loop_model_override(Some(profile.profile.model.clone()));
        return Ok(slash_model_summary_response("set", previous));
    }

    let previous = get_main_loop_model_override();
    set_main_loop_model_override(Some(requested_model));
    Ok(slash_model_summary_response("set", previous))
}

fn slash_model_summary_response(
    action: &str,
    previous_override: Option<String>,
) -> serde_json::Value {
    let override_model = get_main_loop_model_override();
    let initial_model = get_initial_main_loop_model();
    let env_model = std::env::var("MOSSEN_CODE_MODEL")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());
    let settings_profile_count = config_profiles::get_profiles().len();
    let current_profile = config_profiles::get_current_profile();
    let default_profile = config_profiles::get_default_profile();
    let current_profile_name = current_profile.as_ref().map(|profile| profile.name.clone());
    let default_profile_name = default_profile.as_ref().map(|profile| profile.name.clone());
    let current_profile_model = current_profile
        .as_ref()
        .map(|profile| profile.profile.model.clone());
    let profiles = config_profiles::list_all_profiles()
        .iter()
        .map(|profile| {
            slash_profile_entry(
                profile,
                current_profile_name.as_deref(),
                default_profile_name.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let effective_model = override_model
        .clone()
        .or_else(|| initial_model.clone())
        .or_else(|| env_model.clone())
        .or_else(|| current_profile_model.clone());
    let model_strings = get_model_strings();
    let model_strings_keys = model_strings
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|object| {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let recognized_alias = override_model
        .as_deref()
        .map(is_model_alias)
        .unwrap_or(false);
    let source = if override_model.is_some() {
        "slash_command"
    } else if initial_model.is_some() {
        "initial"
    } else if env_model.is_some() {
        "env:MOSSEN_CODE_MODEL"
    } else if current_profile_model.is_some() {
        "profile"
    } else {
        "default"
    };

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "model",
        "status": "completed",
        "model": {
            "action": action,
            "override": override_model,
            "previousOverride": previous_override,
            "initial": initial_model,
            "env": env_model,
            "effective": effective_model,
            "source": source,
            "currentProfileName": current_profile_name,
            "defaultProfileName": default_profile_name,
            "profileCount": profiles.len(),
            "settingsProfileCount": settings_profile_count,
            "profiles": profiles,
            "recognizedAlias": recognized_alias,
            "availableAliases": MODEL_ALIASES,
            "modelStringsAvailable": model_strings.is_some(),
            "modelStringKeys": model_strings_keys,
            "mutationSupported": true,
            "switchAppliesToNextTurn": true,
        },
    })
}

fn slash_profile_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());

    match action.as_str() {
        "status" | "current" | "show" | "summary" | "list" | "profiles" | "options" => {
            if args.len() > 1 {
                return Err(format!("unsupported_slash_command_args: profile {action}"));
            }
            Ok(slash_profile_summary_response(&action, None, false))
        }
        "set" | "use" => {
            if args.len() != 2 {
                return Err(format!(
                    "unsupported_slash_command_args: profile {action} (expected profile name)"
                ));
            }
            let requested_profile = terminal_permission_rule_text(&args[1]);
            if requested_profile.is_empty() {
                return Err(format!(
                    "unsupported_slash_command_args: profile {action} (missing profile name)"
                ));
            }
            let previous = config_profiles::get_active_profile_name();
            config_profiles::set_session_active_profile(&requested_profile)
                .map_err(|error| format!("slash_command_profile_set_failed: {error}"))?;
            if let Some(profile) = config_profiles::get_current_profile() {
                config_profiles::apply_profile_to_custom_backend_env(&profile);
            }
            Ok(slash_profile_summary_response(&action, previous, true))
        }
        "reset" | "clear" | "default" => {
            if args.len() > 1 {
                return Err(format!("unsupported_slash_command_args: profile {action}"));
            }
            let previous = config_profiles::get_active_profile_name();
            config_profiles::clear_session_active_profile();
            Ok(slash_profile_summary_response("reset", previous, true))
        }
        _ => Err(format!("unsupported_slash_command_args: profile {action}")),
    }
}

fn slash_profile_summary_response(
    action: &str,
    previous_active_profile_name: Option<String>,
    mutation_performed: bool,
) -> serde_json::Value {
    let settings_profile_count = config_profiles::get_profiles().len();
    let fallback_profile_available = config_profiles::get_fallback_profile().is_some();
    let active_profile_name = config_profiles::get_active_profile_name();
    let current_profile = config_profiles::get_current_profile();
    let default_profile = config_profiles::get_default_profile();
    let current_profile_name = current_profile.as_ref().map(|profile| profile.name.clone());
    let default_profile_name = default_profile.as_ref().map(|profile| profile.name.clone());
    let profiles = config_profiles::list_all_profiles()
        .iter()
        .map(|profile| {
            slash_profile_entry(
                profile,
                current_profile_name.as_deref(),
                default_profile_name.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let current_profile = current_profile.as_ref().map(|profile| {
        slash_profile_entry(
            profile,
            current_profile_name.as_deref(),
            default_profile_name.as_deref(),
        )
    });
    let default_profile = default_profile.as_ref().map(|profile| {
        slash_profile_entry(
            profile,
            current_profile_name.as_deref(),
            default_profile_name.as_deref(),
        )
    });

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "profile",
        "status": "completed",
        "profile": {
            "action": action,
            "activeProfileName": active_profile_name,
            "currentProfileName": current_profile_name,
            "previousActiveProfileName": previous_active_profile_name,
            "defaultProfileName": default_profile_name,
            "profileCount": profiles.len(),
            "settingsProfileCount": settings_profile_count,
            "fallbackProfileAvailable": fallback_profile_available,
            "profiles": profiles,
            "currentProfile": current_profile,
            "defaultProfile": default_profile,
            "scope": "session",
            "mutationPerformed": mutation_performed,
            "mutationSupported": true,
            "switchAppliesToNextTurn": true,
            "writesConfigFiles": false,
            "rawConfigIncluded": false,
            "rawConfigRedacted": true,
            "apiKeysRedacted": true,
            "baseUrlsRedacted": true,
            "pathsRedacted": true,
        },
    })
}

fn slash_profile_entry(
    profile: &config_profiles::ListedProfile,
    current_profile_name: Option<&str>,
    default_profile_name: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "name": profile.name,
        "source": profile.source,
        "provider": profile.profile.provider,
        "model": profile.profile.model,
        "displayName": profile.profile.name,
        "active": current_profile_name == Some(profile.name.as_str()),
        "default": default_profile_name == Some(profile.name.as_str()),
        "baseUrlPresent": !profile.profile.base_url.trim().is_empty(),
        "baseUrlRedacted": true,
        "apiKeyPresent": !profile.profile.api_key.trim().is_empty(),
        "apiKeyRedacted": true,
    })
}

async fn slash_init_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "prompt".to_string());
    if args.len() > 1 {
        return Err("unsupported_slash_command_args: init".to_string());
    }
    if !matches!(
        action.as_str(),
        "status" | "summary" | "preview" | "prompt" | "run" | "start"
    ) {
        return Err(format!("unsupported_slash_command_args: init {action}"));
    }

    let cwd =
        std::env::current_dir().map_err(|_| "slash_command_init_cwd_unavailable".to_string())?;
    let prompt = slash_init_prompt().await?;
    let prompt_included = matches!(action.as_str(), "prompt" | "run" | "start");
    let prompt_preview = prompt
        .chars()
        .take(240)
        .collect::<String>()
        .replace('\n', " ");

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "init",
        "status": "completed",
        "init": {
            "action": action,
            "handoffType": "agent_prompt",
            "promptSubtype": "project_memory_init",
            "promptIncluded": prompt_included,
            "promptText": if prompt_included { Some(prompt.clone()) } else { None },
            "promptPreview": prompt_preview,
            "promptBytes": prompt.len(),
            "modelTurnRequired": true,
            "agentMayWriteFiles": true,
            "writesFilesDirectly": false,
            "usesNormalToolPermissions": true,
            "requiresToolApprovalForWrites": true,
            "targetFiles": ["MOSSEN.md", "MOSSEN.local.md", ".mossen/skills/*/SKILL.md"],
            "existingFiles": {
                "projectMossenMd": cwd.join("MOSSEN.md").exists(),
                "localMossenMd": cwd.join("MOSSEN.local.md").exists(),
                "mossenDirectory": cwd.join(".mossen").exists(),
            },
            "rawPathsIncluded": false,
            "pathsRedacted": true,
            "rawPromptConfigIncluded": false,
            "mutationSupported": true,
        },
    }))
}

async fn slash_init_prompt() -> std::result::Result<String, String> {
    let directive = mossen_commands::init::InitDirective;
    let context = slash_command_context()
        .map_err(|error| format!("slash_command_init_context_unavailable: {error}"))?;
    match directive
        .execute(&[], &context)
        .await
        .map_err(|error| format!("slash_command_init_prompt_failed: {error}"))?
    {
        CommandResult::Text(prompt) | CommandResult::System(prompt) => Ok(prompt),
        CommandResult::Empty => Err("slash_command_init_prompt_empty".to_string()),
        CommandResult::Widget => Err("slash_command_init_widget_unavailable".to_string()),
        CommandResult::Exit(message) => Err(format!(
            "slash_command_init_unexpected_exit{}",
            message
                .map(|message| format!(": {message}"))
                .unwrap_or_default()
        )),
        CommandResult::Error(error) => Err(format!("slash_command_init_prompt_error: {error}")),
    }
}

async fn slash_auth_response(
    command: &str,
    args: &[String],
) -> std::result::Result<serde_json::Value, String> {
    let raw_action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());
    if args.len() > 1 {
        return Err(format!("unsupported_slash_command_args: {command}"));
    }

    let action = match (command, raw_action.as_str()) {
        ("login", "status" | "summary" | "preview" | "prompt" | "run" | "start") => raw_action,
        ("logout", "status" | "summary" | "preview") => raw_action,
        ("logout", "--confirm" | "confirm") => "confirm".to_string(),
        ("login", _) => {
            return Err(format!(
                "unsupported_slash_command_args: login {raw_action}"
            ))
        }
        ("logout", _) => {
            return Err(format!(
                "unsupported_slash_command_args: logout {raw_action}"
            ));
        }
        _ => return Err(format!("unsupported_slash_command: {command}")),
    };

    let token_source = mossen_utils::auth::get_auth_token_source();
    let token_source_label = auth_token_source_label(&token_source.source);
    let api_key_present = mossen_utils::auth::has_mossen_api_key_auth();
    let oauth_token_present = mossen_utils::auth::get_hosted_oauth_tokens().is_some();
    let authenticated = token_source.has_token || api_key_present || oauth_token_present;
    let account_info_present = mossen_utils::auth::get_oauth_account_info().is_some();
    let manual_command = if command == "login" {
        vec!["mossen", "auth"]
    } else {
        vec!["mossen", "deauth"]
    };
    let equivalent_cli_command = manual_command.join(" ");
    let message = if command == "login" {
        slash_login_message().await?
    } else if authenticated {
        "A local credential source is present. This stream-json command reports status only; remove configured backend credentials from the owning environment or settings file.".to_string()
    } else {
        "No active authentication token was detected in this stream-json process.".to_string()
    };
    let confirmation_received = command == "logout" && action == "confirm";
    let handoff_type = if command == "login" {
        "backend_credential_setup"
    } else {
        "credential_status"
    };
    let next = if command == "login" {
        "configure a model profile or set MOSSEN_CODE_CUSTOM_BASE_URL plus MOSSEN_CODE_CUSTOM_API_KEY"
    } else if confirmation_received {
        "remove backend credentials from the owning environment or settings file if logout is required"
    } else {
        "use /logout --confirm to acknowledge status-only logout handling"
    };

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": command,
        "status": "completed",
        "auth": {
            "command": command,
            "action": action,
            "status": if authenticated { "authenticated" } else { "not_authenticated" },
            "authenticated": authenticated,
            "authTokenSource": token_source_label,
            "authTokenPresent": token_source.has_token,
            "apiKeyPresent": api_key_present,
            "oauthTokenPresent": oauth_token_present,
            "accountInfoPresent": account_info_present,
            "credentialMode": "personal_backend",
            "handoffType": handoff_type,
            "handoffRequired": false,
            "manualCommand": manual_command,
            "equivalentCliCommand": equivalent_cli_command,
            "requiresExternalInteractiveCli": false,
            "requiresConfirmation": command == "logout",
            "confirmationReceived": confirmation_received,
            "mutationSupported": false,
            "mutationPerformed": false,
            "writesAuthStateDirectly": false,
            "tokensRedacted": true,
            "apiKeyRedacted": true,
            "rawPathsIncluded": false,
            "pathsRedacted": true,
            "rawEnvValuesIncluded": false,
            "sensitiveValuesIncluded": false,
            "message": message,
            "next": next,
        },
    }))
}

async fn slash_login_message() -> std::result::Result<String, String> {
    let directive = mossen_commands::auth::AuthDirective;
    let context = slash_command_context()
        .map_err(|error| format!("slash_command_login_context_unavailable: {error}"))?;
    match directive
        .execute(&[], &context)
        .await
        .map_err(|error| format!("slash_command_login_prompt_failed: {error}"))?
    {
        CommandResult::Text(message) | CommandResult::System(message) => Ok(message),
        CommandResult::Empty => Err("slash_command_login_prompt_empty".to_string()),
        CommandResult::Widget => Err("slash_command_login_widget_unavailable".to_string()),
        CommandResult::Exit(message) => Err(format!(
            "slash_command_login_unexpected_exit{}",
            message
                .map(|message| format!(": {message}"))
                .unwrap_or_default()
        )),
        CommandResult::Error(error) => Err(format!("slash_command_login_prompt_error: {error}")),
    }
}

fn auth_token_source_label(source: &mossen_utils::auth::AuthTokenSource) -> &'static str {
    match source {
        mossen_utils::auth::AuthTokenSource::CustomBackend => "custom_backend",
        mossen_utils::auth::AuthTokenSource::MossenCodeAuthToken => "env_auth_token",
        mossen_utils::auth::AuthTokenSource::MossenCodeAuthTokenFileDescriptor => {
            "auth_token_file_descriptor"
        }
        mossen_utils::auth::AuthTokenSource::CcrOauthTokenFile => "oauth_token_file",
        mossen_utils::auth::AuthTokenSource::ApiKeyHelper => "api_key_helper",
        mossen_utils::auth::AuthTokenSource::Hosted => "legacy_stored_token",
        mossen_utils::auth::AuthTokenSource::None => "none",
    }
}

fn slash_command_context() -> std::result::Result<CommandContext, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    Ok(CommandContext {
        cwd,
        is_non_interactive: true,
        is_remote_mode: get_is_remote_mode(),
        is_custom_backend: std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").is_ok(),
        user_type: std::env::var("MOSSEN_CODE_USER_TYPE").ok(),
        env_vars: std::env::vars().collect(),
        product_name: "Mossen".to_string(),
        cli_name: "mossen".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_time: option_env!("BUILD_TIME").map(|value| value.to_string()),
        cost_snapshot: slash_command_cost_snapshot(),
    })
}

fn slash_command_cost_snapshot() -> CommandCostSnapshot {
    CommandCostSnapshot {
        total_cost_usd: get_total_cost_usd(),
        total_api_duration_ms: get_total_api_duration(),
        total_api_duration_without_retries_ms:
            crate::bootstrap::get_total_api_duration_without_retries(),
        total_tool_duration_ms: get_total_tool_duration(),
        total_lines_added: crate::bootstrap::get_total_lines_added(),
        total_lines_removed: crate::bootstrap::get_total_lines_removed(),
        has_unknown_model_cost: has_unknown_model_cost(),
        model_usage: get_model_usage()
            .into_iter()
            .map(|(model, usage)| {
                (
                    model,
                    CommandCostModelUsage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read_input_tokens: usage.cache_read_input_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        web_search_requests: usage.web_search_requests,
                        cost_usd: usage.cost_usd,
                        context_window: usage.context_window,
                        max_output_tokens: usage.max_output_tokens,
                    },
                )
            })
            .collect(),
    }
}

fn slash_readonly_runtime_inventory_response(
    command: &str,
    args: &[String],
) -> std::result::Result<serde_json::Value, String> {
    if !args.is_empty() {
        return Err(format!("unsupported_slash_command_args: {command}"));
    }

    match command {
        "cost" => Ok(slash_cost_response()),
        "hooks" => Ok(slash_hooks_response()),
        "memory" => Ok(slash_memory_response()),
        "skills" => Ok(slash_skills_response()),
        "plugin" => Ok(slash_plugin_response()),
        "agents" => Ok(slash_agents_response()),
        _ => Err(format!("unsupported_slash_command: {command}")),
    }
}

fn slash_cost_response() -> serde_json::Value {
    let mut model_usage = get_model_usage()
        .into_iter()
        .map(|(model, usage)| {
            serde_json::json!({
                "model": model,
                "inputTokens": usage.input_tokens,
                "outputTokens": usage.output_tokens,
                "cacheReadInputTokens": usage.cache_read_input_tokens,
                "cacheCreationInputTokens": usage.cache_creation_input_tokens,
                "webSearchRequests": usage.web_search_requests,
                "costUsd": usage.cost_usd,
                "contextWindow": usage.context_window,
                "maxOutputTokens": usage.max_output_tokens,
            })
        })
        .collect::<Vec<_>>();
    model_usage.sort_by(|left, right| {
        left.get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("model")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "cost",
        "status": "completed",
        "cost": {
            "totalCostUsd": get_total_cost_usd(),
            "hasUnknownModelCost": has_unknown_model_cost(),
            "modelUsageCount": model_usage.len(),
            "modelUsage": model_usage,
            "durations": {
                "apiMs": get_total_api_duration(),
                "toolMs": get_total_tool_duration(),
            },
        },
    })
}

fn slash_context_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());
    if args.len() > 1 {
        return Err("unsupported_slash_command_args: context".to_string());
    }
    if !matches!(
        action.as_str(),
        "status" | "summary" | "usage" | "tokens" | "breakdown"
    ) {
        return Err(format!("unsupported_slash_command_args: context {action}"));
    }

    let override_model = get_main_loop_model_override();
    let initial_model = get_initial_main_loop_model();
    let env_model = std::env::var("MOSSEN_CODE_MODEL")
        .ok()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty());
    let effective_model = override_model
        .clone()
        .or_else(|| initial_model.clone())
        .or_else(|| env_model.clone());

    let usage_snapshot = get_model_usage();
    let mut total_input_tokens = 0_u64;
    let mut total_output_tokens = 0_u64;
    let mut total_cache_read_input_tokens = 0_u64;
    let mut total_cache_creation_input_tokens = 0_u64;
    let mut total_web_search_requests = 0_u64;
    let mut context_window_tokens = effective_model
        .as_deref()
        .map(model_context_window_for_model)
        .unwrap_or(0);
    let mut max_output_tokens = 0_u64;

    let mut model_usage = usage_snapshot
        .into_iter()
        .map(|(model, usage)| {
            total_input_tokens = total_input_tokens.saturating_add(usage.input_tokens);
            total_output_tokens = total_output_tokens.saturating_add(usage.output_tokens);
            total_cache_read_input_tokens =
                total_cache_read_input_tokens.saturating_add(usage.cache_read_input_tokens);
            total_cache_creation_input_tokens =
                total_cache_creation_input_tokens.saturating_add(usage.cache_creation_input_tokens);
            total_web_search_requests =
                total_web_search_requests.saturating_add(usage.web_search_requests);
            context_window_tokens = context_window_tokens.max(usage.context_window);
            max_output_tokens = max_output_tokens.max(usage.max_output_tokens);
            serde_json::json!({
                "model": model,
                "inputTokens": usage.input_tokens,
                "outputTokens": usage.output_tokens,
                "cacheReadInputTokens": usage.cache_read_input_tokens,
                "cacheCreationInputTokens": usage.cache_creation_input_tokens,
                "contextInputTokens": usage.input_tokens
                    .saturating_add(usage.cache_read_input_tokens)
                    .saturating_add(usage.cache_creation_input_tokens),
                "webSearchRequests": usage.web_search_requests,
                "contextWindowTokens": usage.context_window,
                "maxOutputTokens": usage.max_output_tokens,
                "costUsd": usage.cost_usd,
            })
        })
        .collect::<Vec<_>>();
    model_usage.sort_by(|left, right| {
        left.get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("model")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });

    let context_input_tokens = total_input_tokens
        .saturating_add(total_cache_read_input_tokens)
        .saturating_add(total_cache_creation_input_tokens);
    let usage_available = !model_usage.is_empty();
    let remaining_tokens = if context_window_tokens > 0 {
        Some(context_window_tokens.saturating_sub(context_input_tokens))
    } else {
        None
    };
    let used_percent = context_usage_percent(context_input_tokens, context_window_tokens);
    let remaining_percent = used_percent.map(|value| 100_u64.saturating_sub(value));
    let max_output_for_threshold = max_output_tokens.min(u32::MAX as u64) as u32;
    let effective_window_tokens = if context_window_tokens > 0 {
        Some(effective_context_window(
            context_window_tokens,
            max_output_for_threshold,
        ))
    } else {
        None
    };
    let warning_threshold_tokens = if context_window_tokens > 0 {
        Some(warning_threshold(
            context_window_tokens,
            max_output_for_threshold,
        ))
    } else {
        None
    };
    let error_threshold_tokens = if context_window_tokens > 0 {
        Some(error_threshold(
            context_window_tokens,
            max_output_for_threshold,
        ))
    } else {
        None
    };
    let auto_compact_threshold_tokens = if context_window_tokens > 0 {
        Some(auto_compact_threshold(
            context_window_tokens,
            max_output_for_threshold,
        ))
    } else {
        None
    };
    let status_level = context_status_level(
        context_input_tokens,
        warning_threshold_tokens,
        error_threshold_tokens,
    );

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "context",
        "status": "completed",
        "context": {
            "action": action,
            "usageAvailable": usage_available,
            "analysisDepth": "token_usage_snapshot",
            "messageLevelAnalysisIncluded": false,
            "model": {
                "override": override_model,
                "initial": initial_model,
                "env": env_model,
                "effective": effective_model,
            },
            "totals": {
                "inputTokens": total_input_tokens,
                "outputTokens": total_output_tokens,
                "cacheReadInputTokens": total_cache_read_input_tokens,
                "cacheCreationInputTokens": total_cache_creation_input_tokens,
                "contextInputTokens": context_input_tokens,
                "webSearchRequests": total_web_search_requests,
                "totalCostUsd": get_total_cost_usd(),
            },
            "window": {
                "contextWindowTokens": context_window_tokens,
                "maxOutputTokens": max_output_tokens,
                "effectiveWindowTokens": effective_window_tokens,
                "remainingTokens": remaining_tokens,
                "usedPercent": used_percent,
                "remainingPercent": remaining_percent,
                "status": status_level,
                "contextWindowEstimated": context_window_tokens > 0 && !usage_available,
            },
            "thresholds": {
                "warningTokens": warning_threshold_tokens,
                "errorTokens": error_threshold_tokens,
                "autoCompactTokens": auto_compact_threshold_tokens,
                "autoCompactEligible": auto_compact_threshold_tokens
                    .map(|threshold| context_input_tokens >= threshold)
                    .unwrap_or(false),
            },
            "modelUsageCount": model_usage.len(),
            "modelUsage": model_usage,
            "compact": pending_compact_status(),
            "rawMessagesIncluded": false,
            "messageContentRedacted": true,
            "pathsRedacted": true,
            "mutationSupported": false,
        },
    }))
}

fn context_usage_percent(used_tokens: u64, context_window_tokens: u64) -> Option<u64> {
    if context_window_tokens == 0 {
        return None;
    }
    Some((((used_tokens as f64 / context_window_tokens as f64) * 100.0).round() as u64).min(100))
}

fn context_status_level(
    used_tokens: u64,
    warning_threshold_tokens: Option<u64>,
    error_threshold_tokens: Option<u64>,
) -> &'static str {
    if error_threshold_tokens
        .map(|threshold| used_tokens >= threshold)
        .unwrap_or(false)
    {
        "error"
    } else if warning_threshold_tokens
        .map(|threshold| used_tokens >= threshold)
        .unwrap_or(false)
    {
        "warning"
    } else {
        "normal"
    }
}

fn slash_config_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());
    if args.len() > 1 {
        return Err("unsupported_slash_command_args: config".to_string());
    }
    if !matches!(
        action.as_str(),
        "status" | "summary" | "sources" | "runtime" | "security"
    ) {
        return Err(format!("unsupported_slash_command_args: config {action}"));
    }

    let mut allowed_setting_sources = get_allowed_setting_sources();
    allowed_setting_sources.sort();
    let allowed_setting_source_count = allowed_setting_sources.len();
    let flag_settings_path_present = get_flag_settings_path()
        .map(|path| !path.trim().is_empty())
        .unwrap_or(false);
    let flag_settings_inline = get_flag_settings_inline();
    let flag_settings_inline_type = flag_settings_inline
        .as_ref()
        .map(redacted_json_value_kind)
        .unwrap_or("none");
    let flag_settings_inline_object_key_count = flag_settings_inline
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(serde_json::Map::len)
        .unwrap_or(0);
    let inline_plugin_count = get_inline_plugins().len();
    let chrome_flag_override = get_chrome_flag_override();
    let (raw_permission_mode, permission_mode) = current_session_permission_mode();

    let override_model = get_main_loop_model_override();
    let initial_model = get_initial_main_loop_model();
    let env_model_present = std::env::var("MOSSEN_CODE_MODEL")
        .ok()
        .map(|model| !model.trim().is_empty())
        .unwrap_or(false);
    let model_source = if override_model.is_some() {
        "slash_command"
    } else if initial_model.is_some() {
        "initial"
    } else if env_model_present {
        "env:MOSSEN_CODE_MODEL"
    } else {
        "default"
    };

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "config",
        "status": "completed",
        "config": {
            "action": action,
            "runtime": {
                "protocol": "stream_json",
                "clientType": get_client_type(),
                "sessionSource": get_session_source(),
                "remoteMode": get_is_remote_mode(),
                "sessionBypassPermissionsMode": get_session_bypass_permissions_mode(),
                "permissionMode": {
                    "mode": permission_mode.as_str(),
                    "label": permission_mode_label(permission_mode),
                    "rawModePresent": raw_permission_mode.is_some(),
                    "rawModeRedacted": true,
                },
                "model": {
                    "source": model_source,
                    "overridePresent": override_model.is_some(),
                    "initialPresent": initial_model.is_some(),
                    "envPresent": env_model_present,
                    "rawEnvValueRedacted": true,
                },
            },
            "settings": {
                "allowedSettingSources": allowed_setting_sources,
                "allowedSettingSourceCount": allowed_setting_source_count,
                "flagSettingsPathPresent": flag_settings_path_present,
                "flagSettingsPathRedacted": flag_settings_path_present,
                "flagSettingsInlinePresent": flag_settings_inline.is_some(),
                "flagSettingsInlineType": flag_settings_inline_type,
                "flagSettingsInlineObjectKeyCount": flag_settings_inline_object_key_count,
                "flagSettingsInlineValuesRedacted": true,
                "settingsFileContentIncluded": false,
                "rawConfigIncluded": false,
                "rawConfigRedacted": true,
                "pathsRedacted": true,
            },
            "plugins": {
                "inlinePluginCount": inline_plugin_count,
                "inlinePluginNamesIncluded": false,
                "coworkPluginsEnabled": get_use_cowork_plugins(),
                "chromeFlagOverridePresent": chrome_flag_override.is_some(),
                "chromeFlagOverride": chrome_flag_override,
            },
            "security": {
                "mutationSupported": false,
                "secretsRedacted": true,
                "pathsRedacted": true,
                "rawConfigRedacted": true,
                "envValuesRedacted": true,
            },
            "mutationSupported": false,
        },
    }))
}

fn redacted_json_value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn json_bool(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn slash_hooks_response() -> serde_json::Value {
    let hooks = get_registered_hooks().unwrap_or_default();
    let mut events = hooks
        .into_iter()
        .map(|(event, matchers)| {
            let hook_count = matchers
                .iter()
                .map(slash_hook_command_count_from_matcher)
                .sum::<usize>();
            serde_json::json!({
                "event": event,
                "matcherCount": matchers.len(),
                "hookCount": hook_count,
            })
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.get("event")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("event")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });
    let total_matchers = events
        .iter()
        .map(|event| {
            event
                .get("matcherCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        })
        .sum::<u64>();
    let total_hooks = events
        .iter()
        .map(|event| {
            event
                .get("hookCount")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        })
        .sum::<u64>();

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "hooks",
        "status": "completed",
        "hooks": {
            "eventCount": events.len(),
            "matcherCount": total_matchers,
            "hookCount": total_hooks,
            "events": events,
            "rawCommandsRedacted": true,
            "turnHookCount": get_turn_hook_count(),
            "turnHookDurationMs": get_turn_hook_duration_ms(),
        },
    })
}

fn slash_hook_command_count_from_matcher(matcher: &serde_json::Value) -> usize {
    matcher
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn slash_memory_response() -> serde_json::Value {
    let plan_slug_cache = get_plan_slug_cache();
    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "memory",
        "status": "completed",
        "memory": {
            "contentRedacted": true,
            "memoryFilesAttached": false,
            "planSlugCount": plan_slug_cache.len(),
            "sessionPlanSlugsAvailable": !plan_slug_cache.is_empty(),
            "runtime": slash_memory_runtime_snapshot(),
            "rawPathsRedacted": true,
        },
    })
}

fn slash_memory_runtime_snapshot() -> serde_json::Value {
    let session_memory_config =
        mossen_agent::services::session_memory::utils::get_session_memory_config();
    let session_memory_compact_enabled =
        mossen_agent::services::compact::session_memory_compact::should_use_session_memory_compaction(
        );

    serde_json::json!({
        "autoMemoryEnabled": crate::memdir::is_auto_memory_enabled(),
        "extractModeActive": crate::memdir::is_extract_mode_active(),
        "sessionMemory": {
            "enabled": session_memory_config.auto_extract_enabled,
            "compactEnabled": session_memory_compact_enabled,
            "initialized": mossen_agent::services::session_memory::utils::is_session_memory_initialized(),
            "contentIncluded": false,
        },
        "compact": {
            "autoCompactEnabled": crate::query_engine::is_auto_compact_enabled(),
            "sessionMemoryCompactEnabled": session_memory_compact_enabled,
            "pending": get_pending_compact_request().is_some(),
        },
        "files": {
            "contentIncluded": false,
            "pathsIncluded": false,
            "pathsRedacted": true,
        },
        "secretsRedacted": true,
    })
}

async fn slash_mcp_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    if !args.is_empty() {
        return Err("unsupported_slash_command_args: mcp".to_string());
    }

    let statuses = mcp_runtime_status::snapshot().await;
    let manager_installed = statuses.is_some();
    let mut connected_count = 0usize;
    let mut pending_count = 0usize;
    let mut failed_count = 0usize;
    let mut needs_auth_count = 0usize;
    let mut disabled_count = 0usize;
    let mut tool_count = 0usize;
    let mut prompt_count = 0usize;
    let mut resource_count = 0usize;
    let servers = statuses
        .unwrap_or_default()
        .into_iter()
        .map(|status| {
            match status.state {
                mcp_runtime_status::RuntimeMcpConnectionState::Connected => {
                    connected_count = connected_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Pending => {
                    pending_count = pending_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Failed => {
                    failed_count = failed_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::NeedsAuth => {
                    needs_auth_count = needs_auth_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Disabled => {
                    disabled_count = disabled_count.saturating_add(1);
                }
            }
            tool_count = tool_count.saturating_add(status.tools_count);
            prompt_count = prompt_count.saturating_add(status.prompts_count);
            resource_count = resource_count.saturating_add(status.resources_count);
            serde_json::json!({
                "name": status.name,
                "state": mcp_connection_state_label(status.state),
                "transport": status.transport,
                "scope": status.scope,
                "toolsCount": status.tools_count,
                "promptsCount": status.prompts_count,
                "resourcesCount": status.resources_count,
                "lastErrorPresent": status.last_error.is_some(),
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "mcp",
        "status": "completed",
        "mcp": {
            "managerInstalled": manager_installed,
            "serverCount": servers.len(),
            "connectedCount": connected_count,
            "pendingCount": pending_count,
            "failedCount": failed_count,
            "needsAuthCount": needs_auth_count,
            "disabledCount": disabled_count,
            "toolCount": tool_count,
            "promptCount": prompt_count,
            "resourceCount": resource_count,
            "servers": servers,
            "rawConfigRedacted": true,
            "toolSchemasRedacted": true,
            "instructionsRedacted": true,
            "errorDetailsRedacted": true,
            "mutationSupported": false,
        },
    }))
}

async fn slash_ide_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());
    if args.len() > 1 {
        return Err("unsupported_slash_command_args: ide".to_string());
    }
    if !matches!(
        action.as_str(),
        "status" | "summary" | "mcp" | "diagnostics"
    ) {
        return Err(format!("unsupported_slash_command_args: ide {action}"));
    }

    let statuses = mcp_runtime_status::snapshot().await;
    let manager_installed = statuses.is_some();
    let mut connected_count = 0usize;
    let mut pending_count = 0usize;
    let mut failed_count = 0usize;
    let mut needs_auth_count = 0usize;
    let mut disabled_count = 0usize;
    let mut tool_count = 0usize;
    let mut prompt_count = 0usize;
    let mut resource_count = 0usize;
    let servers = statuses
        .unwrap_or_default()
        .into_iter()
        .filter(|status| matches!(status.transport.as_str(), "sse-ide" | "ws-ide"))
        .map(|status| {
            match status.state {
                mcp_runtime_status::RuntimeMcpConnectionState::Connected => {
                    connected_count = connected_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Pending => {
                    pending_count = pending_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Failed => {
                    failed_count = failed_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::NeedsAuth => {
                    needs_auth_count = needs_auth_count.saturating_add(1);
                }
                mcp_runtime_status::RuntimeMcpConnectionState::Disabled => {
                    disabled_count = disabled_count.saturating_add(1);
                }
            }
            tool_count = tool_count.saturating_add(status.tools_count);
            prompt_count = prompt_count.saturating_add(status.prompts_count);
            resource_count = resource_count.saturating_add(status.resources_count);
            serde_json::json!({
                "name": status.name,
                "state": mcp_connection_state_label(status.state),
                "transport": status.transport,
                "scope": status.scope,
                "toolsCount": status.tools_count,
                "promptsCount": status.prompts_count,
                "resourcesCount": status.resources_count,
                "lastErrorPresent": status.last_error.is_some(),
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "ide",
        "status": "completed",
        "ide": {
            "action": action,
            "connected": connected_count > 0,
            "managerInstalled": manager_installed,
            "ideServerCount": servers.len(),
            "connectedCount": connected_count,
            "pendingCount": pending_count,
            "failedCount": failed_count,
            "needsAuthCount": needs_auth_count,
            "disabledCount": disabled_count,
            "toolCount": tool_count,
            "promptCount": prompt_count,
            "resourceCount": resource_count,
            "servers": servers,
            "diagnostics": {
                "pendingLspDiagnosticCount": get_pending_lsp_diagnostic_count(),
                "contentIncluded": false,
                "pathsRedacted": true,
            },
            "detection": {
                "externalScanRun": false,
                "processScanRun": false,
                "openCommandRun": false,
                "mcpRuntimeSnapshot": true,
            },
            "supportedTransports": ["sse-ide", "ws-ide"],
            "connectMutationSupported": false,
            "openMutationSupported": false,
            "rawConfigRedacted": true,
            "errorDetailsRedacted": true,
            "pathsRedacted": true,
            "mutationSupported": false,
        },
    }))
}

fn mcp_connection_state_label(
    state: mcp_runtime_status::RuntimeMcpConnectionState,
) -> &'static str {
    match state {
        mcp_runtime_status::RuntimeMcpConnectionState::Connected => "connected",
        mcp_runtime_status::RuntimeMcpConnectionState::Pending => "pending",
        mcp_runtime_status::RuntimeMcpConnectionState::Failed => "failed",
        mcp_runtime_status::RuntimeMcpConnectionState::NeedsAuth => "needs_auth",
        mcp_runtime_status::RuntimeMcpConnectionState::Disabled => "disabled",
    }
}

async fn slash_diff_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "summary".to_string());
    if args.len() > 1 {
        return Err("unsupported_slash_command_args: diff".to_string());
    }
    if !matches!(action.as_str(), "summary" | "status" | "show" | "files") {
        return Err(format!("unsupported_slash_command_args: diff {action}"));
    }

    let cwd =
        std::env::current_dir().map_err(|_| "slash_command_diff_cwd_unavailable".to_string())?;
    let Some(diff) = fetch_git_diff(&cwd).await else {
        return Ok(serde_json::json!({
            "subtype": "slash_command_result",
            "command": "diff",
            "status": "completed",
            "diff": {
                "action": action,
                "available": false,
                "gitRepo": false,
                "reason": "not_git_repo_or_transient_state",
                "comparison": "HEAD",
                "filesChanged": 0,
                "linesAdded": 0,
                "linesRemoved": 0,
                "files": [],
                "filesTruncated": false,
                "rawDiffIncluded": false,
                "rawDiffRedacted": true,
                "hunksIncluded": false,
                "contentIncluded": false,
                "cwdRedacted": true,
            },
        }));
    };

    let mut files = diff
        .per_file_stats
        .into_iter()
        .map(|(path, stats)| {
            serde_json::json!({
                "path": path,
                "added": stats.added,
                "removed": stats.removed,
                "binary": stats.is_binary,
                "untracked": stats.is_untracked,
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| {
        left.get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });
    let reported_files = files.len();

    Ok(serde_json::json!({
        "subtype": "slash_command_result",
        "command": "diff",
        "status": "completed",
        "diff": {
            "action": action,
            "available": true,
            "gitRepo": true,
            "comparison": "HEAD",
            "filesChanged": diff.stats.files_count,
            "linesAdded": diff.stats.lines_added,
            "linesRemoved": diff.stats.lines_removed,
            "reportedFiles": reported_files,
            "filesTruncated": diff.stats.files_count > reported_files,
            "files": files,
            "rawDiffIncluded": false,
            "rawDiffRedacted": true,
            "hunksIncluded": false,
            "contentIncluded": false,
            "cwdRedacted": true,
            "mutationSupported": false,
        },
    }))
}

fn slash_skills_response() -> serde_json::Value {
    let mut available = mossen_skills::get_bundled_crafts();
    available.extend(mossen_skills::get_dynamic_skills());
    let mut available = available
        .into_iter()
        .filter(|skill| skill.is_user_invocable())
        .map(|skill| {
            serde_json::json!({
                "name": skill.name(),
                "description": skill.base.description,
                "source": slash_skill_source_label(skill.loaded_from),
                "modelInvocationEnabled": !skill.base.disable_model_invocation.unwrap_or(false),
                "contentRedacted": true,
                "pathRedacted": true,
            })
        })
        .collect::<Vec<_>>();
    available.sort_by(|left, right| {
        left.get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });
    available.dedup_by(|left, right| left.get("name") == right.get("name"));

    let mut invoked = get_invoked_skills()
        .into_values()
        .map(|skill| {
            serde_json::json!({
                "name": skill.skill_name,
                "invokedAt": skill.invoked_at,
                "agentId": skill.agent_id,
                "contentRedacted": true,
                "pathRedacted": true,
            })
        })
        .collect::<Vec<_>>();
    invoked.sort_by(|left, right| {
        left.get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .cmp(
                right
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            )
    });

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "skills",
        "status": "completed",
        "skills": {
            "availableCount": available.len(),
            "available": available,
            "invokedCount": invoked.len(),
            "invoked": invoked,
            "contentRedacted": true,
            "pathsRedacted": true,
            "rawSkillRootsIncluded": false,
        },
    })
}

fn slash_skill_source_label(loaded_from: mossen_types::command::CommandLoadedFrom) -> &'static str {
    match loaded_from {
        mossen_types::command::CommandLoadedFrom::Bundled => "bundled",
        mossen_types::command::CommandLoadedFrom::Plugin => "plugin",
        mossen_types::command::CommandLoadedFrom::Mcp => "mcp",
        mossen_types::command::CommandLoadedFrom::Skills => "skills",
        mossen_types::command::CommandLoadedFrom::Managed => "managed",
        mossen_types::command::CommandLoadedFrom::CommandsDeprecated => "commands_deprecated",
    }
}

fn slash_plugin_response() -> serde_json::Value {
    let inline_plugins = get_inline_plugins();
    let plugin_hook_count = get_registered_hooks()
        .unwrap_or_default()
        .values()
        .flat_map(|matchers| matchers.iter())
        .map(slash_hook_command_count_from_matcher)
        .sum::<usize>();

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "plugin",
        "status": "completed",
        "plugins": {
            "inlinePluginCount": inline_plugins.len(),
            "inlinePluginNamesIncluded": false,
            "coworkPluginsEnabled": get_use_cowork_plugins(),
            "registeredHookCount": plugin_hook_count,
            "installMutationSupported": false,
            "rawConfigRedacted": true,
        },
    })
}

fn slash_agents_response() -> serde_json::Value {
    let runtime = snapshot_agent_runtime_status();
    let mut agent_ids = get_invoked_skills()
        .into_values()
        .filter_map(|skill| skill.agent_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    agent_ids.sort();

    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "agents",
        "status": "completed",
        "agents": {
            "activeDialogues": runtime.active_dialogues,
            "activeSessionId": runtime.active_session_id,
            "activeModel": runtime.active_model,
            "knownAgentCount": agent_ids.len(),
            "agentIds": agent_ids,
            "promptsRedacted": true,
            "pathsRedacted": true,
        },
    })
}

fn pending_compact_status() -> serde_json::Value {
    match get_pending_compact_request() {
        Some(request) => {
            let age_ms = request.enqueued_at.elapsed().as_millis() as u64;
            serde_json::json!({
                "pending": true,
                "request_id": request.request_id,
                "mode": match request.mode {
                    CompactMode::Manual => "manual",
                },
                "dry_run": request.dry_run,
                "custom_instructions_present": request.custom_instructions.is_some(),
                "age_ms": age_ms,
                "timeout_ms": COMPACT_REQUEST_TIMEOUT.as_millis() as u64,
                "timed_out": request.enqueued_at.elapsed() > COMPACT_REQUEST_TIMEOUT,
            })
        }
        None => serde_json::json!({
            "pending": false,
        }),
    }
}

fn pending_clear_status() -> serde_json::Value {
    match get_pending_clear_request() {
        Some(request) => {
            let age_ms = request.enqueued_at.elapsed().as_millis() as u64;
            serde_json::json!({
                "pending": true,
                "request_id": request.request_id,
                "dry_run": request.dry_run,
                "age_ms": age_ms,
                "timeout_ms": CLEAR_REQUEST_TIMEOUT.as_millis() as u64,
                "timed_out": request.enqueued_at.elapsed() > CLEAR_REQUEST_TIMEOUT,
            })
        }
        None => serde_json::json!({
            "pending": false,
            "timeout_ms": CLEAR_REQUEST_TIMEOUT.as_millis() as u64,
        }),
    }
}

fn slash_permissions_response(args: &[String]) -> Option<serde_json::Value> {
    if args.is_empty()
        || (args.len() == 1
            && matches!(
                args[0].as_str(),
                "mode" | "modes" | "status" | "summary" | "picker"
            ))
    {
        return Some(slash_permissions_summary_response(None, "status", None));
    }

    if args.len() == 1 && matches!(args[0].as_str(), "list" | "show" | "rules") {
        return Some(slash_permissions_summary_response(None, "list", None));
    }

    if let Some(response) = slash_permissions_rule_response(args) {
        return Some(response);
    }

    let requested_mode = if matches!(
        args.first().map(String::as_str),
        Some("mode" | "set" | "choose" | "select")
    ) {
        args.iter()
            .skip(1)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let requested_mode = requested_mode.trim();
    if requested_mode.is_empty() {
        return Some(slash_permissions_summary_response(None, "status", None));
    }

    let mode = parse_permission_mode_arg(requested_mode)?;
    let (_, previous_mode) = current_session_permission_mode();
    std::env::set_var(PERMISSION_MODE_ENV, mode.as_str());
    Some(slash_permissions_summary_response(
        Some(previous_mode),
        "set",
        Some(requested_mode),
    ))
}

fn slash_permissions_summary_response(
    previous_mode: Option<PermissionMode>,
    action: &str,
    requested_mode: Option<&str>,
) -> serde_json::Value {
    let (raw_mode, mode) = current_session_permission_mode();
    let source = if previous_mode.is_some() {
        "slash_command"
    } else if raw_mode.is_some() {
        "env:MOSSEN_PERMISSION_MODE"
    } else {
        "default"
    };
    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "permissions",
        "status": "completed",
        "permissions": {
            "mode": mode.as_str(),
            "mode_label": permission_mode_label(mode),
            "previous_mode": previous_mode.map(|mode| mode.as_str()),
            "action": action,
            "requested_mode": requested_mode,
            "source": source,
            "available_modes": permission_mode_values(),
            "mode_options": permission_mode_options(mode),
            "codex_approval_modes": ["suggest", "auto-edit", "full-auto"],
            "codex_mode": permission_mode_codex_mode(mode),
            "mode_picker": permission_mode_picker_payload(mode),
            "terminal_control": {
                "status_line_label": permission_mode_label(mode),
                "picker_options": permission_mode_options(mode),
                "selected_index": permission_mode_option_index(mode),
                "selected_value": permission_mode_option_value(mode),
                "aliases_accepted": true,
                "rule_patterns_redacted": true,
            },
            "selected_option": permission_mode_option_value(mode),
            "rule_counts": permission_rule_counts(),
            "rules": permission_rules_redacted_payload(),
            "rule_patterns_redacted": true,
            "mutation_supported": true,
            "mode_mutation_supported": true,
            "rule_mutation_supported": true,
        },
    })
}

fn slash_permissions_rule_response(args: &[String]) -> Option<serde_json::Value> {
    let action = args.first().map(String::as_str)?;
    match action {
        "allow" | "deny" => {
            let rule = args
                .iter()
                .skip(1)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            let rule = terminal_permission_rule_text(&rule);
            if rule.is_empty() {
                return None;
            }
            apply_session_permission_rule(action, &rule);
            Some(slash_permissions_rule_summary_response(action, Some(&rule)))
        }
        "reset" | "clear" => {
            std::env::remove_var(PERMISSION_ALLOW_RULES_ENV);
            std::env::remove_var(PERMISSION_DENY_RULES_ENV);
            Some(slash_permissions_rule_summary_response("reset", None))
        }
        _ => None,
    }
}

fn slash_permissions_rule_summary_response(action: &str, rule: Option<&str>) -> serde_json::Value {
    let mut response = slash_permissions_summary_response(None, action, None);
    if let Some(permissions) = response
        .get_mut("permissions")
        .and_then(serde_json::Value::as_object_mut)
    {
        permissions.insert(
            "rule_update".to_string(),
            serde_json::json!({
                "action": action,
                "behavior": match action {
                    "allow" => "allow",
                    "deny" => "deny",
                    _ => "none",
                },
                "applied": matches!(action, "allow" | "deny" | "reset" | "clear"),
                "rule_pattern_present": rule
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false),
                "rule_pattern_redacted": rule.is_some(),
                "raw_rule_included": false,
            }),
        );
    }
    response
}

fn apply_session_permission_rule(behavior: &str, rule: &str) {
    let (target, opposite) = match behavior {
        "allow" => (PERMISSION_ALLOW_RULES_ENV, PERMISSION_DENY_RULES_ENV),
        "deny" => (PERMISSION_DENY_RULES_ENV, PERMISSION_ALLOW_RULES_ENV),
        _ => return,
    };
    let rule = terminal_permission_rule_text(rule);
    if rule.is_empty() {
        return;
    }
    let mut target_rules = permission_rule_env_lines(target);
    let mut opposite_rules = permission_rule_env_lines(opposite);
    remove_permission_rule_line(&mut opposite_rules, &rule);
    add_permission_rule_line(&mut target_rules, rule);
    sync_permission_rule_env(target, &target_rules);
    sync_permission_rule_env(opposite, &opposite_rules);
}

fn permission_rule_counts() -> serde_json::Value {
    serde_json::json!({
        "allow": permission_rule_env_lines(PERMISSION_ALLOW_RULES_ENV).len(),
        "deny": permission_rule_env_lines(PERMISSION_DENY_RULES_ENV).len(),
        "ask": 0,
    })
}

fn permission_rules_redacted_payload() -> serde_json::Value {
    let allow_count = permission_rule_env_lines(PERMISSION_ALLOW_RULES_ENV).len();
    let deny_count = permission_rule_env_lines(PERMISSION_DENY_RULES_ENV).len();
    serde_json::json!({
        "allow_count": allow_count,
        "deny_count": deny_count,
        "ask_count": 0,
        "patterns_redacted": true,
        "raw_patterns_included": false,
    })
}

fn permission_rule_env_lines(key: &str) -> Vec<String> {
    std::env::var(key)
        .ok()
        .map(|value| {
            value
                .lines()
                .map(terminal_permission_rule_text)
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn add_permission_rule_line(rules: &mut Vec<String>, rule: String) {
    if !rules
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&rule))
    {
        rules.push(rule);
    }
}

fn remove_permission_rule_line(rules: &mut Vec<String>, rule: &str) {
    rules.retain(|existing| !existing.eq_ignore_ascii_case(rule));
}

fn sync_permission_rule_env(key: &str, rules: &[String]) {
    if rules.is_empty() {
        std::env::remove_var(key);
    } else {
        std::env::set_var(key, rules.join("\n"));
    }
}

fn slash_plan_response(args: &[String]) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "status".to_string());
    if args.len() > 1 {
        return Err(
            "unsupported_slash_command_args: plan (use status, enter, or exit)".to_string(),
        );
    }

    match action.as_str() {
        "status" | "show" | "mode" => Ok(slash_plan_summary_response("status", None)),
        "enter" | "on" | "start" | "enable" => {
            let (_, previous_mode) = current_session_permission_mode();
            std::env::set_var(PERMISSION_MODE_ENV, PermissionMode::Plan.as_str());
            Ok(slash_plan_summary_response("enter", Some(previous_mode)))
        }
        "exit" | "off" | "stop" | "disable" => {
            let (_, previous_mode) = current_session_permission_mode();
            std::env::set_var(PERMISSION_MODE_ENV, PermissionMode::Default.as_str());
            Ok(slash_plan_summary_response("exit", Some(previous_mode)))
        }
        _ => Err(format!("unsupported_slash_command_args: plan {action}")),
    }
}

fn slash_plan_summary_response(
    action: &str,
    previous_mode: Option<PermissionMode>,
) -> serde_json::Value {
    let (raw_mode, mode) = current_session_permission_mode();
    let source = if previous_mode.is_some() {
        "slash_command"
    } else if raw_mode.is_some() {
        "env:MOSSEN_PERMISSION_MODE"
    } else {
        "default"
    };
    let active = matches!(mode, PermissionMode::Plan);
    serde_json::json!({
        "subtype": "slash_command_result",
        "command": "plan",
        "status": "completed",
        "plan": {
            "action": action,
            "active": active,
            "mode": mode.as_str(),
            "mode_label": permission_mode_label(mode),
            "previous_mode": previous_mode.map(|mode| mode.as_str()),
            "source": source,
            "permission_mode": {
                "value": mode.as_str(),
                "label": permission_mode_label(mode),
                "read_only_exploration": active,
                "mutating_tools_blocked": active,
            },
            "supported_actions": ["status", "enter", "exit"],
            "enter_supported": true,
            "exit_supported": true,
            "mode_mutation_supported": true,
            "plan_file_attached": false,
            "plan_file_path": serde_json::Value::Null,
            "next": if active {
                "read-only planning is active; use /plan exit or /permissions default to leave plan mode"
            } else {
                "use /plan enter to switch to read-only planning mode"
            },
        },
    })
}

fn current_session_permission_mode() -> (Option<String>, PermissionMode) {
    let raw_mode = std::env::var(PERMISSION_MODE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mode = raw_mode
        .as_deref()
        .and_then(parse_permission_mode_arg)
        .unwrap_or(PermissionMode::Default);
    (raw_mode, mode)
}

fn parse_permission_mode_arg(raw: &str) -> Option<PermissionMode> {
    match permission_mode_match_key(raw).as_str() {
        "default" | "supervised" | "suggest" | "ask" => Some(PermissionMode::Default),
        "plan" | "readonly" | "read" => Some(PermissionMode::Plan),
        "acceptedits" | "acceptedit" | "autoedit" | "autoedits" | "autoaccept" => {
            Some(PermissionMode::AcceptEdits)
        }
        "bypasspermissions" | "bypass" | "fullauto" => Some(PermissionMode::BypassPermissions),
        "dontask" | "dontprompt" | "neverask" | "deny" => Some(PermissionMode::DontAsk),
        "auto" => Some(PermissionMode::Auto),
        "yolo" => Some(PermissionMode::Yolo),
        _ => None,
    }
}

fn permission_mode_match_key(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn permission_mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "Supervised",
        PermissionMode::Plan => "Plan",
        PermissionMode::AcceptEdits => "Accept Edits",
        PermissionMode::BypassPermissions => "Full Auto",
        PermissionMode::DontAsk => "Don't Ask",
        PermissionMode::Auto => "Auto",
        PermissionMode::Yolo => "Yolo",
    }
}

fn permission_mode_options(selected_mode: PermissionMode) -> Vec<serde_json::Value> {
    [
        (
            PermissionMode::Default,
            "suggest",
            "Suggest",
            "Supervised",
            "Ask before edits and commands that need approval.",
            "ask",
            "ask",
            "low",
            &["default", "supervised", "suggest", "ask"][..],
        ),
        (
            PermissionMode::Plan,
            "plan",
            "Plan",
            "Plan",
            "Read-only exploration; mutating tools are blocked.",
            "deny",
            "deny",
            "low",
            &["plan", "read-only", "readonly"][..],
        ),
        (
            PermissionMode::AcceptEdits,
            "auto-edit",
            "Auto Edit",
            "Accept Edits",
            "Auto-accept file edits while still asking for shell commands.",
            "auto",
            "ask",
            "medium",
            &["acceptEdits", "accept-edits", "auto-edit", "autoedit"][..],
        ),
        (
            PermissionMode::BypassPermissions,
            "full-auto",
            "Full Auto",
            "Full Auto",
            "Allow edits and shell commands without prompting.",
            "auto",
            "auto",
            "high",
            &["bypassPermissions", "bypass", "full-auto", "fullauto"][..],
        ),
        (
            PermissionMode::DontAsk,
            "dont-ask",
            "Don't Ask",
            "Don't Ask",
            "Deny approval-gated tools instead of prompting.",
            "deny",
            "deny",
            "restricted",
            &["dontAsk", "dont-ask", "never-ask", "deny"][..],
        ),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(index, (mode, codex_value, codex_label, label, summary, edits, shell, risk, aliases))| {
            serde_json::json!({
                "value": mode.as_str(),
                "codex_value": codex_value,
                "label": label,
                "codex_label": codex_label,
                "summary": summary,
                "edit_approval": edits,
                "shell_approval": shell,
                "risk": risk,
                "index": index,
                "selected": mode == selected_mode,
                "aliases": aliases,
            })
        },
    )
    .collect()
}

fn permission_mode_option_index(mode: PermissionMode) -> usize {
    match mode {
        PermissionMode::Default => 0,
        PermissionMode::Plan => 1,
        PermissionMode::AcceptEdits => 2,
        PermissionMode::BypassPermissions => 3,
        PermissionMode::DontAsk => 4,
        PermissionMode::Auto | PermissionMode::Yolo => 0,
    }
}

fn permission_mode_picker_payload(mode: PermissionMode) -> serde_json::Value {
    serde_json::json!({
        "kind": "permission_mode_picker",
        "layout": "segmented_control",
        "title": "Permission mode",
        "options": permission_mode_options(mode),
        "selected_index": permission_mode_option_index(mode),
        "selected_value": permission_mode_option_value(mode),
        "accepted_aliases": ["suggest", "read-only", "readonly", "auto-edit", "full-auto", "dont-ask"],
        "codex_order": ["suggest", "plan", "auto-edit", "full-auto", "dont-ask"],
        "risk_order": ["low", "medium", "high", "restricted"],
        "keyboard": {
            "left_right": true,
            "up_down": true,
            "enter_selects": true,
            "esc_cancels": true,
        },
        "mutation_supported": true,
        "applies_to_session": true,
        "status_line_label": permission_mode_label(mode),
    })
}

fn permission_mode_codex_mode(mode: PermissionMode) -> serde_json::Value {
    let (codex_value, codex_label, edits, shell, risk, legacy) = match mode {
        PermissionMode::Default => ("suggest", "Suggest", "ask", "ask", "low", false),
        PermissionMode::Plan => ("plan", "Plan", "deny", "deny", "low", false),
        PermissionMode::AcceptEdits => ("auto-edit", "Auto Edit", "auto", "ask", "medium", false),
        PermissionMode::BypassPermissions => {
            ("full-auto", "Full Auto", "auto", "auto", "high", false)
        }
        PermissionMode::DontAsk => ("dont-ask", "Don't Ask", "deny", "deny", "restricted", false),
        PermissionMode::Auto => ("auto", "Legacy Auto", "auto", "auto", "high", true),
        PermissionMode::Yolo => ("yolo", "Legacy Yolo", "auto", "auto", "high", true),
    };
    serde_json::json!({
        "value": codex_value,
        "label": codex_label,
        "internal_mode": mode.as_str(),
        "edit_approval": edits,
        "shell_approval": shell,
        "risk": risk,
        "selected": true,
        "legacy_internal_mode": legacy,
        "read_only_exploration": matches!(mode, PermissionMode::Plan),
        "mutating_tools_blocked": matches!(mode, PermissionMode::Plan),
        "approval_prompt_suppressed": matches!(mode, PermissionMode::DontAsk),
    })
}

fn permission_mode_option_value(mode: PermissionMode) -> serde_json::Value {
    permission_mode_options(mode)
        .into_iter()
        .find(|option| {
            option.get("value").and_then(serde_json::Value::as_str) == Some(mode.as_str())
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "value": mode.as_str(),
                "label": permission_mode_label(mode),
                "codex_label": permission_mode_label(mode),
                "summary": "Legacy permission mode.",
                "selected": true,
                "aliases": [mode.as_str()],
            })
        })
}

fn build_compact_slash_response(
    request_id: String,
    args: &[String],
) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.as_str())
        .unwrap_or("preview");

    match action {
        "status" => {
            let pending = get_pending_compact_request();
            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "compact",
                "status": "completed",
                "compact": {
                    "action": "status",
                    "pending": pending.is_some(),
                    "request_id": pending.as_ref().map(|request| request.request_id.clone()),
                    "dry_run": pending.as_ref().map(|request| request.dry_run),
                    "mode": pending.as_ref().map(|_| "manual"),
                    "has_custom_instructions": pending
                        .as_ref()
                        .and_then(|request| request.custom_instructions.as_ref())
                        .is_some(),
                    "timeout_seconds": COMPACT_REQUEST_TIMEOUT.as_secs(),
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                    "requires_confirm": false,
                    "run_requires_confirm": true,
                    "available_actions": compact_action_options(),
                },
            }))
        }
        "cancel" | "stop" => {
            let pending = get_pending_compact_request();
            clear_pending_compact_request();
            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "compact",
                "status": "completed",
                "compact": {
                    "action": "cancel",
                    "cancelled": pending.is_some(),
                    "pending": false,
                    "request_id": pending.as_ref().map(|request| request.request_id.clone()),
                    "dry_run": pending.as_ref().map(|request| request.dry_run),
                    "mode": pending.as_ref().map(|_| "manual"),
                    "had_custom_instructions": pending
                        .as_ref()
                        .and_then(|request| request.custom_instructions.as_ref())
                        .is_some(),
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                    "requires_confirm": false,
                    "run_requires_confirm": true,
                    "available_actions": compact_action_options(),
                },
            }))
        }
        "plan" | "preview" | "dry-run" | "dryrun" => {
            let requested_action = action;
            let action = "preview";
            let dry_run = true;
            let custom_instructions = compact_custom_instructions(args, 1);
            enqueue_pending_compact_request(
                request_id.clone(),
                CompactMode::Manual,
                dry_run,
                custom_instructions.clone(),
            )
            .map_err(|err| format!("compact_request_blocked: {err}"))?;

            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "compact",
                "status": "queued",
                "compact": {
                    "action": action,
                    "requested_action": requested_action,
                    "mode": "manual",
                    "dry_run": dry_run,
                    "custom_instructions": custom_instructions,
                    "request_id": request_id,
                    "pending": true,
                    "requires_confirm": false,
                    "run_requires_confirm": true,
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                    "confirmation": {
                        "required_for_run": true,
                        "provided": false,
                        "confirm_command": "/compact run --confirm",
                    },
                    "compact_preview": compact_preview_payload(
                        action,
                        &request_id,
                        dry_run,
                        custom_instructions.is_some(),
                    ),
                    "available_actions": compact_action_options(),
                    "next": "use /compact run --confirm to apply a real compaction",
                },
            }))
        }
        "run" => {
            if !args.iter().skip(1).any(|arg| arg == "--confirm") {
                return Ok(serde_json::json!({
                    "subtype": "slash_command_result",
                    "command": "compact",
                    "status": "completed",
                    "compact": {
                        "action": "run",
                        "pending": get_pending_compact_request().is_some(),
                        "dry_run": false,
                        "requires_confirm": true,
                        "run_requires_confirm": true,
                        "execution_stage": "dialogue_safe_point",
                        "mutation_supported": true,
                        "confirmation": {
                            "required_for_run": true,
                            "provided": false,
                            "confirm_command": "/compact run --confirm",
                        },
                        "available_actions": compact_action_options(),
                        "next": "use /compact run --confirm to queue conversation compaction",
                    },
                }));
            }
            let custom_instructions = compact_custom_instructions(args, 1);
            enqueue_pending_compact_request(
                request_id.clone(),
                CompactMode::Manual,
                false,
                custom_instructions.clone(),
            )
            .map_err(|err| format!("compact_request_blocked: {err}"))?;

            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "compact",
                "status": "queued",
                "compact": {
                    "action": "run",
                    "mode": "manual",
                    "dry_run": false,
                    "custom_instructions": custom_instructions,
                    "request_id": request_id,
                    "pending": true,
                    "requires_confirm": false,
                    "run_requires_confirm": true,
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                    "confirmation": {
                        "required_for_run": true,
                        "provided": true,
                        "confirm_command": "/compact run --confirm",
                    },
                    "compact_preview": compact_preview_payload(
                        action,
                        &request_id,
                        false,
                        custom_instructions.is_some(),
                    ),
                    "available_actions": compact_action_options(),
                },
            }))
        }
        "--confirm" | "confirm" => {
            let custom_instructions = compact_custom_instructions(args, 1);
            enqueue_pending_compact_request(
                request_id.clone(),
                CompactMode::Manual,
                false,
                custom_instructions.clone(),
            )
            .map_err(|err| format!("compact_request_blocked: {err}"))?;

            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "compact",
                "status": "queued",
                "compact": {
                    "action": "run",
                    "mode": "manual",
                    "dry_run": false,
                    "custom_instructions": custom_instructions,
                    "request_id": request_id,
                    "pending": true,
                    "requires_confirm": false,
                    "run_requires_confirm": true,
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                    "confirmation": {
                        "required_for_run": true,
                        "provided": true,
                        "confirm_command": "/compact run --confirm",
                    },
                    "compact_preview": compact_preview_payload(
                        "run",
                        &request_id,
                        false,
                        custom_instructions.is_some(),
                    ),
                    "available_actions": compact_action_options(),
                },
            }))
        }
        _ => Err(format!(
            "unsupported_slash_command_args: compact {}",
            args.join(" ")
        )),
    }
}

fn compact_action_options() -> serde_json::Value {
    serde_json::json!([
        {
            "id": "preview",
            "label": "Preview",
            "command": "/compact preview",
            "dry_run": true,
            "requires_confirm": false,
            "mutates_history": false,
        },
        {
            "id": "status",
            "label": "Status",
            "command": "/compact status",
            "dry_run": true,
            "requires_confirm": false,
            "mutates_history": false,
        },
        {
            "id": "run",
            "label": "Run",
            "command": "/compact run --confirm",
            "dry_run": false,
            "requires_confirm": true,
            "mutates_history": true,
        },
        {
            "id": "cancel",
            "label": "Cancel",
            "command": "/compact cancel",
            "dry_run": true,
            "requires_confirm": false,
            "mutates_history": false,
        },
    ])
}

fn compact_preview_payload(
    action: &str,
    request_id: &str,
    dry_run: bool,
    has_custom_instructions: bool,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "compact_preview",
        "action": action,
        "request_id": request_id,
        "dry_run": dry_run,
        "will_mutate_history": !dry_run,
        "safe_point": "dialogue_safe_point",
        "expected_status_event": "compact_request_status",
        "custom_instructions_present": has_custom_instructions,
        "history_boundary": if dry_run {
            "preview_only"
        } else {
            "compact_boundary_on_safe_point"
        },
        "summary_strategy": "manual_compact",
        "post_cleanup": !dry_run,
        "followup_command": if dry_run {
            "/compact run --confirm"
        } else {
            "/compact status"
        },
    })
}

fn compact_custom_instructions(args: &[String], start: usize) -> Option<String> {
    let custom_instructions = args
        .iter()
        .skip(start)
        .filter(|arg| arg.as_str() != "--confirm")
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let custom_instructions = custom_instructions.trim();
    if custom_instructions.is_empty() {
        None
    } else {
        Some(custom_instructions.to_string())
    }
}

fn build_clear_slash_response(
    request_id: String,
    args: &[String],
) -> std::result::Result<serde_json::Value, String> {
    let action = args
        .first()
        .map(|value| value.as_str())
        .unwrap_or("preview");

    match action {
        "status" => {
            let pending = get_pending_clear_request();
            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "clear",
                "status": "completed",
                "clear": {
                    "action": "status",
                    "pending": pending.is_some(),
                    "request_id": pending.as_ref().map(|request| request.request_id.clone()),
                    "dry_run": pending.as_ref().map(|request| request.dry_run),
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                },
            }))
        }
        "preview" | "plan" if args.len() == 1 || args.is_empty() => Ok(serde_json::json!({
            "subtype": "slash_command_result",
            "command": "clear",
            "status": "completed",
            "clear": {
                "action": "preview",
                "pending": get_pending_clear_request().is_some(),
                "dry_run": true,
                "requires_confirm": true,
                "execution_stage": "dialogue_safe_point",
                "mutation_supported": true,
                "next": "use /clear --confirm to queue conversation clearing",
            },
        })),
        "--confirm" | "confirm" if args.len() == 1 => {
            enqueue_pending_clear_request(request_id.clone(), false)
                .map_err(|err| format!("clear_request_blocked: {err}"))?;

            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "clear",
                "status": "queued",
                "clear": {
                    "action": "run",
                    "request_id": request_id,
                    "pending": true,
                    "dry_run": false,
                    "requires_confirm": false,
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                },
            }))
        }
        "run" if args.len() == 2 && args.get(1).map(String::as_str) == Some("--confirm") => {
            enqueue_pending_clear_request(request_id.clone(), false)
                .map_err(|err| format!("clear_request_blocked: {err}"))?;

            Ok(serde_json::json!({
                "subtype": "slash_command_result",
                "command": "clear",
                "status": "queued",
                "clear": {
                    "action": "run",
                    "request_id": request_id,
                    "pending": true,
                    "dry_run": false,
                    "requires_confirm": false,
                    "execution_stage": "dialogue_safe_point",
                    "mutation_supported": true,
                },
            }))
        }
        "run" => Err("unsupported_slash_command_args: clear run (missing --confirm)".to_string()),
        _ => Err(format!(
            "unsupported_slash_command_args: clear {}",
            args.join(" ")
        )),
    }
}

fn is_wired_stream_json_slash_command(command: &str) -> bool {
    matches!(
        command,
        "help"
            | "capabilities"
            | "status"
            | "model"
            | "profile"
            | "cost"
            | "hooks"
            | "memory"
            | "skills"
            | "mcp"
            | "ide"
            | "init"
            | "login"
            | "logout"
            | "diff"
            | "context"
            | "config"
            | "doctor"
            | "plugin"
            | "agents"
            | "approvals"
            | "permissions"
            | "plan"
            | "compact"
            | "clear"
    )
}

fn wired_stream_json_slash_commands() -> Vec<&'static str> {
    vec![
        "help",
        "capabilities",
        "status",
        "model",
        "profile",
        "cost",
        "hooks",
        "memory",
        "skills",
        "mcp",
        "ide",
        "init",
        "login",
        "logout",
        "diff",
        "context",
        "config",
        "doctor",
        "plugin",
        "agents",
        "approvals",
        "permissions",
        "plan",
        "compact",
        "clear",
    ]
}

fn permission_mode_values() -> Vec<&'static str> {
    vec![
        "default",
        "acceptEdits",
        "bypassPermissions",
        "plan",
        "dontAsk",
        "auto",
        "yolo",
    ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use mossen_agent::services::compact::pending_compact_request::{
        clear_pending_compact_request, get_pending_compact_request,
    };
    use mossen_agent::services::root::pending_clear_request::{
        clear_pending_clear_request, get_pending_clear_request,
    };

    struct PendingCompactTestGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for PendingCompactTestGuard {
        fn drop(&mut self) {
            clear_pending_compact_request();
        }
    }

    fn pending_compact_test_guard() -> PendingCompactTestGuard {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let guard = LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_pending_compact_request();
        PendingCompactTestGuard { _guard: guard }
    }

    fn permission_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn restore_permission_env(previous: Option<String>) {
        if let Some(previous) = previous {
            std::env::set_var(PERMISSION_MODE_ENV, previous);
        } else {
            std::env::remove_var(PERMISSION_MODE_ENV);
        }
    }

    fn auth_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    fn restore_env_vars(previous: Vec<(&'static str, Option<String>)>) {
        for (key, value) in previous {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    fn save_profile_runtime_env_vars() -> Vec<(&'static str, Option<String>)> {
        PROFILE_RUNTIME_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>()
    }

    fn clear_profile_runtime_env_vars() {
        for key in PROFILE_RUNTIME_ENV_KEYS {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn auth_token_source_label_uses_personal_backend_terms() {
        let hosted_label = auth_token_source_label(&mossen_utils::auth::AuthTokenSource::Hosted);

        assert_eq!(hosted_label, "legacy_stored_token");
        assert!(!hosted_label.contains("hosted"));
    }

    #[test]
    fn stream_json_slash_capability_manifest_has_no_available_unwired_commands() {
        let wired = wired_stream_json_slash_commands();
        for capability in get_stream_json_slash_command_capabilities() {
            assert_eq!(
                is_wired_stream_json_slash_command(&capability.command),
                wired.contains(&capability.command.as_str()),
                "/{} wired predicate and list diverged",
                capability.command
            );
            if matches!(capability.status, CommandStatus::Available) {
                assert!(
                    is_wired_stream_json_slash_command(&capability.command),
                    "/{} is advertised as available but would return unavailable_slash_command",
                    capability.command
                );
            }
        }
    }

    const PROFILE_RUNTIME_ENV_KEYS: &[&str] = &[
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_API_BASE_URL",
        "MOSSEN_API_KEY",
    ];

    fn model_state_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("model state lock")
    }

    fn config_state_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("config state lock")
    }

    fn process_cwd_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("process cwd lock")
    }

    fn run_git(cwd: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .expect("git command should start");
        assert!(
            status.success(),
            "git command failed in {}: git {}",
            cwd.display(),
            args.join(" ")
        );
    }

    struct ProcessCwdRestore {
        previous: std::path::PathBuf,
    }

    impl Drop for ProcessCwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }

    fn sample_can_use_tool_request(tool_use_id: &str) -> ControlRequestPayload {
        ControlRequestPayload::CanUseTool {
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "echo safe" }),
            permission_suggestions: None,
            blocked_path: None,
            decision_reason: None,
            tool_use_id: tool_use_id.to_string(),
            agent_id: Some("agent-1".to_string()),
            description: Some("Run shell command".to_string()),
        }
    }

    fn spawn_permission_request(
        io: &StructuredIO,
        request_id: &str,
        tool_use_id: &str,
    ) -> tokio::task::JoinHandle<Result<serde_json::Value>> {
        let io = io.clone();
        let request_id = request_id.to_string();
        let tool_use_id = tool_use_id.to_string();
        tokio::spawn(async move {
            io.send_request(sample_can_use_tool_request(&tool_use_id), Some(request_id))
                .await
        })
    }

    async fn wait_for_pending_permission_count(io: &StructuredIO, expected: usize) {
        for _ in 0..40 {
            let count = io.get_pending_permission_requests().await.len();
            if count == expected {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!(
            "expected {expected} pending permission requests, got {}",
            io.get_pending_permission_requests().await.len()
        );
    }

    async fn recv_control_request(
        outbound: &mut tokio::sync::mpsc::Receiver<StdoutMessage>,
        expected_request_id: &str,
    ) {
        match outbound.recv().await.expect("control request") {
            StdoutMessage::ControlRequest(request) => {
                assert_eq!(request.request_id, expected_request_id);
                assert!(matches!(
                    request.request,
                    ControlRequestPayload::CanUseTool { .. }
                ));
            }
            other => panic!("expected control request, got {other:?}"),
        }
    }

    async fn recv_cancel_request(
        outbound: &mut tokio::sync::mpsc::Receiver<StdoutMessage>,
        expected_request_id: &str,
    ) {
        match outbound.recv().await.expect("control cancel request") {
            StdoutMessage::ControlCancelRequest { request_id, .. } => {
                assert_eq!(request_id, expected_request_id);
            }
            other => panic!("expected control cancel request, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_approve_once_resolves_pending_permission() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-approve", "tool-approve");

        recv_control_request(&mut outbound, "perm-approve").await;
        wait_for_pending_permission_count(&io, 1).await;

        let response = io
            .resolve_pending_permission_with_approval_action("approve_once")
            .await
            .expect("approval bridge")
            .expect("response");

        assert_eq!(response.response.request_id, "perm-approve");
        assert_eq!(response.response.subtype, "success");
        assert_eq!(
            response.response.response.as_ref().expect("body")["behavior"],
            "allow"
        );
        assert!(
            response
                .response
                .response
                .as_ref()
                .expect("body")
                .get("updated_input")
                .is_none(),
            "permission decisions must serialize SDK camelCase fields"
        );
        recv_cancel_request(&mut outbound, "perm-approve").await;

        let result = request_task.await.expect("request task").expect("result");
        assert_eq!(result["behavior"], "allow");
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_reject_resolves_pending_permission_and_callback() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let resolved = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let resolved_for_callback = resolved.clone();
        io.set_on_control_request_resolved(Some(Box::new(move |request_id| {
            resolved_for_callback
                .lock()
                .expect("resolved lock")
                .push(request_id.to_string());
        })))
        .await;
        let request_task = spawn_permission_request(&io, "perm-reject", "tool-reject");

        recv_control_request(&mut outbound, "perm-reject").await;
        wait_for_pending_permission_count(&io, 1).await;

        let response = io
            .resolve_pending_permission_with_approval_action("reject")
            .await
            .expect("reject bridge")
            .expect("response");

        assert_eq!(
            response.response.response.as_ref().expect("body")["behavior"],
            "deny"
        );
        assert!(
            response.response.response.as_ref().expect("body")["message"]
                .as_str()
                .expect("message")
                .contains("Rejected")
        );
        recv_cancel_request(&mut outbound, "perm-reject").await;

        let result = request_task.await.expect("request task").expect("result");
        assert_eq!(result["behavior"], "deny");
        assert_eq!(
            resolved.lock().expect("resolved lock").clone(),
            vec!["perm-reject".to_string()]
        );
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_approve_for_session_returns_rule_update() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-session", "tool-session");

        recv_control_request(&mut outbound, "perm-session").await;
        wait_for_pending_permission_count(&io, 1).await;

        let response = io
            .resolve_pending_permission_with_approval_action("approve_for_session")
            .await
            .expect("session approval bridge")
            .expect("response");

        let body = response.response.response.as_ref().expect("body");
        assert_eq!(body["behavior"], "allow");
        assert_eq!(
            body["decisionReason"]["action"],
            TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION
        );
        let updates = body["updatedPermissions"]
            .as_array()
            .expect("updated permission rules");
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0]["type"], "addRules");
        assert_eq!(updates[0]["destination"], "session");
        assert_eq!(updates[0]["behavior"], "allow");
        assert_eq!(updates[0]["rules"][0]["toolName"], "Bash");
        assert_eq!(updates[0]["rules"][0]["ruleContent"], "echo safe");

        recv_cancel_request(&mut outbound, "perm-session").await;

        let result = request_task.await.expect("request task").expect("result");
        assert_eq!(result["behavior"], "allow");
        assert_eq!(result["updatedPermissions"][0]["destination"], "session");
        assert_eq!(
            result["updatedPermissions"][0]["rules"][0]["ruleContent"],
            "echo safe"
        );
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_edit_command_returns_updated_input() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-edit", "tool-edit");

        recv_control_request(&mut outbound, "perm-edit").await;
        wait_for_pending_permission_count(&io, 1).await;

        let response = io
            .resolve_pending_permission_with_approval_action_input(
                "edit_command",
                Some(serde_json::Value::String("echo edited".to_string())),
            )
            .await
            .expect("edit command bridge")
            .expect("response");

        let body = response.response.response.as_ref().expect("body");
        assert_eq!(body["behavior"], "allow");
        assert_eq!(body["updatedInput"]["command"], "echo edited");
        assert_eq!(body["userModified"], true);
        assert_eq!(
            body["decisionReason"]["action"],
            TERMINAL_APPROVAL_ACTION_EDIT_COMMAND
        );

        recv_cancel_request(&mut outbound, "perm-edit").await;

        let result = request_task.await.expect("request task").expect("result");
        assert_eq!(result["behavior"], "allow");
        assert_eq!(result["updatedInput"]["command"], "echo edited");
        assert_eq!(result["userModified"], true);
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_control_request_edit_command_submits_updated_input() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-edit-control", "tool-edit-control");

        recv_control_request(&mut outbound, "perm-edit-control").await;
        wait_for_pending_permission_count(&io, 1).await;

        io.process_line(
            r#"{"type":"control_request","request_id":"approval-action-edit-1","request":{"subtype":"terminal_approval_action","action":"edit_command","command":"echo via control"}}"#,
        )
        .await
        .expect("terminal approval action control request");

        recv_cancel_request(&mut outbound, "perm-edit-control").await;
        match outbound.recv().await.expect("approval action result") {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "approval-action-edit-1");
                assert_eq!(response.response.subtype, "terminal_approval_action_result");
                let body = response.response.response.expect("body");
                assert_eq!(body["status"], "submitted");
                assert_eq!(body["action"], TERMINAL_APPROVAL_ACTION_EDIT_COMMAND);
                assert_eq!(body["resolved_request_id"], "perm-edit-control");
                assert_eq!(
                    body["decision"]["updatedInput"]["command"],
                    "echo via control"
                );
            }
            other => panic!("expected terminal approval action response, got {other:?}"),
        }

        let result = request_task.await.expect("request task").expect("result");
        assert_eq!(result["behavior"], "allow");
        assert_eq!(result["updatedInput"]["command"], "echo via control");
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_edit_command_requires_updated_input() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-edit", "tool-edit");

        recv_control_request(&mut outbound, "perm-edit").await;
        wait_for_pending_permission_count(&io, 1).await;

        let error = io
            .resolve_pending_permission_with_approval_action("edit_command")
            .await
            .expect_err("edit action must not auto-approve without edited input");
        assert!(error
            .to_string()
            .contains("requires updatedInput or command"));
        wait_for_pending_permission_count(&io, 1).await;

        let response = permission_decision_control_response(
            "perm-edit".to_string(),
            permission_decision_for_approval_action("reject", None, None).expect("reject decision"),
        )
        .expect("cleanup response");
        io.inject_control_response(response).await;
        recv_cancel_request(&mut outbound, "perm-edit").await;
        let _ = request_task.await.expect("request task");
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_multiple_pending_permissions_fail_closed() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let first = spawn_permission_request(&io, "perm-one", "tool-one");
        recv_control_request(&mut outbound, "perm-one").await;
        let second = spawn_permission_request(&io, "perm-two", "tool-two");
        recv_control_request(&mut outbound, "perm-two").await;
        wait_for_pending_permission_count(&io, 2).await;

        let error = io
            .resolve_pending_permission_with_approval_action("approve_once")
            .await
            .expect_err("ambiguous approval must fail closed");
        assert!(error.to_string().contains("ambiguous"));
        wait_for_pending_permission_count(&io, 2).await;

        for request_id in ["perm-one", "perm-two"] {
            let response = permission_decision_control_response(
                request_id.to_string(),
                permission_decision_for_approval_action("reject", None, None)
                    .expect("reject decision"),
            )
            .expect("cleanup response");
            io.inject_control_response(response).await;
        }
        for request_id in ["perm-one", "perm-two"] {
            recv_cancel_request(&mut outbound, request_id).await;
        }
        let _ = first.await.expect("first request task");
        let _ = second.await.expect("second request task");
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn terminal_approval_action_bridge_returns_none_without_pending_permission() {
        let io = StructuredIO::new(false);

        let response = io
            .resolve_pending_permission_with_approval_action("approve_once")
            .await
            .expect("no pending approval action");

        assert!(response.is_none());
    }

    #[tokio::test]
    async fn slash_command_approvals_reports_redacted_pending_state() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let request_task = spawn_permission_request(&io, "perm-approvals", "tool-approvals");

        recv_control_request(&mut outbound, "perm-approvals").await;
        wait_for_pending_permission_count(&io, 1).await;

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-approvals-1","request":{"subtype":"slash_command","command":"/approval-history","args":["pending"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "slash-approvals-1");
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["subtype"], "slash_command_result");
                assert_eq!(body["command"], "approvals");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["approvals"]["action"], "pending");
                assert_eq!(body["approvals"]["pendingCount"], 1);
                assert_eq!(body["approvals"]["mutationSupported"], false);
                assert_eq!(body["approvals"]["resolveViaControlRequest"], true);
                assert_eq!(
                    body["approvals"]["actionControlSubtype"],
                    "terminal_approval_action"
                );
                assert_eq!(body["approvals"]["rawPayloadsRedacted"], true);
                assert_eq!(body["approvals"]["inputsRedacted"], true);
                assert!(body["approvals"]["decisions"]["total"].is_u64());

                let pending = body["approvals"]["pending"]
                    .as_array()
                    .expect("pending approvals");
                assert_eq!(pending.len(), 1);
                assert_eq!(pending[0]["requestId"], "perm-approvals");
                assert_eq!(pending[0]["toolName"], "Bash");
                assert_eq!(pending[0]["toolUseId"], "tool-approvals");
                assert_eq!(pending[0]["agentId"], "agent-1");
                assert_eq!(pending[0]["descriptionPresent"], true);
                assert_eq!(pending[0]["inputPreview"], "echo safe");
                assert_eq!(pending[0]["inputRedacted"], true);
                assert_eq!(pending[0]["rawInputIncluded"], false);
                assert!(pending[0].get("input").is_none());
                let actions = body["approvals"]["actions"].as_array().expect("actions");
                assert!(actions
                    .iter()
                    .any(|action| action["id"] == TERMINAL_APPROVAL_ACTION_APPROVE_ONCE));
                assert!(actions
                    .iter()
                    .any(|action| action["id"] == TERMINAL_APPROVAL_ACTION_REJECT));
                assert!(actions
                    .iter()
                    .any(|action| action["id"] == TERMINAL_APPROVAL_ACTION_APPROVE_FOR_SESSION));
                assert!(actions
                    .iter()
                    .any(|action| action["id"] == TERMINAL_APPROVAL_ACTION_EDIT_COMMAND));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let response = permission_decision_control_response(
            "perm-approvals".to_string(),
            permission_decision_for_approval_action("reject", None, None).expect("reject decision"),
        )
        .expect("cleanup response");
        io.inject_control_response(response).await;
        recv_cancel_request(&mut outbound, "perm-approvals").await;
        let _ = request_task.await.expect("request task");
        wait_for_pending_permission_count(&io, 0).await;
    }

    #[tokio::test]
    async fn slash_command_help_control_request_responds_with_manifest_summary() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-help-1","request":{"subtype":"slash_command","command":"/help","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "slash-help-1");
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["subtype"], "slash_command_result");
                assert_eq!(body["command"], "help");
                assert_eq!(body["status"], "completed");
                assert!(body["commands"]
                    .as_array()
                    .expect("commands")
                    .iter()
                    .any(|entry| entry["name"] == "compact" && entry["supported"] == true));
                assert_eq!(
                    body["streamJsonCapabilities"]["manifestVersion"],
                    STREAM_JSON_SLASH_CAPABILITY_MANIFEST_VERSION
                );
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_response_emits_render_items_when_renderer_is_attached() {
        let render_event_emitter = Arc::new(Mutex::new(StreamJsonRenderEventEmitter::new()));
        let io = StructuredIO::new_with_render_event_emitter(false, render_event_emitter);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-help-render-1","request":{"subtype":"slash_command","command":"/help","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        match outbound.recv().await.expect("control response") {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "slash-help-render-1");
                assert_eq!(response.response.subtype, "slash_command_result");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        match outbound.recv().await.expect("render event") {
            StdoutMessage::StreamEvent(value) => {
                assert_eq!(value["type"], STREAM_JSON_RENDER_EVENT_TYPE);
                assert_eq!(value["kind"], "slash_command_result");
                assert_eq!(value["payload"]["requestId"], "slash-help-render-1");
                assert_eq!(value["payload"]["command"], "help");
            }
            other => panic!("expected render event, got {other:?}"),
        }
        assert!(matches!(
            outbound.recv().await.expect("render snapshot"),
            StdoutMessage::StreamEvent(value)
                if value["type"] == STREAM_JSON_RENDER_SNAPSHOT_TYPE
                    && value["activity"]["kind"] == "slash_command_result"
        ));
        assert!(matches!(
            outbound.recv().await.expect("render frame"),
            StdoutMessage::StreamEvent(value)
                if value["type"] == STREAM_JSON_RENDER_FRAME_TYPE
        ));
        assert!(matches!(
            outbound.recv().await.expect("render patch"),
            StdoutMessage::StreamEvent(value)
                if value["type"] == STREAM_JSON_RENDER_PATCH_TYPE
        ));
        assert!(matches!(
            outbound.recv().await.expect("draw plan"),
            StdoutMessage::StreamEvent(value)
                if value["type"] == STREAM_JSON_RENDER_DRAW_PLAN_TYPE
        ));
    }

    #[tokio::test]
    async fn slash_command_status_reports_runtime_snapshot() {
        let _guard = permission_env_lock();
        let previous = std::env::var(PERMISSION_MODE_ENV).ok();
        std::env::set_var(PERMISSION_MODE_ENV, "plan");
        clear_pending_compact_request();
        clear_pending_clear_request();
        enqueue_pending_compact_request(
            "status-compact".to_string(),
            CompactMode::Manual,
            true,
            Some("keep context".to_string()),
        )
        .expect("enqueue compact request");
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-status-1","request":{"subtype":"slash_command","command":"/status","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "status");
                assert_eq!(body["runtime"]["permission_mode"], "plan");
                assert_eq!(body["runtime"]["pending_compact_request"], true);
                assert_eq!(body["runtime"]["pending_clear_request"], false);
                assert_eq!(body["runtime"]["compact"]["pending"], true);
                assert_eq!(body["runtime"]["clear"]["pending"], false);
                assert_eq!(body["runtime"]["compact"]["request_id"], "status-compact");
                assert_eq!(body["runtime"]["compact"]["dry_run"], true);
                assert_eq!(
                    body["runtime"]["compact"]["custom_instructions_present"],
                    true
                );
                assert!(body["runtime"]["agent"]["activeDialogues"].is_u64());
                assert!(body["runtime"]["agent"]["totalToolCallsStarted"].is_u64());
                assert!(body["runtime"]["agent"]["totalPermissionDecisions"].is_u64());
                assert!(body["runtime"]["agent"]["permissionModeDecisions"].is_u64());
                assert_eq!(body["runtime"]["render"]["event_stream"], true);
                assert_eq!(body["runtime"]["render"]["event_type"], "render_event");
                assert_eq!(body["runtime"]["render"]["schema_version"], 2);
                assert_eq!(body["runtime"]["render"]["snapshot_stream"], true);
                assert_eq!(
                    body["runtime"]["render"]["snapshot_type"],
                    "render_snapshot"
                );
                assert_eq!(body["runtime"]["render"]["snapshot_schema_version"], 1);
                assert_eq!(body["runtime"]["render"]["frame_stream"], true);
                assert_eq!(body["runtime"]["render"]["frame_type"], "render_frame");
                assert_eq!(body["runtime"]["render"]["frame_schema_version"], 1);
                assert_eq!(body["runtime"]["render"]["patch_stream"], true);
                assert_eq!(body["runtime"]["render"]["patch_type"], "render_patch");
                assert_eq!(body["runtime"]["render"]["patch_schema_version"], 1);
                assert_eq!(body["runtime"]["render"]["draw_plan_stream"], true);
                assert_eq!(
                    body["runtime"]["render"]["draw_plan_type"],
                    "render_draw_plan"
                );
                assert_eq!(body["runtime"]["render"]["draw_plan_schema_version"], 1);
                assert_eq!(body["runtime"]["render"]["draw_executor"], true);
                assert_eq!(
                    body["runtime"]["render"]["draw_executor_backend"],
                    "crossterm"
                );
                assert_eq!(body["runtime"]["render"]["draw_runtime_queue"], true);
                assert_eq!(body["runtime"]["render"]["draw_runtime_coalescing"], true);
                assert_eq!(body["runtime"]["render"]["draw_runtime_resize_aware"], true);
                assert_eq!(
                    body["runtime"]["render"]["draw_runtime_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_runtime_manual_scroll_deadline_suppression"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_runtime_manual_scroll_no_busy_retry"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_synchronized_update_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_executor_error_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_runtime_manual_scroll_critical_bypass"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_runtime_noncritical_scrollback_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_completion_manual_scroll_hold"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_frontend"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_emit"],
                    "terminal"
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_transport_isolated"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_log_isolated"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_scrollback_transcript"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_scrollback_soft_wrap"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_approval_widget"], true);
                assert_eq!(body["runtime"]["render"]["terminal_command_widget"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_file_change_summary_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_file_change_diff_separation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_file_change_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_file_change_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_file_change_context"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_error_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_error_detail_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_background_task_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_background_task_expanded_panel"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_top_stack_clip_diagnostics"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_visible_top_budget_report"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_diff_widget"], true);
                assert_eq!(body["runtime"]["render"]["terminal_error_widget"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_widget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_viewport_collision_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_dynamic_top_stack"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_retired_region_clear"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_event_pump"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_raw_mode_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_mouse_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_early_input_isolation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_ctrl_c_interrupt"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_interrupt_cancels_turn"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_interrupt_unblocks_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_resize_events"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_manual_scroll_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_native_mouse_scroll"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_mouse_capture_opt_in"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_mouse_capture_default_off"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_key_release_filter"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_bracketed_paste_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_frontend_edit_command_paste"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_semantic_colors"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_color_plain_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_color_no_color_env_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_color_dumb_terminal_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_color_clicolor_zero_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_unicode_grapheme_cluster_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_complex_unicode_width_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_ascii_glyph_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_unicode_ascii_mode_policy"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_ansi_control_sequence_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_osc_control_sequence_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_inline_control_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_carriage_return_progress_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_backspace_progress_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_control_char_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_tab_width_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_newline_write_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_bidi_control_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_unicode_format_control_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_directional_spoof_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_style_reset_after_line"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_status_bar_rich_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_status_bar_model_mode_reasoning"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_status_bar_context_usage"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_status_bar_width_variants"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_command_history"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_verification_results"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_residual_risks"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_final_summary_bounded_command_history"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_preview_lines"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_log_collapse_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_stream_tail_buffer"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_stream_chunk_accounting"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_bounded_tail_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_file_summary_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_hunk_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_collapsed_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_unified_diff_file_sections"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_file_grouped_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_per_file_hunk_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_widget_expand_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_command_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_diff_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_expanded_preview_budgets"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_expansion_immediate_redraw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_footer_keymap_hints"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_footer_hint_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_footer_hint_overflow_count"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_footer_full_hints_snapshot"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_contextual_interaction_metadata"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_widget_key_hints"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_decision_hints"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_action_model"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_focus_navigation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_edit_command_action"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_action_control_request"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_edit_command_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_edit_command_updated_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_local_edit_command_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_local_edit_command_submit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["permission_decision_updated_input_execution"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_session_action"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_action_activation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_enter_select"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_shortcut_actions"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_action_intent_model"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_decision_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_decision_bridge_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_approve_once_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_reject_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_interactive_gate_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_local_decision_submit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_submitted_nonblocking"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]
                        ["terminal_approval_submitted_retires_blocking_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_allow_always_session_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_interactive_gate_scoped_allow_always"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_interactive_gate_exact_command_rule"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_session_rule_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_session_rule_updates"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_edit_command_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_input_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_approval_bounded_input_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["terminal_critical_region_top_priority"],
                    true
                );
                assert_eq!(body["runtime"]["render"]["terminal_plan_widget"], true);
                assert_eq!(
                    body["runtime"]["render"]["terminal_plan_status_panel"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["preferred_strategy"],
                    "patch_regions"
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["replace_whole_screen"],
                    false
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["region_hashes"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["changed_region_ids"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["skip_unchanged_regions"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["frame_hash_excludes_sequence"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["patch_operations"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["skip_duplicate_frames"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["ansi_safe_lines"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["max_patch_line_cells"],
                    240
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["preserve_prompt_cursor"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["anchored_draw_plan"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["cursor_save_restore"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["clear_stale_region_lines"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["drop_superseded_frames"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["synchronized_update"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["absolute_row_moves"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["line_wrap_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["no_newline_writes"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["coalesced_runtime_queue"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["throttle_deadline_flush"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["resize_before_pending_flush"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["manual_scroll_preserves_active_update"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["manual_scroll_deadline_suppression"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["manual_scroll_no_busy_retry"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["synchronized_update_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["manual_scroll_critical_draw_bypass"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_frontend_emit_mode"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["ndjson_ansi_isolation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_frontend_log_isolation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_transcript_commit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_scrollback_append_once"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_scrollback_soft_wrap"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_clear_visible_rows_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_clear_rows_executor_enforced"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_executor_budget_hard_caps"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_budget_declaration_hard_cap"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_executor_zero_copy_budgeting"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_lines_zero_copy_budgeting"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_plan_borrowed_patch_inputs"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_plan_region_lines_no_preclone"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_owned_pending_submit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_pending_draw_plan_move_on_queue"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_last_report_snapshot"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_report_counters"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_diagnostics_json"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_last_report_json_summary"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_diagnostics_soak"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_diagnostics_no_stuck_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_final_diagnostics_export_env"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_final_diagnostics_json_file"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_oneshot_diagnostics_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_diagnostics_no_stuck_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_manual_scroll_preserved_counter"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_oneshot_manual_scroll_diagnostics_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_manual_scroll_hold_no_stuck_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_oneshot_resize_scroll_diagnostics_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_resize_scroll_latest_viewport"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_tail_hold_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_tail_hold_until_restore"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_draw_runtime_teardown_release_counter"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_teardown_release_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_teardown_release_no_stuck_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_resize_teardown_release_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_resize_teardown_latest_viewport"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_resize_interrupt_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_resize_interrupt_latest_viewport"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_bypass_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_bypasses_manual_scroll"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_reject_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_reject_no_execute"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_approve_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_approve_executes_and_renders"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_always_allow_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_always_allow_executes_and_renders"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_edit_command_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_edit_command_executes_updated_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_edit_cancel_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_edit_cancel_rejects_without_execute"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_resize_approve_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_resize_approve_latest_viewport"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_approval_active_scroll_reject_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_survives_active_scroll_reject"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_approval_reject_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_mouse_scroll_reject"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_approval_approve_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_mouse_scroll_approve_executes"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_approval_edit_command_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_mouse_scroll_edit_command_executes_updated_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_approval_always_allow_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_approval_mouse_scroll_always_allow_executes"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_output_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_output_manual_scroll_hold_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_acceptance_gate_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w305"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w307"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w308"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w309"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w310"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w311"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w312"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w313"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w314"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w315"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w316"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w317"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w318"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w319"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w321"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_matrix_w288_w322"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_no_fullscreen_clear"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_no_fullscreen_clear_pty_contract_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_product_external_pty_no_fullscreen_clear_w104_w320"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_command_pagedown_live_tail_matrix_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_pagedown_live_tail_release_after_approval_matrix"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_command_mouse_wheel_down_live_tail_matrix_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_mouse_wheel_down_live_tail_release_after_approval_matrix"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_output_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_output_mouse_scroll_hold_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_output_resize_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_output_resize_hold_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_output_resize_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_output_mouse_resize_hold_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_interrupt_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_interrupt_manual_scroll_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_interrupt_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_interrupt_mouse_scroll_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_resize_interrupt_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_resize_interrupt_manual_scroll_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_resize_interrupt_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_resize_interrupt_mouse_scroll_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_live_tail_release_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_live_tail_release_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_live_tail_release_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_mouse_live_tail_release_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_resize_live_tail_release_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_resize_live_tail_release_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_mouse_scroll_command_resize_live_tail_release_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_mouse_resize_live_tail_release_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_manual_scroll_command_end_live_tail_release_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_end_live_tail_release_after_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_command_end_live_tail_matrix_after_approval_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_command_end_live_tail_release_after_approval_matrix"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_interrupt_diagnostics_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_interrupt_no_stuck_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_interrupt_cleanup_balanced"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_cleanup_balance_pty_contract"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_completion_cleanup_balanced"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_external_process_scroll_resize_cleanup_balanced"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_render_status_heartbeat"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_slow_first_token_status_visible"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_status_heartbeat_stops_after_finish"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_slow_first_token_interrupt_pty_smoke"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_slow_first_token_interrupt_before_content"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_slow_first_token_interrupt_cleanup_balanced"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_transcript_final_assistant_dedupe"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_transcript_scrollback_appends_once"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_heartbeat_survives_metadata_update"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_no_empty_activity_during_slow_first_token"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_seeded_model_before_sdk_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_initial_status_model_not_unknown"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_status_heartbeat_replace_active"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_metadata_no_redundant_waiting_redraw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_assistant_activity_text_first"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_assistant_activity_no_byte_summary"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_duplicate_final_assistant_preview_stable"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_final_assistant_no_byte_summary_flash"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_assistant_activity_stable_line_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_assistant_activity_no_row_growth"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_noncritical_scrollback_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_render_completion_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_noncritical_scrollback_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_completion_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_draw_plan_only_dispatch"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_omits_snapshot_frame_patch_dispatch"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_resize_immediate_redraw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_resize_bypasses_superseded_sequence"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_resize_suppresses_duplicate_scrollback_append"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_resize_event_pending_gate"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_resize_burst_coalesced_before_queue"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_resize_pending_released_after_handle"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scroll_event_pending_gate"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scroll_burst_coalesced_before_queue"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scroll_pending_released_after_handle"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_manual_scroll_latest_state_coalescing"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_manual_scroll_opposite_state_supersedes_pending"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_priority_event_queue"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_priority_bypasses_low_priority_render_events"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_priority_drops_superseded_low_priority_backlog"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_priority_drain_preserves_resize_redraw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_priority_drain_reports_resize_follow_up"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_priority_drain_releases_manual_scroll_end"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_priority_drain_flushes_manual_scroll_pending_draw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_priority_fairness_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_priority_yields_to_sdk_and_permission"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_teardown_releases_manual_scroll_hold"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_teardown_flushes_pending_draw_after_release"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_soft_wrap_materialization_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_soft_wrap_budget_before_allocation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_scrollback_soft_wrap_streaming_sanitizer"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_soft_wrap_sanitize_without_full_line_clone"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_dynamic_top_stack"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_frontend_event_pump"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_raw_mode_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_frontend_mouse_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_early_input_isolation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_ctrl_c_interrupt"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_interrupt_cancels_turn"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_interrupt_unblocks_approval"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_frontend_resize_events"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_manual_scroll_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_native_mouse_scroll"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_mouse_capture_opt_in"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_mouse_capture_default_off"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_key_release_filter"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_bracketed_paste_capture"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_frontend_edit_command_paste"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_semantic_colors"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_color_plain_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["semantic_color_plain_text_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["no_color_env_plain_text_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_unicode_grapheme_cluster_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_complex_unicode_width_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_ascii_glyph_fallback"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_unicode_ascii_mode_policy"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_ansi_control_sequence_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_osc_control_sequence_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_inline_control_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_carriage_return_progress_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_backspace_progress_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_control_char_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_tab_width_normalization"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_newline_write_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_bidi_control_strip"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_unicode_format_control_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_directional_spoof_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_style_reset_after_line"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_style_reset_fail_safe"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_style_write_error_reset"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_cursor_restore_on_write_error"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_status_bar_rich_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_status_bar_model_mode_reasoning"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_status_bar_context_usage"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_status_bar_width_variants"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_final_summary_command_history"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_final_summary_verification_results"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_final_summary_residual_risks"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_final_summary_bounded_command_history"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_command_preview_lines"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_command_log_collapse_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_command_stream_tail_buffer"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_command_stream_chunk_accounting"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_command_bounded_tail_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_diff_file_summary_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_diff_hunk_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_diff_collapsed_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_unified_diff_file_sections"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_diff_file_grouped_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_diff_per_file_hunk_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_widget_expand_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_command_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_diff_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_expanded_preview_budgets"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_expansion_immediate_redraw"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_footer_keymap_hints"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_footer_hint_budget"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_footer_hint_overflow_count"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_footer_full_hints_snapshot"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_contextual_interaction_metadata"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_widget_key_hints"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_decision_hints"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_action_model"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_focus_navigation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_edit_command_action"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_action_control_request"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_edit_command_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_edit_command_updated_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_local_edit_command_input"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_local_edit_command_submit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["permission_decision_updated_input_execution"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_session_action"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_action_activation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_enter_select"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_shortcut_actions"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_action_intent_model"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_decision_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_decision_bridge_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_approve_once_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_reject_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_interactive_gate_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_local_decision_submit"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_submitted_nonblocking"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_submitted_retires_blocking_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_allow_always_session_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_interactive_gate_scoped_allow_always"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_interactive_gate_exact_command_rule"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_session_rule_bridge"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_session_rule_updates"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_edit_command_fail_closed"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_approval_input_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_approval_bounded_input_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_approval_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["approval_blocks_active_log"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["approval_draw_plan_blocking_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_command_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["command_output_summary_only"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_diff_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_file_change_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_file_change_summary_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_file_change_diff_separation"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_file_change_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_file_change_expand_collapse"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_final_summary_file_change_context"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_error_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_error_detail_preview"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_background_task_expansion_controls"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_background_task_expanded_panel"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_top_stack_clip_diagnostics"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_visible_top_budget_report"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["diff_collapsed_by_default"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_plan_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_plan_progress_summary"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_plan_status_panel"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_error_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["layered_error_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["independent_final_summary_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["final_summary_terminal_region"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_top_bottom_collision_guard"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_independent_widget_suppresses_duplicate_active"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]["terminal_retired_region_clear"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["draw_contract"]
                        ["terminal_critical_region_top_priority"],
                    true
                );
                assert!(body["runtime"]["render"]["raw_sdk_messages"]
                    .as_bool()
                    .unwrap_or(false));
                assert!(body["runtime"]["render"]["throttle_ms"].is_u64());
                assert_eq!(
                    body["runtime"]["render"]["ordering"]["monotonic_event_sequence"],
                    true
                );
                assert_eq!(
                    body["runtime"]["render"]["ordering"]["source_message_sequence"],
                    true
                );
                assert!(body["runtime"]["slash"]["implemented_commands"]
                    .as_array()
                    .expect("implemented commands")
                    .iter()
                    .any(|command| command == "permissions"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
        clear_pending_compact_request();
        clear_pending_clear_request();
        restore_permission_env(previous);
    }

    #[tokio::test]
    async fn slash_command_permissions_reports_current_mode() {
        let _guard = permission_env_lock();
        let previous = std::env::var(PERMISSION_MODE_ENV).ok();
        std::env::remove_var(PERMISSION_MODE_ENV);
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-1","request":{"subtype":"slash_command","command":"permissions","args":["mode"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "permissions");
                assert_eq!(body["permissions"]["mode"], "default");
                assert_eq!(body["permissions"]["source"], "default");
                assert_eq!(body["permissions"]["mutation_supported"], true);
                assert!(body["permissions"]["available_modes"]
                    .as_array()
                    .expect("available modes")
                    .iter()
                    .any(|mode| mode == "plan"));
                let mode_options = body["permissions"]["mode_options"]
                    .as_array()
                    .expect("mode options");
                assert!(mode_options.iter().any(|option| {
                    option["codex_label"] == "Suggest"
                        && option["codex_value"] == "suggest"
                        && option["value"] == "default"
                        && option["index"] == 0
                }));
                assert!(mode_options.iter().any(|option| {
                    option["codex_label"] == "Auto Edit"
                        && option["codex_value"] == "auto-edit"
                        && option["value"] == "acceptEdits"
                        && option["shell_approval"] == "ask"
                }));
                assert!(mode_options.iter().any(|option| {
                    option["codex_label"] == "Full Auto"
                        && option["codex_value"] == "full-auto"
                        && option["value"] == "bypassPermissions"
                        && option["shell_approval"] == "auto"
                }));
                assert_eq!(body["permissions"]["codex_mode"]["value"], "suggest");
                assert_eq!(body["permissions"]["codex_mode"]["edit_approval"], "ask");
                assert_eq!(body["permissions"]["codex_mode"]["shell_approval"], "ask");
                assert_eq!(
                    body["permissions"]["terminal_control"]["status_line_label"],
                    "Supervised"
                );
                assert_eq!(body["permissions"]["terminal_control"]["selected_index"], 0);
                assert_eq!(
                    body["permissions"]["terminal_control"]["selected_value"]["codex_value"],
                    "suggest"
                );
                assert_eq!(
                    body["permissions"]["terminal_control"]["aliases_accepted"],
                    true
                );
                assert_eq!(body["permissions"]["selected_option"]["value"], "default");
                assert_eq!(
                    body["permissions"]["mode_picker"]["kind"],
                    "permission_mode_picker"
                );
                assert_eq!(
                    body["permissions"]["mode_picker"]["layout"],
                    "segmented_control"
                );
                assert_eq!(body["permissions"]["mode_picker"]["selected_index"], 0);
                assert_eq!(
                    body["permissions"]["mode_picker"]["codex_order"][2],
                    "auto-edit"
                );
                assert_eq!(
                    body["permissions"]["mode_picker"]["keyboard"]["enter_selects"],
                    true
                );
                assert_eq!(body["permissions"]["codex_approval_modes"][0], "suggest");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
        restore_permission_env(previous);
    }

    #[tokio::test]
    async fn slash_command_permissions_mode_updates_session_env() {
        let _guard = permission_env_lock();
        let previous = std::env::var(PERMISSION_MODE_ENV).ok();
        std::env::set_var(PERMISSION_MODE_ENV, "default");
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-2","request":{"subtype":"slash_command","command":"/permission-mode","args":["full","auto"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_MODE_ENV).expect("permission env"),
            "bypassPermissions"
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "permissions");
                assert_eq!(body["permissions"]["mode"], "bypassPermissions");
                assert_eq!(body["permissions"]["mode_label"], "Full Auto");
                assert_eq!(body["permissions"]["previous_mode"], "default");
                assert_eq!(body["permissions"]["source"], "slash_command");
                assert_eq!(body["permissions"]["codex_mode"]["value"], "full-auto");
                assert_eq!(body["permissions"]["codex_mode"]["shell_approval"], "auto");
                assert_eq!(body["permissions"]["terminal_control"]["selected_index"], 3);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
        restore_permission_env(previous);
    }

    #[tokio::test]
    async fn slash_command_permissions_accepts_codex_mode_aliases() {
        let _guard = permission_env_lock();
        let previous = std::env::var(PERMISSION_MODE_ENV).ok();
        std::env::set_var(PERMISSION_MODE_ENV, "default");
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-auto-edit","request":{"subtype":"slash_command","command":"/approval-mode","args":["auto-edit"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_MODE_ENV).expect("permission env"),
            "acceptEdits"
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["mode"], "acceptEdits");
                assert_eq!(body["permissions"]["requested_mode"], "auto-edit");
                assert_eq!(body["permissions"]["codex_mode"]["value"], "auto-edit");
                assert_eq!(
                    body["permissions"]["terminal_control"]["status_line_label"],
                    "Accept Edits"
                );
                assert_eq!(
                    body["permissions"]["selected_option"]["codex_label"],
                    "Auto Edit"
                );
                assert_eq!(
                    body["permissions"]["selected_option"]["edit_approval"],
                    "auto"
                );
                assert_eq!(
                    body["permissions"]["selected_option"]["shell_approval"],
                    "ask"
                );
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-suggest","request":{"subtype":"slash_command","command":"/permissions","args":["choose","suggest"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_MODE_ENV).expect("permission env"),
            "default"
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["mode"], "default");
                assert_eq!(body["permissions"]["requested_mode"], "suggest");
                assert_eq!(body["permissions"]["codex_mode"]["value"], "suggest");
                assert_eq!(
                    body["permissions"]["selected_option"]["codex_label"],
                    "Suggest"
                );
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        restore_permission_env(previous);
    }

    #[tokio::test]
    async fn slash_command_permissions_rule_subcommands_update_session_env() {
        let _guard = permission_env_lock();
        let previous = vec![
            (PERMISSION_MODE_ENV, std::env::var(PERMISSION_MODE_ENV).ok()),
            (
                PERMISSION_ALLOW_RULES_ENV,
                std::env::var(PERMISSION_ALLOW_RULES_ENV).ok(),
            ),
            (
                PERMISSION_DENY_RULES_ENV,
                std::env::var(PERMISSION_DENY_RULES_ENV).ok(),
            ),
        ];
        std::env::set_var(PERMISSION_MODE_ENV, "default");
        std::env::remove_var(PERMISSION_ALLOW_RULES_ENV);
        std::env::remove_var(PERMISSION_DENY_RULES_ENV);
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-allow","request":{"subtype":"slash_command","command":"/permissions","args":["allow","Bash","cargo","test"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_ALLOW_RULES_ENV).expect("allow rules"),
            "Bash cargo test"
        );
        assert!(std::env::var(PERMISSION_DENY_RULES_ENV).is_err());
        let response = outbound.recv().await.expect("allow response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["action"], "allow");
                assert_eq!(body["permissions"]["rule_counts"]["allow"], 1);
                assert_eq!(body["permissions"]["rule_counts"]["deny"], 0);
                assert_eq!(body["permissions"]["rule_mutation_supported"], true);
                assert_eq!(body["permissions"]["rules"]["raw_patterns_included"], false);
                assert_eq!(body["permissions"]["rule_update"]["behavior"], "allow");
                assert_eq!(
                    body["permissions"]["rule_update"]["rule_pattern_redacted"],
                    true
                );
                let encoded = serde_json::to_string(&body).expect("response json");
                assert!(
                    !encoded.contains("Bash cargo test"),
                    "permission rule response must not echo raw patterns"
                );
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-deny","request":{"subtype":"slash_command","command":"/permissions","args":["deny","Write"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_ALLOW_RULES_ENV).expect("allow rules"),
            "Bash cargo test"
        );
        assert_eq!(
            std::env::var(PERMISSION_DENY_RULES_ENV).expect("deny rules"),
            "Write"
        );
        let response = outbound.recv().await.expect("deny response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["action"], "deny");
                assert_eq!(body["permissions"]["rule_counts"]["allow"], 1);
                assert_eq!(body["permissions"]["rule_counts"]["deny"], 1);
                assert_eq!(body["permissions"]["rule_update"]["behavior"], "deny");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-list","request":{"subtype":"slash_command","command":"/permissions","args":["list"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("list response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["action"], "list");
                assert_eq!(body["permissions"]["rule_counts"]["allow"], 1);
                assert_eq!(body["permissions"]["rule_counts"]["deny"], 1);
                assert_eq!(body["permissions"]["rules"]["allow_count"], 1);
                assert_eq!(body["permissions"]["rules"]["deny_count"], 1);
                assert_eq!(body["permissions"]["rules"]["patterns_redacted"], true);
                assert_eq!(body["permissions"]["rules"]["raw_patterns_included"], false);
                let encoded = serde_json::to_string(&body).expect("response json");
                assert!(!encoded.contains("Bash cargo test"));
                assert!(!encoded.contains("Write"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-perms-reset","request":{"subtype":"slash_command","command":"/permissions","args":["reset"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert!(std::env::var(PERMISSION_ALLOW_RULES_ENV).is_err());
        assert!(std::env::var(PERMISSION_DENY_RULES_ENV).is_err());
        let response = outbound.recv().await.expect("reset response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["permissions"]["action"], "reset");
                assert_eq!(body["permissions"]["rule_counts"]["allow"], 0);
                assert_eq!(body["permissions"]["rule_counts"]["deny"], 0);
                assert_eq!(body["permissions"]["rule_update"]["behavior"], "none");
                assert_eq!(body["permissions"]["rule_update"]["applied"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        restore_env_vars(previous);
    }

    #[tokio::test]
    async fn slash_command_plan_enters_and_exits_plan_mode() {
        let _guard = permission_env_lock();
        let previous = std::env::var(PERMISSION_MODE_ENV).ok();
        std::env::set_var(PERMISSION_MODE_ENV, "default");
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-plan-1","request":{"subtype":"slash_command","command":"/plan","args":["enter"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_MODE_ENV).expect("plan env"),
            "plan"
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "plan");
                assert_eq!(body["plan"]["action"], "enter");
                assert_eq!(body["plan"]["active"], true);
                assert_eq!(body["plan"]["mode"], "plan");
                assert_eq!(body["plan"]["previous_mode"], "default");
                assert_eq!(body["plan"]["mode_mutation_supported"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-plan-2","request":{"subtype":"slash_command","command":"plan-mode","args":["exit"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(
            std::env::var(PERMISSION_MODE_ENV).expect("default env"),
            "default"
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "plan");
                assert_eq!(body["plan"]["action"], "exit");
                assert_eq!(body["plan"]["active"], false);
                assert_eq!(body["plan"]["mode"], "default");
                assert_eq!(body["plan"]["previous_mode"], "plan");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        restore_permission_env(previous);
    }

    #[tokio::test]
    async fn slash_command_diff_returns_bounded_git_summary() {
        let _guard = process_cwd_lock();
        let previous = std::env::current_dir().expect("current cwd");
        let _restore = ProcessCwdRestore { previous };
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).expect("src dir");
        std::fs::write(root.join("src/lib.rs"), "pub fn value() -> i32 { 1 }\n")
            .expect("seed file");
        run_git(root, &["init"]);
        run_git(root, &["add", "src/lib.rs"]);
        run_git(
            root,
            &[
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Test User",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "seed",
            ],
        );
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn value() -> i32 { 2 }\npub fn extra() -> i32 { 3 }\n",
        )
        .expect("modify file");
        std::fs::write(root.join("README.md"), "# draft\n").expect("untracked file");
        std::env::set_current_dir(root).expect("switch cwd");

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-diff-1","request":{"subtype":"slash_command","command":"/changes","args":["summary"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "diff");
                assert_eq!(body["diff"]["available"], true);
                assert_eq!(body["diff"]["gitRepo"], true);
                assert_eq!(body["diff"]["rawDiffIncluded"], false);
                assert_eq!(body["diff"]["contentIncluded"], false);
                assert!(body["diff"]["filesChanged"].as_u64().unwrap_or(0) >= 2);
                assert!(body["diff"]["linesAdded"].as_u64().unwrap_or(0) >= 1);
                let files = body["diff"]["files"].as_array().expect("files");
                assert!(files.iter().any(|file| {
                    file["path"] == "src/lib.rs"
                        && file["added"].as_u64().unwrap_or(0) >= 1
                        && file["removed"].as_u64().unwrap_or(0) >= 1
                }));
                assert!(files
                    .iter()
                    .any(|file| file["path"] == "README.md" && file["untracked"] == true));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_context_reports_token_window_snapshot() {
        let _guard = model_state_lock();
        let previous_override = get_main_loop_model_override();
        crate::bootstrap::reset_cost_state();
        set_main_loop_model_override(Some("mossen-balanced-4".to_string()));
        let mut usage = std::collections::HashMap::new();
        usage.insert(
            "mossen-balanced-4".to_string(),
            crate::bootstrap::ModelUsage {
                input_tokens: 10_000,
                output_tokens: 500,
                cache_read_input_tokens: 2_000,
                cache_creation_input_tokens: 1_000,
                web_search_requests: 1,
                cost_usd: 0.42,
                context_window: 200_000,
                max_output_tokens: 32_000,
            },
        );
        crate::bootstrap::set_cost_state_for_restore(
            0.42,
            1200,
            1000,
            250,
            0,
            0,
            Some(1200),
            Some(usage),
        );

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-context-1","request":{"subtype":"slash_command","command":"/ctx","args":["breakdown"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "context");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["context"]["action"], "breakdown");
                assert_eq!(body["context"]["usageAvailable"], true);
                assert_eq!(body["context"]["analysisDepth"], "token_usage_snapshot");
                assert_eq!(body["context"]["messageLevelAnalysisIncluded"], false);
                assert_eq!(body["context"]["model"]["effective"], "mossen-balanced-4");
                assert_eq!(body["context"]["totals"]["inputTokens"], 10_000);
                assert_eq!(body["context"]["totals"]["outputTokens"], 500);
                assert_eq!(body["context"]["totals"]["cacheReadInputTokens"], 2_000);
                assert_eq!(body["context"]["totals"]["cacheCreationInputTokens"], 1_000);
                assert_eq!(body["context"]["totals"]["contextInputTokens"], 13_000);
                assert_eq!(body["context"]["totals"]["webSearchRequests"], 1);
                assert_eq!(body["context"]["window"]["contextWindowTokens"], 200_000);
                assert_eq!(body["context"]["window"]["maxOutputTokens"], 32_000);
                assert_eq!(body["context"]["window"]["effectiveWindowTokens"], 180_000);
                assert_eq!(body["context"]["window"]["remainingTokens"], 187_000);
                assert_eq!(body["context"]["window"]["usedPercent"], 7);
                assert_eq!(body["context"]["window"]["status"], "normal");
                assert_eq!(body["context"]["thresholds"]["warningTokens"], 160_000);
                assert_eq!(body["context"]["thresholds"]["autoCompactTokens"], 167_000);
                assert_eq!(body["context"]["thresholds"]["autoCompactEligible"], false);
                assert_eq!(body["context"]["modelUsageCount"], 1);
                assert_eq!(body["context"]["rawMessagesIncluded"], false);
                assert_eq!(body["context"]["messageContentRedacted"], true);
                assert_eq!(body["context"]["mutationSupported"], false);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        crate::bootstrap::reset_cost_state();
        set_main_loop_model_override(previous_override);
    }

    #[tokio::test]
    async fn slash_command_config_returns_redacted_runtime_snapshot() {
        let _config_guard = config_state_lock();
        let _permission_guard = permission_env_lock();
        let previous_permission_mode = std::env::var(PERMISSION_MODE_ENV).ok();
        let previous_allowed_sources = get_allowed_setting_sources();
        let previous_flag_settings_path = get_flag_settings_path();
        let previous_flag_settings_inline = get_flag_settings_inline();
        let previous_client_type = get_client_type();
        let previous_remote_mode = get_is_remote_mode();
        let previous_bypass_mode = get_session_bypass_permissions_mode();
        let previous_inline_plugins = get_inline_plugins();
        let previous_use_cowork_plugins = get_use_cowork_plugins();
        let previous_chrome_flag_override = get_chrome_flag_override();

        std::env::set_var(PERMISSION_MODE_ENV, "plan");
        crate::bootstrap::set_allowed_setting_sources(vec![
            "userSettings".to_string(),
            "projectSettings".to_string(),
        ]);
        crate::bootstrap::set_flag_settings_path(Some("/tmp/private/settings.json".to_string()));
        crate::bootstrap::set_flag_settings_inline(Some(serde_json::json!({
            "apiKey": "secret-value",
            "theme": "dark",
        })));
        crate::bootstrap::set_client_type("stream-json-test".to_string());
        crate::bootstrap::set_is_remote_mode(true);
        crate::bootstrap::set_session_bypass_permissions_mode(true);
        crate::bootstrap::set_inline_plugins(vec!["private-plugin".to_string()]);
        crate::bootstrap::set_use_cowork_plugins(true);
        crate::bootstrap::set_chrome_flag_override(Some(true));

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-config-1","request":{"subtype":"slash_command","command":"/settings","args":["sources"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "config");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["config"]["action"], "sources");
                assert_eq!(body["config"]["runtime"]["protocol"], "stream_json");
                assert_eq!(body["config"]["runtime"]["clientType"], "stream-json-test");
                assert_eq!(body["config"]["runtime"]["remoteMode"], true);
                assert_eq!(
                    body["config"]["runtime"]["sessionBypassPermissionsMode"],
                    true
                );
                assert_eq!(body["config"]["runtime"]["permissionMode"]["mode"], "plan");
                assert_eq!(
                    body["config"]["runtime"]["permissionMode"]["rawModeRedacted"],
                    true
                );
                assert_eq!(body["config"]["settings"]["allowedSettingSourceCount"], 2);
                assert_eq!(body["config"]["settings"]["flagSettingsPathPresent"], true);
                assert_eq!(body["config"]["settings"]["flagSettingsPathRedacted"], true);
                assert_eq!(
                    body["config"]["settings"]["flagSettingsInlinePresent"],
                    true
                );
                assert_eq!(
                    body["config"]["settings"]["flagSettingsInlineType"],
                    "object"
                );
                assert_eq!(
                    body["config"]["settings"]["flagSettingsInlineObjectKeyCount"],
                    2
                );
                assert_eq!(
                    body["config"]["settings"]["flagSettingsInlineValuesRedacted"],
                    true
                );
                assert_eq!(body["config"]["settings"]["rawConfigIncluded"], false);
                assert_eq!(body["config"]["settings"]["rawConfigRedacted"], true);
                assert!(body["config"]["settings"].get("flagSettingsPath").is_none());
                assert_eq!(body["config"]["plugins"]["inlinePluginCount"], 1);
                assert_eq!(
                    body["config"]["plugins"]["inlinePluginNamesIncluded"],
                    false
                );
                assert!(body["config"]["plugins"].get("inlinePlugins").is_none());
                assert_eq!(body["config"]["plugins"]["coworkPluginsEnabled"], true);
                assert_eq!(body["config"]["plugins"]["chromeFlagOverride"], true);
                assert_eq!(body["config"]["security"]["mutationSupported"], false);
                assert_eq!(body["config"]["security"]["secretsRedacted"], true);
                assert_eq!(body["config"]["mutationSupported"], false);

                let body_text = serde_json::to_string(&body).expect("serialize body");
                assert!(!body_text.contains("/tmp/private/settings.json"));
                assert!(!body_text.contains("secret-value"));
                assert!(!body_text.contains("apiKey"));
                assert!(!body_text.contains("private-plugin"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        restore_permission_env(previous_permission_mode);
        crate::bootstrap::set_allowed_setting_sources(previous_allowed_sources);
        crate::bootstrap::set_flag_settings_path(previous_flag_settings_path);
        crate::bootstrap::set_flag_settings_inline(previous_flag_settings_inline);
        crate::bootstrap::set_client_type(previous_client_type);
        crate::bootstrap::set_is_remote_mode(previous_remote_mode);
        crate::bootstrap::set_session_bypass_permissions_mode(previous_bypass_mode);
        crate::bootstrap::set_inline_plugins(previous_inline_plugins);
        crate::bootstrap::set_use_cowork_plugins(previous_use_cowork_plugins);
        crate::bootstrap::set_chrome_flag_override(previous_chrome_flag_override);
    }

    #[tokio::test]
    async fn slash_command_doctor_returns_redacted_runtime_health_snapshot() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-doctor-1","request":{"subtype":"slash_command","command":"/doctor","args":["render"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "doctor");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["doctor"]["action"], "render");
                assert_eq!(body["doctor"]["analysisDepth"], "runtime_health_snapshot");
                assert_eq!(body["doctor"]["externalChecksRun"], false);
                assert_eq!(body["doctor"]["networkChecksRun"], false);
                assert_eq!(body["doctor"]["installChecksRun"], false);
                assert_eq!(body["doctor"]["slowChecksSkipped"], true);
                assert_eq!(body["doctor"]["blockingChecksSkipped"], true);
                assert_eq!(body["doctor"]["runtime"]["protocol"], "stream_json");
                assert!(body["doctor"]["runtime"]["pendingControlRequests"].is_u64());
                assert_eq!(body["doctor"]["slash"]["doctorCommandWired"], true);
                assert!(
                    body["doctor"]["slash"]["wiredCommandCount"]
                        .as_u64()
                        .unwrap_or(0)
                        >= 1
                );
                assert_eq!(body["doctor"]["render"]["ready"], true);
                assert_eq!(body["doctor"]["render"]["eventStream"], true);
                assert_eq!(body["doctor"]["render"]["snapshotStream"], true);
                assert_eq!(body["doctor"]["render"]["frameStream"], true);
                assert_eq!(body["doctor"]["render"]["patchStream"], true);
                assert_eq!(body["doctor"]["render"]["drawPlanStream"], true);
                assert_eq!(body["doctor"]["render"]["drawExecutor"], true);
                assert_eq!(body["doctor"]["render"]["terminalFrontend"], true);
                assert_eq!(
                    body["doctor"]["render"]["manualScrollDeadlineSuppression"],
                    true
                );
                assert_eq!(
                    body["doctor"]["render"]["synchronizedUpdateFailClosed"],
                    true
                );
                assert!(body["doctor"]["agent"]["activeDialogues"].is_u64());
                assert_eq!(body["doctor"]["agent"]["lastErrorRedacted"], true);
                assert_eq!(body["doctor"]["mcp"]["serverDetailsIncluded"], false);
                assert_eq!(body["doctor"]["mcp"]["rawConfigRedacted"], true);
                assert_eq!(body["doctor"]["mcp"]["errorDetailsRedacted"], true);
                assert!(body["doctor"]["mcp"].get("servers").is_none());
                assert_eq!(body["doctor"]["redaction"]["pathsRedacted"], true);
                assert_eq!(body["doctor"]["redaction"]["secretsRedacted"], true);
                assert_eq!(body["doctor"]["redaction"]["installPathsRedacted"], true);
                assert_eq!(body["doctor"]["mutationSupported"], false);

                let body_text = serde_json::to_string(&body).expect("serialize body");
                assert!(!body_text.contains("binPath"));
                assert!(!body_text.contains("installationPath"));
                assert!(!body_text.contains("currentExe"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_doctor_guides_when_model_profile_is_missing() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = save_profile_runtime_env_vars();
        clear_profile_runtime_env_vars();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::Value::Null,
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::Null,
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["doctor"]["healthStatus"], "warning");
        assert_eq!(body["doctor"]["modelConfig"]["status"], "missing");
        assert_eq!(
            body["doctor"]["modelConfig"]["issues"][0],
            "no_model_profile"
        );
        assert_eq!(
            body["doctor"]["modelConfig"]["nextCommands"][0],
            "mossen --add-model-profile my-model --provider openai-compatible --baseURL https://api.example.com/v1 --model your-model-name --apiKey \"$YOUR_API_KEY\""
        );
        assert_eq!(body["doctor"]["modelConfig"]["rawConfigIncluded"], false);
        assert_eq!(body["doctor"]["modelConfig"]["baseUrlsRedacted"], true);
        assert_eq!(body["doctor"]["modelConfig"]["apiKeysRedacted"], true);

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_doctor_warns_when_active_profile_is_missing() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = save_profile_runtime_env_vars();
        clear_profile_runtime_env_vars();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "example": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-large",
                    "apiKey": "sk-test-example-secret-value"
                }
            }),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("missing".to_string()),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["doctor"]["healthStatus"], "warning");
        assert_eq!(body["doctor"]["modelConfig"]["status"], "warning");
        assert!(body["doctor"]["modelConfig"]["issues"]
            .as_array()
            .expect("issues array")
            .iter()
            .any(|issue| issue == "active_profile_not_found"));
        assert_eq!(
            body["doctor"]["modelConfig"]["rawActiveProfileValid"],
            false
        );
        assert_eq!(body["doctor"]["modelConfig"]["visibleProfileCount"], 1);
        assert_eq!(
            body["doctor"]["modelConfig"]["nextCommands"][0],
            "mossen --list-model-profiles"
        );

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_doctor_reports_invalid_model_profiles() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = save_profile_runtime_env_vars();
        clear_profile_runtime_env_vars();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "broken": {
                    "provider": "openai-compatible",
                    "baseURL": "https://invalid-provider.example/v1",
                    "model": ""
                }
            }),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("broken".to_string()),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["doctor"]["healthStatus"], "warning");
        assert_eq!(body["doctor"]["modelConfig"]["status"], "missing");
        let issues = body["doctor"]["modelConfig"]["issues"]
            .as_array()
            .expect("issues array");
        assert!(issues
            .iter()
            .any(|issue| issue == "no_valid_settings_profiles"));
        assert!(issues.iter().any(|issue| issue == "no_model_profile"));
        assert_eq!(body["doctor"]["modelConfig"]["rawProfileEntryCount"], 1);
        assert_eq!(
            body["doctor"]["modelConfig"]["invalidSettingsProfileCount"],
            1
        );
        assert_eq!(body["doctor"]["modelConfig"]["rawConfigIncluded"], false);
        let body_text = serde_json::to_string(&body).expect("serialize body");
        assert!(!body_text.contains("https://invalid-provider.example/v1"));

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_doctor_reports_partial_custom_backend_env() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = save_profile_runtime_env_vars();
        clear_profile_runtime_env_vars();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::Value::Null,
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::Null,
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        std::env::set_var("MOSSEN_CODE_USE_CUSTOM_BACKEND", "1");
        std::env::set_var(
            "MOSSEN_CODE_CUSTOM_BASE_URL",
            "https://partial-provider.example/v1",
        );

        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["doctor"]["healthStatus"], "warning");
        assert_eq!(body["doctor"]["modelConfig"]["status"], "missing");
        let issues = body["doctor"]["modelConfig"]["issues"]
            .as_array()
            .expect("issues array");
        assert!(issues.iter().any(|issue| issue == "no_model_profile"));
        assert!(issues
            .iter()
            .any(|issue| issue == "custom_backend_env_incomplete"));
        assert_eq!(body["doctor"]["modelConfig"]["fallbackEnvPartial"], true);
        assert_eq!(
            body["doctor"]["modelConfig"]["env"]["customBackendEnabled"],
            true
        );
        assert_eq!(body["doctor"]["modelConfig"]["env"]["baseUrlPresent"], true);
        assert_eq!(body["doctor"]["modelConfig"]["env"]["modelPresent"], false);
        assert_eq!(body["doctor"]["modelConfig"]["env"]["apiKeyPresent"], false);
        assert_eq!(body["doctor"]["modelConfig"]["env"]["valuesRedacted"], true);
        let body_text = serde_json::to_string(&body).expect("serialize body");
        assert!(!body_text.contains("https://partial-provider.example/v1"));

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_doctor_redacts_configured_model_profile_secrets() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = save_profile_runtime_env_vars();
        clear_profile_runtime_env_vars();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "private": {
                    "provider": "openai-responses",
                    "baseURL": "https://private-provider.example/v1",
                    "model": "private-model",
                    "apiKey": "sk-test-private-secret-value"
                }
            }),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("private".to_string()),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["doctor"]["modelConfig"]["status"], "configured");
        assert_eq!(
            body["doctor"]["modelConfig"]["currentProfile"]["name"],
            "private"
        );
        assert_eq!(
            body["doctor"]["modelConfig"]["currentProfile"]["provider"],
            "openai-responses"
        );
        assert_eq!(
            body["doctor"]["modelConfig"]["currentProfile"]["model"],
            "private-model"
        );
        assert_eq!(
            body["doctor"]["modelConfig"]["currentProfile"]["baseUrlPresent"],
            true
        );
        assert_eq!(
            body["doctor"]["modelConfig"]["currentProfile"]["apiKeyPresent"],
            true
        );
        let body_text = serde_json::to_string(&body).expect("serialize body");
        assert!(!body_text.contains("https://private-provider.example/v1"));
        assert!(!body_text.contains("sk-test-private-secret-value"));

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_ide_returns_readonly_mcp_ide_snapshot() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-ide-1","request":{"subtype":"slash_command","command":"/ide","args":["mcp"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "ide");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["ide"]["action"], "mcp");
                assert!(body["ide"]["connected"].is_boolean());
                assert!(body["ide"]["managerInstalled"].is_boolean());
                assert!(body["ide"]["ideServerCount"].is_u64());
                assert!(body["ide"]["connectedCount"].is_u64());
                assert!(body["ide"]["pendingCount"].is_u64());
                assert!(body["ide"]["failedCount"].is_u64());
                assert!(body["ide"]["needsAuthCount"].is_u64());
                assert!(body["ide"]["disabledCount"].is_u64());
                assert!(body["ide"]["toolCount"].is_u64());
                assert!(body["ide"]["promptCount"].is_u64());
                assert!(body["ide"]["resourceCount"].is_u64());
                assert!(body["ide"]["servers"].is_array());
                assert!(body["ide"]["diagnostics"]["pendingLspDiagnosticCount"].is_u64());
                assert_eq!(body["ide"]["diagnostics"]["contentIncluded"], false);
                assert_eq!(body["ide"]["diagnostics"]["pathsRedacted"], true);
                assert_eq!(body["ide"]["detection"]["externalScanRun"], false);
                assert_eq!(body["ide"]["detection"]["processScanRun"], false);
                assert_eq!(body["ide"]["detection"]["openCommandRun"], false);
                assert_eq!(body["ide"]["detection"]["mcpRuntimeSnapshot"], true);
                assert_eq!(body["ide"]["supportedTransports"][0], "sse-ide");
                assert_eq!(body["ide"]["supportedTransports"][1], "ws-ide");
                assert_eq!(body["ide"]["connectMutationSupported"], false);
                assert_eq!(body["ide"]["openMutationSupported"], false);
                assert_eq!(body["ide"]["rawConfigRedacted"], true);
                assert_eq!(body["ide"]["errorDetailsRedacted"], true);
                assert_eq!(body["ide"]["pathsRedacted"], true);
                assert_eq!(body["ide"]["mutationSupported"], false);
                assert!(body["ide"].get("rawConfig").is_none());
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_model_reports_and_updates_override() {
        let _guard = model_state_lock();
        let previous_override = get_main_loop_model_override();
        set_main_loop_model_override(None);
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-model-1","request":{"subtype":"slash_command","command":"/model","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "model");
                assert_eq!(body["model"]["action"], "status");
                assert_eq!(body["model"]["override"], serde_json::Value::Null);
                assert_eq!(body["model"]["mutationSupported"], true);
                assert_eq!(body["model"]["switchAppliesToNextTurn"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-model-2","request":{"subtype":"slash_command","command":"model","args":["max"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(get_main_loop_model_override().as_deref(), Some("max"));
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "model");
                assert_eq!(body["model"]["action"], "set");
                assert_eq!(body["model"]["override"], "max");
                assert_eq!(body["model"]["previousOverride"], serde_json::Value::Null);
                assert_eq!(body["model"]["recognizedAlias"], true);
                assert_eq!(body["model"]["source"], "slash_command");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-model-3","request":{"subtype":"slash_command","command":"model","args":["reset"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert_eq!(get_main_loop_model_override(), None);
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "model");
                assert_eq!(body["model"]["action"], "reset");
                assert_eq!(body["model"]["override"], serde_json::Value::Null);
                assert_eq!(body["model"]["previousOverride"], "max");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        set_main_loop_model_override(previous_override);
    }

    #[tokio::test]
    async fn slash_command_model_lists_and_switches_configured_profiles() {
        let _model_guard = model_state_lock();
        let _config_guard = config_state_lock();
        let previous_env = PROFILE_RUNTIME_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        let previous_override = get_main_loop_model_override();
        set_main_loop_model_override(None);
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "example": {
                    "provider": "openai-compatible",
                    "baseURL": "https://api.example.com/v1",
                    "model": "example-large",
                    "apiKey": "sk-test-example-secret-value"
                }
            }),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");
        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-model-profiles-1","request":{"subtype":"slash_command","command":"/model","args":["list"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "model");
                assert_eq!(body["model"]["profileCount"], 1);
                assert_eq!(body["model"]["profiles"][0]["name"], "example");
                assert_eq!(body["model"]["profiles"][0]["model"], "example-large");
                assert_eq!(body["model"]["profiles"][0]["apiKeyRedacted"], true);
                let body_text = serde_json::to_string(&body).expect("serialize body");
                assert!(!body_text.contains("sk-test-example-secret-value"));
                assert!(!body_text.contains("https://api.example.com/v1"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-model-profiles-2","request":{"subtype":"slash_command","command":"/model","args":["example"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());
        assert_eq!(
            config_profiles::get_active_profile_name().as_deref(),
            Some("example")
        );
        assert_eq!(
            get_main_loop_model_override().as_deref(),
            Some("example-large")
        );
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-compatible")
        );
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["model"]["action"], "set");
                assert_eq!(body["model"]["override"], "example-large");
                assert_eq!(body["model"]["currentProfileName"], "example");
                assert_eq!(body["model"]["profiles"][0]["active"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        mossen_agent::services::config::facade::reset_facade_for_testing();
        set_main_loop_model_override(previous_override);
        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_profile_lists_and_switches_session_profile() {
        let _guard = config_state_lock();
        let previous_env = PROFILE_RUNTIME_ENV_KEYS
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        mossen_agent::services::config::facade::reset_facade_for_testing();
        mossen_agent::services::config::facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "fast": {
                    "provider": "openai-compatible",
                    "baseURL": "https://internal.example.com/v1",
                    "model": "fast-model",
                    "apiKey": "sk-test-secret-value"
                }
            }),
            mossen_agent::services::config::types::ConfigOverrideScope::Override,
        );

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-profile-1","request":{"subtype":"slash_command","command":"/profiles","args":["list"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "profile");
                assert_eq!(body["profile"]["action"], "list");
                assert_eq!(body["profile"]["profileCount"], 1);
                assert_eq!(body["profile"]["settingsProfileCount"], 1);
                assert_eq!(body["profile"]["profiles"][0]["name"], "fast");
                assert_eq!(body["profile"]["profiles"][0]["model"], "fast-model");
                assert_eq!(body["profile"]["profiles"][0]["baseUrlPresent"], true);
                assert_eq!(body["profile"]["profiles"][0]["baseUrlRedacted"], true);
                assert_eq!(body["profile"]["profiles"][0]["apiKeyPresent"], true);
                assert_eq!(body["profile"]["profiles"][0]["apiKeyRedacted"], true);
                assert_eq!(body["profile"]["rawConfigIncluded"], false);
                assert_eq!(body["profile"]["rawConfigRedacted"], true);
                assert_eq!(body["profile"]["writesConfigFiles"], false);

                let body_text = serde_json::to_string(&body).expect("serialize body");
                assert!(!body_text.contains("sk-test-secret-value"));
                assert!(!body_text.contains("https://internal.example.com/v1"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-profile-2","request":{"subtype":"slash_command","command":"/profile","args":["use","fast"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());
        assert_eq!(
            config_profiles::get_active_profile_name().as_deref(),
            Some("fast")
        );
        assert_eq!(
            std::env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-compatible")
        );

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "profile");
                assert_eq!(body["profile"]["action"], "use");
                assert_eq!(body["profile"]["activeProfileName"], "fast");
                assert_eq!(body["profile"]["currentProfileName"], "fast");
                assert_eq!(body["profile"]["mutationPerformed"], true);
                assert_eq!(body["profile"]["mutationSupported"], true);
                assert_eq!(body["profile"]["switchAppliesToNextTurn"], true);
                assert_eq!(body["profile"]["writesConfigFiles"], false);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        mossen_agent::services::config::facade::reset_facade_for_testing();
        restore_env_vars(previous_env);
    }

    #[test]
    fn slash_command_memory_response_reports_runtime_without_content() {
        let _guard = permission_env_lock();
        let previous_env = [
            "MOSSEN_CODE_ENABLE_TEAM_MEMORY",
            "MOSSEN_TEAM_MEMORY",
            "MOSSEN_MEMORY_TEAM_MEMORY_ENABLED",
            "MOSSEN_TEAM_MEMORY_ENABLED",
        ]
        .into_iter()
        .map(|key| (key, std::env::var(key).ok()))
        .collect::<Vec<_>>();
        for (key, _) in &previous_env {
            std::env::remove_var(key);
        }

        let body = slash_memory_response();

        assert_eq!(body["command"], "memory");
        assert_eq!(body["memory"]["contentRedacted"], true);
        assert_eq!(body["memory"]["memoryFilesAttached"], false);
        assert!(body["memory"]["runtime"]["autoMemoryEnabled"].is_boolean());
        assert!(body["memory"]["runtime"]["extractModeActive"].is_boolean());
        assert_eq!(
            body["memory"]["runtime"].get("teamMemory"),
            None,
            "personal /memory surface must not expose team-memory controls"
        );
        assert_eq!(
            body["memory"]["runtime"]["sessionMemory"]["contentIncluded"],
            false
        );
        assert!(body["memory"]["runtime"]["compact"]["autoCompactEnabled"].is_boolean());
        assert_eq!(body["memory"]["runtime"]["files"]["contentIncluded"], false);

        let serialized = serde_json::to_string(&body).expect("serialize memory response");
        assert!(!serialized.contains("readFile"));
        assert!(!serialized.contains("apiKey"));
        assert!(!serialized.contains("token"));
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("teamMemory"));

        restore_env_vars(previous_env);
    }

    #[tokio::test]
    async fn slash_command_doctor_reports_memory_and_compact_checks() {
        let io = StructuredIO::new(false);
        let body = io
            .slash_doctor_response(&[])
            .await
            .expect("doctor response");

        assert_eq!(body["command"], "doctor");
        assert!(body["doctor"]["memory"]["autoMemoryEnabled"].is_boolean());
        assert_eq!(
            body["doctor"]["memory"].get("teamMemory"),
            None,
            "personal /doctor surface must not expose team-memory controls"
        );
        assert_eq!(
            body["doctor"]["memory"]["sessionMemory"]["contentIncluded"],
            false
        );
        assert!(body["doctor"]["compact"]["autoCompactEnabled"].is_boolean());
        assert_eq!(body["doctor"]["compact"]["slashBridge"], true);
        assert_eq!(body["doctor"]["compact"]["contentIncluded"], false);
    }

    #[tokio::test]
    async fn slash_command_skills_lists_available_inventory_redacted() {
        static SKILL_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let _guard = SKILL_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("skill inventory lock");
        mossen_skills::clear_dynamic_skills();
        mossen_skills::init_bundled_skills();

        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("m6_inventory_skill");
        tokio::fs::create_dir_all(&skill_dir)
            .await
            .expect("create skill dir");
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: M6 inventory skill\n---\nM6_INVENTORY_SECRET_BODY\n",
        )
        .await
        .expect("write skill");
        let added = mossen_skills::add_skill_directories(&[temp.path().to_path_buf()]).await;
        assert_eq!(added, 1);

        let body = slash_skills_response();

        assert_eq!(body["command"], "skills");
        assert!(
            body["skills"]["availableCount"]
                .as_u64()
                .unwrap_or_default()
                >= 1,
            "{body}"
        );
        let available = body["skills"]["available"]
            .as_array()
            .expect("available array");
        assert!(available
            .iter()
            .any(|skill| skill["name"] == "m6_inventory_skill"));
        assert!(available.iter().any(|skill| skill["source"] == "bundled"));
        assert_eq!(body["skills"]["contentRedacted"], true);
        assert_eq!(body["skills"]["pathsRedacted"], true);
        assert_eq!(body["skills"]["rawSkillRootsIncluded"], false);

        let serialized = serde_json::to_string(&body).expect("serialize skills response");
        assert!(!serialized.contains("M6_INVENTORY_SECRET_BODY"));
        mossen_skills::clear_dynamic_skills();
    }

    #[tokio::test]
    async fn slash_command_init_returns_agent_prompt_handoff_without_direct_write() {
        let _guard = process_cwd_lock();
        let previous_cwd = std::env::current_dir().expect("cwd");
        let temp = tempfile::tempdir().expect("tempdir");
        std::env::set_current_dir(temp.path()).expect("chdir temp");

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-init-1","request":{"subtype":"slash_command","command":"/init","args":["preview"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "init");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["init"]["action"], "preview");
                assert_eq!(body["init"]["handoffType"], "agent_prompt");
                assert_eq!(body["init"]["promptSubtype"], "project_memory_init");
                assert_eq!(body["init"]["promptIncluded"], false);
                assert_eq!(body["init"]["promptText"], serde_json::Value::Null);
                assert!(body["init"]["promptPreview"]
                    .as_str()
                    .unwrap_or("")
                    .contains("MOSSEN.md"));
                assert!(body["init"]["promptBytes"].as_u64().unwrap_or(0) > 100);
                assert_eq!(body["init"]["modelTurnRequired"], true);
                assert_eq!(body["init"]["agentMayWriteFiles"], true);
                assert_eq!(body["init"]["writesFilesDirectly"], false);
                assert_eq!(body["init"]["usesNormalToolPermissions"], true);
                assert_eq!(body["init"]["requiresToolApprovalForWrites"], true);
                assert_eq!(body["init"]["existingFiles"]["projectMossenMd"], false);
                assert_eq!(body["init"]["existingFiles"]["localMossenMd"], false);
                assert_eq!(body["init"]["existingFiles"]["mossenDirectory"], false);
                assert_eq!(body["init"]["rawPathsIncluded"], false);
                assert_eq!(body["init"]["pathsRedacted"], true);
                assert_eq!(body["init"]["rawPromptConfigIncluded"], false);
                assert_eq!(body["init"]["mutationSupported"], true);
                assert!(!temp.path().join("MOSSEN.md").exists());
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-init-2","request":{"subtype":"slash_command","command":"/init","args":["run"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "init");
                assert_eq!(body["init"]["action"], "run");
                assert_eq!(body["init"]["promptIncluded"], true);
                assert!(body["init"]["promptText"]
                    .as_str()
                    .unwrap_or("")
                    .contains("Please analyze this codebase"));
                assert_eq!(body["init"]["writesFilesDirectly"], false);
                assert!(!temp.path().join("MOSSEN.md").exists());
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        std::env::set_current_dir(previous_cwd).expect("restore cwd");
    }

    #[tokio::test]
    async fn slash_command_auth_returns_redacted_backend_credential_status() {
        let _guard = auth_env_lock();
        let keys = vec![
            "MOSSEN_CODE_AUTH_TOKEN",
            "MOSSEN_CODE_AUTH_TOKEN_FILE_DESCRIPTOR",
            "MOSSEN_CODE_CUSTOM_BACKEND_URL",
            "MOSSEN_CODE_CUSTOM_BACKEND_AUTH_TOKEN",
            "MOSSEN_CODE_CUSTOM_BACKEND_API_KEY",
            "MOSSEN_CODE_BARE",
        ];
        let previous = keys
            .iter()
            .map(|key| (*key, std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for key in &keys {
            std::env::remove_var(key);
        }
        std::env::set_var("MOSSEN_CODE_AUTH_TOKEN", "secret-auth-token-value");

        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-login-1","request":{"subtype":"slash_command","command":"/login","args":["run"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "login");
                assert_eq!(body["auth"]["action"], "run");
                assert_eq!(body["auth"]["status"], "authenticated");
                assert_eq!(body["auth"]["authTokenSource"], "env_auth_token");
                assert_eq!(body["auth"]["authTokenPresent"], true);
                assert_eq!(body["auth"]["credentialMode"], "personal_backend");
                assert_eq!(body["auth"]["handoffType"], "backend_credential_setup");
                assert_eq!(body["auth"]["handoffRequired"], false);
                assert_eq!(body["auth"]["equivalentCliCommand"], "mossen auth");
                assert_eq!(body["auth"]["requiresExternalInteractiveCli"], false);
                assert_eq!(body["auth"]["mutationSupported"], false);
                assert_eq!(body["auth"]["mutationPerformed"], false);
                assert_eq!(body["auth"]["writesAuthStateDirectly"], false);
                assert_eq!(body["auth"]["tokensRedacted"], true);
                assert_eq!(body["auth"]["apiKeyRedacted"], true);
                assert_eq!(body["auth"]["rawEnvValuesIncluded"], false);
                assert!(!body.to_string().contains("secret-auth-token-value"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-logout-1","request":{"subtype":"slash_command","command":"/logout","args":["--confirm"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "logout");
                assert_eq!(body["auth"]["action"], "confirm");
                assert_eq!(body["auth"]["confirmationReceived"], true);
                assert_eq!(body["auth"]["requiresConfirmation"], true);
                assert_eq!(body["auth"]["credentialMode"], "personal_backend");
                assert_eq!(body["auth"]["handoffType"], "credential_status");
                assert_eq!(body["auth"]["handoffRequired"], false);
                assert_eq!(body["auth"]["equivalentCliCommand"], "mossen deauth");
                assert_eq!(body["auth"]["requiresExternalInteractiveCli"], false);
                assert_eq!(body["auth"]["mutationSupported"], false);
                assert_eq!(body["auth"]["mutationPerformed"], false);
                assert_eq!(body["auth"]["writesAuthStateDirectly"], false);
                assert_eq!(body["auth"]["tokensRedacted"], true);
                assert!(!body.to_string().contains("secret-auth-token-value"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        restore_env_vars(previous);
    }

    #[tokio::test]
    async fn slash_command_mcp_inventory_returns_redacted_snapshot() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-mcp-1","request":{"subtype":"slash_command","command":"/mcp","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "mcp");
                assert_eq!(body["status"], "completed");
                assert!(body["mcp"]["managerInstalled"].is_boolean());
                assert!(body["mcp"]["servers"].is_array());
                assert_eq!(body["mcp"]["rawConfigRedacted"], true);
                assert_eq!(body["mcp"]["toolSchemasRedacted"], true);
                assert_eq!(body["mcp"]["instructionsRedacted"], true);
                assert_eq!(body["mcp"]["errorDetailsRedacted"], true);
                assert_eq!(body["mcp"]["mutationSupported"], false);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-mcp-2","request":{"subtype":"slash_command","command":"mcp","args":["add"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "error");
                assert!(response
                    .response
                    .error
                    .expect("error")
                    .contains("unsupported_slash_command_args: mcp"));
            }
            other => panic!("expected slash command error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_readonly_inventory_commands_return_safe_snapshots() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        for (request_id, command, payload_key) in [
            ("slash-cost-1", "cost", "cost"),
            ("slash-hooks-1", "hooks", "hooks"),
            ("slash-skills-1", "skills", "skills"),
            ("slash-plugin-1", "plugin", "plugins"),
        ] {
            let line = format!(
                r#"{{"type":"control_request","request_id":"{request_id}","request":{{"subtype":"slash_command","command":"{command}"}}}}"#
            );
            let returned = io.process_line(&line).await.expect("process line");
            assert!(returned.is_none());

            let response = outbound.recv().await.expect("control response");
            match response {
                StdoutMessage::ControlResponse(response) => {
                    assert_eq!(response.response.subtype, "slash_command_result");
                    let body = response.response.response.expect("response body");
                    assert_eq!(body["command"], command);
                    assert_eq!(body["status"], "completed");
                    assert!(body[payload_key].is_object());
                    if command == "plugin" {
                        assert_eq!(body["plugins"]["inlinePluginNamesIncluded"], false);
                        assert!(body["plugins"].get("inlinePlugins").is_none());
                        assert_eq!(body["plugins"]["rawConfigRedacted"], true);
                        assert_eq!(body["plugins"]["installMutationSupported"], false);
                    }
                }
                other => panic!("expected slash command response, got {other:?}"),
            }
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-plugin-1","request":{"subtype":"slash_command","command":"plugin","args":["install"]}}"#,
            )
            .await
            .expect("process line");
        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "error");
                assert!(response
                    .response
                    .error
                    .expect("error")
                    .contains("unsupported_slash_command_args: plugin"));
            }
            other => panic!("expected slash command error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn slash_command_compact_preview_enqueues_dry_run_request() {
        let _guard = pending_compact_test_guard();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-compact-1","request":{"subtype":"slash_command","command":"compact","args":["plan","preserve","permission","decisions"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let pending = get_pending_compact_request().expect("pending compact request");
        assert_eq!(pending.request_id, "slash-compact-1");
        assert!(pending.dry_run);
        assert_eq!(
            pending.custom_instructions.as_deref(),
            Some("preserve permission decisions")
        );

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "compact");
                assert_eq!(body["status"], "queued");
                assert_eq!(body["compact"]["action"], "preview");
                assert_eq!(body["compact"]["requested_action"], "plan");
                assert_eq!(body["compact"]["dry_run"], true);
                assert_eq!(body["compact"]["confirmation"]["provided"], false);
                assert_eq!(
                    body["compact"]["confirmation"]["confirm_command"],
                    "/compact run --confirm"
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["kind"],
                    "compact_preview"
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["will_mutate_history"],
                    false
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["safe_point"],
                    "dialogue_safe_point"
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["expected_status_event"],
                    "compact_request_status"
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["followup_command"],
                    "/compact run --confirm"
                );
                assert!(body["compact"]["available_actions"]
                    .as_array()
                    .expect("compact actions")
                    .iter()
                    .any(|action| action["id"] == "run"
                        && action["requires_confirm"] == true
                        && action["mutates_history"] == true));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn slash_command_compact_run_requires_confirm() {
        let _guard = pending_compact_test_guard();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-compact-run","request":{"subtype":"slash_command","command":"compact","args":["run"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert!(get_pending_compact_request().is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "compact");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["compact"]["action"], "run");
                assert_eq!(body["compact"]["pending"], false);
                assert_eq!(body["compact"]["requires_confirm"], true);
                assert_eq!(body["compact"]["run_requires_confirm"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn slash_command_compact_run_confirm_enqueues_real_request() {
        let _guard = pending_compact_test_guard();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-compact-confirm","request":{"subtype":"slash_command","command":"compact","args":["run","--confirm","keep","approval","history"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let pending = get_pending_compact_request().expect("pending compact request");
        assert_eq!(pending.request_id, "slash-compact-confirm");
        assert!(!pending.dry_run);
        assert_eq!(
            pending.custom_instructions.as_deref(),
            Some("keep approval history")
        );

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "compact");
                assert_eq!(body["status"], "queued");
                assert_eq!(body["compact"]["action"], "run");
                assert_eq!(body["compact"]["dry_run"], false);
                assert_eq!(body["compact"]["pending"], true);
                assert_eq!(body["compact"]["requires_confirm"], false);
                assert_eq!(body["compact"]["run_requires_confirm"], true);
                assert_eq!(body["compact"]["confirmation"]["provided"], true);
                assert_eq!(
                    body["compact"]["compact_preview"]["will_mutate_history"],
                    true
                );
                assert_eq!(
                    body["compact"]["compact_preview"]["history_boundary"],
                    "compact_boundary_on_safe_point"
                );
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn slash_command_compact_cancel_clears_pending_request() {
        let _guard = pending_compact_test_guard();
        enqueue_pending_compact_request(
            "compact-cancel-target".to_string(),
            CompactMode::Manual,
            true,
            Some("keep approval history".to_string()),
        )
        .expect("enqueue compact request");
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-compact-cancel","request":{"subtype":"slash_command","command":"compact","args":["cancel"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert!(get_pending_compact_request().is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "compact");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["compact"]["action"], "cancel");
                assert_eq!(body["compact"]["cancelled"], true);
                assert_eq!(body["compact"]["pending"], false);
                assert_eq!(body["compact"]["request_id"], "compact-cancel-target");
                assert_eq!(body["compact"]["dry_run"], true);
                assert_eq!(body["compact"]["had_custom_instructions"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn slash_command_clear_preview_and_confirm_queue_request() {
        clear_pending_clear_request();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-clear-preview","request":{"subtype":"slash_command","command":"clear","args":[]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert!(get_pending_clear_request().is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "clear");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["clear"]["action"], "preview");
                assert_eq!(body["clear"]["requires_confirm"], true);
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-clear-confirm","request":{"subtype":"slash_command","command":"clear","args":["--confirm"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let pending = get_pending_clear_request().expect("pending clear request");
        assert_eq!(pending.request_id, "slash-clear-confirm");
        assert!(!pending.dry_run);

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "clear");
                assert_eq!(body["status"], "queued");
                assert_eq!(body["clear"]["action"], "run");
                assert_eq!(body["clear"]["dry_run"], false);
                assert_eq!(body["clear"]["execution_stage"], "dialogue_safe_point");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-clear-status","request":{"subtype":"slash_command","command":"clear","args":["status"]}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.subtype, "slash_command_result");
                let body = response.response.response.expect("response body");
                assert_eq!(body["command"], "clear");
                assert_eq!(body["status"], "completed");
                assert_eq!(body["clear"]["pending"], true);
                assert_eq!(body["clear"]["request_id"], "slash-clear-confirm");
            }
            other => panic!("expected slash command response, got {other:?}"),
        }

        clear_pending_clear_request();
    }

    #[tokio::test]
    async fn slash_command_unknown_returns_error_response() {
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"slash-unknown-1","request":{"subtype":"slash_command","command":"/not-real"}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "slash-unknown-1");
                assert_eq!(response.response.subtype, "error");
                assert!(response
                    .response
                    .error
                    .expect("error")
                    .contains("unsupported_slash_command"));
            }
            other => panic!("expected slash command response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compact_conversation_control_request_enqueues_and_responds() {
        let _guard = pending_compact_test_guard();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"compact-1","request":{"subtype":"compact_conversation","mode":"manual","dry_run":true,"custom_instructions":"keep decisions"}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        let pending = get_pending_compact_request().expect("pending compact request");
        assert_eq!(pending.request_id, "compact-1");
        assert!(pending.dry_run);
        assert_eq!(
            pending.custom_instructions.as_deref(),
            Some("keep decisions")
        );

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                assert_eq!(response.response.request_id, "compact-1");
                assert_eq!(response.response.subtype, "compact_conversation");
                let body = response.response.response.expect("response body");
                assert_eq!(body["status"], "queued");
                assert_eq!(body["dry_run"], true);
            }
            other => panic!("expected compact control_response, got {other:?}"),
        }

        clear_pending_compact_request();
    }

    #[tokio::test]
    async fn compact_conversation_control_request_blocks_unsupported_mode() {
        let _guard = pending_compact_test_guard();
        let io = StructuredIO::new(false);
        let mut outbound = io.take_outbound_rx().await.expect("outbound receiver");

        let returned = io
            .process_line(
                r#"{"type":"control_request","request_id":"compact-2","request":{"subtype":"compact_conversation","mode":"auto"}}"#,
            )
            .await
            .expect("process line");

        assert!(returned.is_none());
        assert!(get_pending_compact_request().is_none());

        let response = outbound.recv().await.expect("control response");
        match response {
            StdoutMessage::ControlResponse(response) => {
                let body = response.response.response.expect("response body");
                assert_eq!(body["status"], "blocked");
                assert!(body["reason"]
                    .as_str()
                    .expect("reason")
                    .contains("unsupported compact mode"));
            }
            other => panic!("expected compact control_response, got {other:?}"),
        }
    }
}
