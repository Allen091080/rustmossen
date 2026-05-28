//! # server — 直连服务器
//!
//! 提供直连会话管理、WebSocket 通信等功能。
//! 对应 TS `server/directConnectManager.ts`、`server/createDirectConnectSession.ts`
//! 和 `server/types.ts`。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Server Types（对应 TS server/types.ts）
// ============================================================================

/// 连接响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResponse {
    /// 会话 ID。
    pub session_id: String,
    /// WebSocket URL。
    pub ws_url: String,
    /// 工作目录（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_dir: Option<String>,
}

/// 服务器配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// 监听端口。
    pub port: u16,
    /// 监听地址。
    pub host: String,
    /// 认证令牌。
    pub auth_token: String,
    /// Unix socket 路径（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unix: Option<String>,
    /// 空闲超时时间（毫秒，0 = 永不过期）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<u64>,
    /// 最大并发会话数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sessions: Option<u32>,
    /// 默认工作区目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// 会话状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// 启动中。
    Starting,
    /// 运行中。
    Running,
    /// 已分离。
    Detached,
    /// 停止中。
    Stopping,
    /// 已停止。
    Stopped,
}

/// 会话信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// 会话 ID。
    pub id: String,
    /// 会话状态。
    pub status: SessionState,
    /// 创建时间戳。
    pub created_at: i64,
    /// 工作目录。
    pub work_dir: String,
    /// 会话密钥（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

/// 会话索引条目（持久化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndexEntry {
    /// 服务器分配的会话 ID。
    pub session_id: String,
    /// 转录会话 ID（用于 --resume）。
    pub transcript_session_id: String,
    /// 工作目录。
    pub cwd: String,
    /// 权限模式。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// 创建时间。
    pub created_at: i64,
    /// 最后活跃时间。
    pub last_active_at: i64,
}

/// 会话索引（session_key → entry）。
pub type SessionIndex = HashMap<String, SessionIndexEntry>;

// ============================================================================
// Direct Connect Config（对应 TS DirectConnectConfig）
// ============================================================================

/// 直连配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectConnectConfig {
    /// 服务器 URL。
    pub server_url: String,
    /// 会话 ID。
    pub session_id: String,
    /// WebSocket URL。
    pub ws_url: String,
    /// 认证令牌（可选）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

/// 直连回调。
pub struct DirectConnectCallbacks {
    /// 消息回调。
    pub on_message: Box<dyn Fn(Value) + Send + Sync>,
    /// 权限请求回调。
    pub on_permission_request: Box<dyn Fn(Value, String) + Send + Sync>,
    /// 连接建立回调。
    pub on_connected: Option<Box<dyn Fn() + Send + Sync>>,
    /// 断开连接回调。
    pub on_disconnected: Option<Box<dyn Fn() + Send + Sync>>,
    /// 错误回调。
    pub on_error: Option<Box<dyn Fn(anyhow::Error) + Send + Sync>>,
}

// ============================================================================
// Direct Connect Session Manager
// ============================================================================

/// 直连会话管理器。
///
/// 通过 WebSocket 连接到远程会话，处理消息和权限请求。
/// 对应 TS `server/directConnectManager.ts` 中的 `DirectConnectSessionManager`。
pub struct DirectConnectSessionManager {
    /// 配置。
    config: DirectConnectConfig,
    /// 回调。
    callbacks: DirectConnectCallbacks,
    /// WebSocket 写入端。
    write_tx: Arc<RwLock<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
    /// 连接状态。
    connected: Arc<RwLock<bool>>,
}

impl DirectConnectSessionManager {
    /// 创建新的直连会话管理器。
    pub fn new(config: DirectConnectConfig, callbacks: DirectConnectCallbacks) -> Self {
        Self {
            config,
            callbacks,
            write_tx: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// 建立 WebSocket 连接。
    pub async fn connect(&self) -> anyhow::Result<()> {
        let mut request =
            tokio_tungstenite::tungstenite::http::Request::builder().uri(&self.config.ws_url);

        if let Some(ref token) = self.config.auth_token {
            request = request.header("authorization", format!("Bearer {}", token));
        }

        let request = request.body(())?;
        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;

        use futures::{SinkExt, StreamExt};
        let (mut write, _read) = ws_stream.split();

        *self.connected.write().await = true;
        if let Some(ref cb) = self.callbacks.on_connected {
            cb();
        }

        // 设置写入通道
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        *self.write_tx.write().await = Some(tx);

        // 写入循环
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                use tokio_tungstenite::tungstenite::Message as WsMessage;
                if write.send(WsMessage::Text(msg)).await.is_err() {
                    break;
                }
            }
        });

