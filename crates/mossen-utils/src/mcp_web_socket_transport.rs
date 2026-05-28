use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// JSON-RPC message (simplified representation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

/// WebSocket ready state constants.
pub const WS_CONNECTING: u8 = 0;
pub const WS_OPEN: u8 = 1;

/// Trait for WebSocket-like connections.
#[async_trait::async_trait]
pub trait WebSocketLike: Send + Sync {
    fn ready_state(&self) -> u8;
    fn close(&self);
    async fn send_text(&self, data: &str) -> Result<(), WebSocketTransportError>;
}

/// Error type for WebSocket transport.
#[derive(Debug, thiserror::Error)]
pub enum WebSocketTransportError {
    #[error("Start can only be called once per transport.")]
    AlreadyStarted,
    #[error("WebSocket is not open. Cannot start transport.")]
    NotOpen,
    #[error("WebSocket is not open. Cannot send message.")]
    SendNotOpen,
    #[error("WebSocket error: {0}")]
    WebSocketError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Callback types for the transport.
pub type OnCloseCallback = Box<dyn Fn() + Send + Sync>;
pub type OnErrorCallback = Box<dyn Fn(WebSocketTransportError) + Send + Sync>;
pub type OnMessageCallback = Box<dyn Fn(JsonRpcMessage) + Send + Sync>;

/// WebSocket transport for MCP protocol.
pub struct WebSocketTransport {
    ws: Arc<dyn WebSocketLike>,
    started: Mutex<bool>,
    pub on_close: Mutex<Option<OnCloseCallback>>,
    pub on_error: Mutex<Option<OnErrorCallback>>,
    pub on_message: Mutex<Option<OnMessageCallback>>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport.
    pub fn new(ws: Arc<dyn WebSocketLike>) -> Self {
        Self {
            ws,
            started: Mutex::new(false),
            on_close: Mutex::new(None),
            on_error: Mutex::new(None),
            on_message: Mutex::new(None),
        }
    }

    /// Handle incoming raw message data.
    pub async fn handle_incoming_message(&self, data: &str) {
        match serde_json::from_str::<JsonRpcMessage>(data) {
            Ok(message) => {
                if let Some(cb) = self.on_message.lock().await.as_ref() {
                    cb(message);
                }
            }
            Err(e) => {
                self.handle_error(WebSocketTransportError::ParseError(e.to_string()))
                    .await;
            }
        }
    }

    /// Handle errors.
    pub async fn handle_error(&self, error: WebSocketTransportError) {
        if let Some(cb) = self.on_error.lock().await.as_ref() {
            cb(error);
        }
    }

    /// Handle close events.
    pub async fn handle_close(&self) {
        if let Some(cb) = self.on_close.lock().await.as_ref() {
            cb();
        }
    }

    /// Start listening for messages on the WebSocket.
    pub async fn start(&self) -> Result<(), WebSocketTransportError> {
        let mut started = self.started.lock().await;
        if *started {
            return Err(WebSocketTransportError::AlreadyStarted);
        }
        if self.ws.ready_state() != WS_OPEN {
            return Err(WebSocketTransportError::NotOpen);
        }
        *started = true;
        Ok(())
    }

    /// Close the WebSocket connection.
    pub async fn close(&self) {
        let state = self.ws.ready_state();
        if state == WS_OPEN || state == WS_CONNECTING {
            self.ws.close();
        }
        self.handle_close().await;
    }

    /// Send a JSON-RPC message over the WebSocket connection.
    pub async fn send(&self, message: &JsonRpcMessage) -> Result<(), WebSocketTransportError> {
        if self.ws.ready_state() != WS_OPEN {
            return Err(WebSocketTransportError::SendNotOpen);
        }

        let json = serde_json::to_string(message)
            .map_err(|e| WebSocketTransportError::ParseError(e.to_string()))?;

        self.ws
            .send_text(&json)
            .await
            .map_err(|e| WebSocketTransportError::WebSocketError(e.to_string()))?;

        Ok(())
    }
}
