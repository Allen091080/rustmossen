//! WebSocket transport for MCP (Model Context Protocol).
//!
//! Implements a WebSocket-based transport that handles JSON-RPC messages
//! over WebSocket connections.

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// WebSocket ready state constants.
const WS_CONNECTING: u8 = 0;
const WS_OPEN: u8 = 1;

/// JSON-RPC message (generic).
pub type JsonRpcMessage = serde_json::Value;

/// Callbacks for the WebSocket transport.
pub struct TransportCallbacks {
    pub on_close: Option<Box<dyn Fn() + Send + Sync>>,
    pub on_error: Option<Box<dyn Fn(anyhow::Error) + Send + Sync>>,
    pub on_message: Option<Box<dyn Fn(JsonRpcMessage) + Send + Sync>>,
}

impl Default for TransportCallbacks {
    fn default() -> Self {
        Self {
            on_close: None,
            on_error: None,
            on_message: None,
        }
    }
}

/// Minimal WebSocket-like trait for abstracting over different WS implementations.
#[async_trait::async_trait]
pub trait WebSocketLike: Send + Sync {
    fn ready_state(&self) -> u8;
    async fn send_text(&self, data: &str) -> Result<()>;
    async fn close(&self) -> Result<()>;
}

/// WebSocket transport for MCP.
pub struct WebSocketTransport<W: WebSocketLike> {
    ws: Arc<W>,
    started: Mutex<bool>,
    callbacks: Mutex<TransportCallbacks>,
}

impl<W: WebSocketLike + 'static> WebSocketTransport<W> {
    /// Create a new WebSocket transport.
    pub fn new(ws: W) -> Self {
        Self {
            ws: Arc::new(ws),
            started: Mutex::new(false),
            callbacks: Mutex::new(TransportCallbacks::default()),
        }
    }

    /// Set the on_close callback.
    pub async fn set_on_close<F: Fn() + Send + Sync + 'static>(&self, f: F) {
        self.callbacks.lock().await.on_close = Some(Box::new(f));
    }

    /// Set the on_error callback.
    pub async fn set_on_error<F: Fn(anyhow::Error) + Send + Sync + 'static>(&self, f: F) {
        self.callbacks.lock().await.on_error = Some(Box::new(f));
    }

    /// Set the on_message callback.
    pub async fn set_on_message<F: Fn(JsonRpcMessage) + Send + Sync + 'static>(&self, f: F) {
        self.callbacks.lock().await.on_message = Some(Box::new(f));
    }

    /// Handle an incoming text message from the WebSocket.
    pub async fn handle_incoming_message(&self, data: &str) {
        match serde_json::from_str::<JsonRpcMessage>(data) {
            Ok(message) => {
                let cbs = self.callbacks.lock().await;
                if let Some(ref on_message) = cbs.on_message {
                    on_message(message);
                }
            }
            Err(e) => {
                self.handle_error(anyhow!("Failed to parse JSON-RPC message: {}", e))
                    .await;
            }
        }
    }

    /// Handle an error.
    async fn handle_error(&self, error: anyhow::Error) {
        tracing::error!("mcp_websocket_message_fail");
        let cbs = self.callbacks.lock().await;
        if let Some(ref on_error) = cbs.on_error {
            on_error(error);
        }
    }

    /// Handle close with listener cleanup.
    pub async fn handle_close_cleanup(&self) {
        let cbs = self.callbacks.lock().await;
        if let Some(ref on_close) = cbs.on_close {
            on_close();
        }
    }

    /// Start listening for messages. Can only be called once.
    pub async fn start(&self) -> Result<()> {
        let mut started = self.started.lock().await;
        if *started {
            return Err(anyhow!("Start can only be called once per transport."));
        }

        if self.ws.ready_state() != WS_OPEN {
            tracing::error!("mcp_websocket_start_not_opened");
            return Err(anyhow!("WebSocket is not open. Cannot start transport."));
        }

        *started = true;
        Ok(())
    }

    /// Close the WebSocket connection.
    pub async fn close(&self) -> Result<()> {
        let state = self.ws.ready_state();
        if state == WS_OPEN || state == WS_CONNECTING {
            self.ws.close().await?;
        }
        self.handle_close_cleanup().await;
        Ok(())
    }

    /// Send a JSON-RPC message over the WebSocket connection.
    pub async fn send(&self, message: &JsonRpcMessage) -> Result<()> {
        if self.ws.ready_state() != WS_OPEN {
            tracing::error!("mcp_websocket_send_not_opened");
            return Err(anyhow!("WebSocket is not open. Cannot send message."));
        }

        let json = serde_json::to_string(message)
            .map_err(|e| anyhow!("Failed to serialize JSON-RPC message: {}", e))?;

        match self.ws.send_text(&json).await {
            Ok(_) => Ok(()),
            Err(e) => {
                self.handle_error(anyhow!("WebSocket send failed: {}", e))
                    .await;
                Err(e)
            }
        }
    }
}