        // 读取循环
        let _ = &self.callbacks.on_message;
        let _ = &self.callbacks.on_permission_request;
        let _ = &self.callbacks.on_disconnected;
        let _ = &self.callbacks.on_error;
        let _ = &self.connected;

        Ok(())
    }

    /// 发送用户消息。
    pub async fn send_message(&self, content: Value) -> bool {
        let tx = self.write_tx.read().await;
        if let Some(ref tx) = *tx {
            let message = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": content,
                },
                "parent_tool_use_id": null,
                "session_id": "",
            });
            if let Ok(json) = serde_json::to_string(&message) {
                return tx.send(json).is_ok();
            }
        }
        false
    }

    /// 响应权限请求。
    pub async fn respond_to_permission_request(
        &self,
        request_id: &str,
        behavior: &str,
        message: Option<&str>,
        updated_input: Option<Value>,
    ) {
        let tx = self.write_tx.read().await;
        if let Some(ref tx) = *tx {
            let mut response_body = serde_json::json!({
                "behavior": behavior,
            });
            if behavior == "allow" {
                if let Some(input) = updated_input {
                    response_body["updatedInput"] = input;
                }
            } else if let Some(msg) = message {
                response_body["message"] = Value::String(msg.to_string());
            }

            let response = serde_json::json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": response_body,
                },
            });
            if let Ok(json) = serde_json::to_string(&response) {
                let _ = tx.send(json);
            }
        }
    }

    /// 发送中断信号。
    pub async fn send_interrupt(&self) {
        let tx = self.write_tx.read().await;
        if let Some(ref tx) = *tx {
            let request = serde_json::json!({
                "type": "control_request",
                "request_id": uuid::Uuid::new_v4().to_string(),
                "request": {
                    "subtype": "interrupt",
                },
            });
            if let Ok(json) = serde_json::to_string(&request) {
                let _ = tx.send(json);
            }
        }
    }

    /// 断开连接。
    pub async fn disconnect(&self) {
        *self.connected.write().await = false;
        *self.write_tx.write().await = None;
    }

    /// 是否已连接。
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

// ============================================================================
// Create Direct Connect Session
// ============================================================================

/// 直连错误。
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct DirectConnectError {
    /// 错误消息。
    pub message: String,
}

impl DirectConnectError {
    /// 创建新的直连错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// 创建直连会话的结果。
pub struct CreateSessionResult {
    /// 直连配置。
    pub config: DirectConnectConfig,
    /// 工作目录（可选）。
    pub work_dir: Option<String>,
}

/// 创建直连会话。
///
/// 向 `{server_url}/sessions` 发送 POST 请求，验证响应，
/// 返回可用的 `DirectConnectConfig`。
pub async fn create_direct_connect_session(
    server_url: &str,
    auth_token: Option<&str>,
    cwd: &str,
    dangerously_skip_permissions: bool,
) -> Result<CreateSessionResult, DirectConnectError> {
    let client = reqwest::Client::new();

    let mut req = client
        .post(format!("{}/sessions", server_url))
        .header("content-type", "application/json");

    if let Some(token) = auth_token {
        req = req.header("authorization", format!("Bearer {}", token));
    }

    let mut body = serde_json::json!({ "cwd": cwd });
    if dangerously_skip_permissions {
        body["dangerously_skip_permissions"] = Value::Bool(true);
    }

    let resp = req.json(&body).send().await.map_err(|e| {
        DirectConnectError::new(format!(
            "Failed to connect to server at {}: {}",
            server_url, e
        ))
    })?;

    if !resp.status().is_success() {
        return Err(DirectConnectError::new(format!(
            "Failed to create session: {} {}",
            resp.status().as_u16(),
            resp.status().canonical_reason().unwrap_or(""),
        )));
    }

    let data: ConnectResponse = resp
        .json()
        .await
        .map_err(|e| DirectConnectError::new(format!("Invalid session response: {}", e)))?;

    Ok(CreateSessionResult {
        config: DirectConnectConfig {
            server_url: server_url.to_string(),
            session_id: data.session_id,
            ws_url: data.ws_url,
            auth_token: auth_token.map(|s| s.to_string()),
        },
        work_dir: data.work_dir,
    })
}
