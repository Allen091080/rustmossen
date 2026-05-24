// server.rs — Translation of server/ directory:
// server/types.ts, server/createDirectConnectSession.ts, server/directConnectManager.ts

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// types.ts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResponse {
    pub session_id: String,
    pub ws_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub auth_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_sessions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Starting,
    Running,
    Detached,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub status: SessionState,
    pub created_at: i64,
    pub work_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndexEntry {
    pub session_id: String,
    pub transcript_session_id: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    pub created_at: i64,
    pub last_active_at: i64,
}

pub type SessionIndex = std::collections::HashMap<String, SessionIndexEntry>;

// ============================================================================
// createDirectConnectSession.ts
// ============================================================================

#[derive(Debug)]
pub struct DirectConnectError {
    pub message: String,
}

impl std::fmt::Display for DirectConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DirectConnectError: {}", self.message)
    }
}

impl std::error::Error for DirectConnectError {}

#[derive(Debug, Clone)]
pub struct CreateSessionParams {
    pub server_url: String,
    pub auth_token: Option<String>,
    pub cwd: String,
    pub dangerously_skip_permissions: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct CreateSessionResult {
    pub config: DirectConnectConfig,
    pub work_dir: Option<String>,
}

pub async fn create_direct_connect_session(
    params: CreateSessionParams,
) -> Result<CreateSessionResult, DirectConnectError> {
    let client = reqwest::Client::new();
    let url = format!("{}/sessions", params.server_url);

    let mut body = serde_json::json!({ "cwd": params.cwd });
    if params.dangerously_skip_permissions == Some(true) {
        body["dangerously_skip_permissions"] = serde_json::Value::Bool(true);
    }

    let mut request = client.post(&url).json(&body);
    if let Some(ref token) = params.auth_token {
        request = request.header("authorization", format!("Bearer {}", token));
    }

    let resp = request.send().await.map_err(|e| DirectConnectError {
        message: format!(
            "Failed to connect to server at {}: {}",
            params.server_url, e
        ),
    })?;

    if !resp.status().is_success() {
        return Err(DirectConnectError {
            message: format!(
                "Failed to create session: {} {}",
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or("")
            ),
        });
    }

    let data: ConnectResponse = resp.json().await.map_err(|e| DirectConnectError {
        message: format!("Invalid session response: {}", e),
    })?;

    Ok(CreateSessionResult {
        config: DirectConnectConfig {
            server_url: params.server_url,
            session_id: data.session_id,
            ws_url: data.ws_url,
            auth_token: params.auth_token,
        },
        work_dir: data.work_dir,
    })
}

// ============================================================================
// directConnectManager.ts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectConnectConfig {
    pub server_url: String,
    pub session_id: String,
    pub ws_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StdoutMessage {
    #[serde(rename = "control_request")]
    ControlRequest {
        request_id: String,
        request: serde_json::Value,
    },
    #[serde(rename = "control_response")]
    ControlResponse(serde_json::Value),
    #[serde(rename = "keep_alive")]
    KeepAlive,
    #[serde(other)]
    Other,
}

pub struct DirectConnectCallbacks {
    pub on_message: Box<dyn Fn(serde_json::Value) + Send + Sync>,
    pub on_permission_request: Box<dyn Fn(serde_json::Value, String) + Send + Sync>,
    pub on_connected: Option<Box<dyn Fn() + Send + Sync>>,
    pub on_disconnected: Option<Box<dyn Fn() + Send + Sync>>,
    pub on_error: Option<Box<dyn Fn(String) + Send + Sync>>,
}

pub struct DirectConnectSessionManager {
    config: DirectConnectConfig,
    callbacks: Arc<DirectConnectCallbacks>,
    ws_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl DirectConnectSessionManager {
    pub fn new(config: DirectConnectConfig, callbacks: DirectConnectCallbacks) -> Self {
        Self {
            config,
            callbacks: Arc::new(callbacks),
            ws_sender: Arc::new(Mutex::new(None)),
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub async fn connect(&self) {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut request = self
            .config
            .ws_url
            .clone()
            .into_client_request()
            .unwrap_or_else(|_| {
                use tokio_tungstenite::tungstenite::http;
                http::Request::builder()
                    .uri(&self.config.ws_url)
                    .body(())
                    .unwrap()
            });
        if let Some(ref token) = self.config.auth_token {
            request.headers_mut().insert(
                "authorization",
                format!("Bearer {}", token).parse().unwrap(),
            );
        }

        let ws_result = tokio_tungstenite::connect_async(request).await;
        match ws_result {
            Ok((ws_stream, _)) => {
                use futures_util::{SinkExt, StreamExt};
                let (mut write, mut read) = ws_stream.split();
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

                {
                    let mut sender = self.ws_sender.lock().await;
                    *sender = Some(tx);
                }
                self.connected
                    .store(true, std::sync::atomic::Ordering::SeqCst);

                if let Some(ref on_connected) = self.callbacks.on_connected {
                    on_connected();
                }

                let callbacks = self.callbacks.clone();
                let connected = self.connected.clone();

                // Read task
                let read_handle = tokio::spawn(async move {
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                for line in text.split('\n').filter(|l| !l.trim().is_empty()) {
                                    if let Ok(raw) = serde_json::from_str::<serde_json::Value>(line)
                                    {
                                        let msg_type =
                                            raw.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                        match msg_type {
                                            "control_request" => {
                                                let request_id = raw
                                                    .get("request_id")
                                                    .and_then(|r| r.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let request =
                                                    raw.get("request").cloned().unwrap_or_default();
                                                let subtype = request
                                                    .get("subtype")
                                                    .and_then(|s| s.as_str())
                                                    .unwrap_or("");
                                                if subtype == "can_use_tool" {
                                                    (callbacks.on_permission_request)(
                                                        request, request_id,
                                                    );
                                                }
                                            }
                                            "control_response"
                                            | "keep_alive"
                                            | "control_cancel_request" => {}
                                            _ => {
                                                (callbacks.on_message)(raw);
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                            Err(_) => break,
                            _ => {}
                        }
                    }
                    connected.store(false, std::sync::atomic::Ordering::SeqCst);
                    if let Some(ref on_disconnected) = callbacks.on_disconnected {
                        on_disconnected();
                    }
                });

                // Write task
                tokio::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        if write
                            .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                let _ = read_handle.await;
            }
            Err(e) => {
                if let Some(ref on_error) = self.callbacks.on_error {
                    on_error(format!("WebSocket connection error: {}", e));
                }
            }
        }
    }

    pub async fn send_message(&self, content: serde_json::Value) -> bool {
        let sender = self.ws_sender.lock().await;
        if let Some(ref tx) = *sender {
            let message = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": content,
                },
                "parent_tool_use_id": null,
                "session_id": "",
            });
            tx.send(serde_json::to_string(&message).unwrap_or_default())
                .is_ok()
        } else {
            false
        }
    }

    pub async fn respond_to_permission_request(
        &self,
        request_id: &str,
        behavior: &str,
        updated_input: Option<serde_json::Value>,
        message: Option<&str>,
    ) {
        let sender = self.ws_sender.lock().await;
        if let Some(ref tx) = *sender {
            let mut response_inner = serde_json::json!({
                "behavior": behavior,
            });
            if behavior == "allow" {
                if let Some(input) = updated_input {
                    response_inner["updatedInput"] = input;
                }
            } else if let Some(msg) = message {
                response_inner["message"] = serde_json::Value::String(msg.to_string());
            }
            let response = serde_json::json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": response_inner,
                },
            });
            let _ = tx.send(serde_json::to_string(&response).unwrap_or_default());
        }
    }

    pub async fn send_interrupt(&self) {
        let sender = self.ws_sender.lock().await;
        if let Some(ref tx) = *sender {
            let request = serde_json::json!({
                "type": "control_request",
                "request_id": uuid::Uuid::new_v4().to_string(),
                "request": {
                    "subtype": "interrupt",
                },
            });
            let _ = tx.send(serde_json::to_string(&request).unwrap_or_default());
        }
    }

    pub async fn disconnect(&self) {
        let mut sender = self.ws_sender.lock().await;
        *sender = None;
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }
}
