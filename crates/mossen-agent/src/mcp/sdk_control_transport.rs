//! SDK MCP Transport Bridge
//!
//! Implements a transport bridge that allows MCP servers running in the SDK process
//! to communicate with the Mossen CLI process through control messages.

use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A JSON-RPC message type alias.
pub type JsonRpcMessage = Value;

/// Callback function to send an MCP message and get the response.
pub type SendMcpMessageCallback = Arc<
    dyn Fn(
            String,
            JsonRpcMessage,
        ) -> futures::future::BoxFuture<'static, anyhow::Result<JsonRpcMessage>>
        + Send
        + Sync,
>;

/// Callback function to send an MCP message (fire-and-forget for server side).
pub type SendMcpMessageServerCallback = Arc<dyn Fn(JsonRpcMessage) + Send + Sync>;

/// CLI-side transport for SDK MCP servers.
///
/// This transport is used in the CLI process to bridge communication between:
/// - The CLI's MCP Client (which wants to call tools on SDK MCP servers)
/// - The SDK process (where the actual MCP server runs)
///
/// It converts MCP protocol messages into control requests that can be sent
/// through stdout/stdin to the SDK process.
pub struct SdkControlClientTransport {
    server_name: String,
    send_mcp_message: SendMcpMessageCallback,
    is_closed: Arc<Mutex<bool>>,
    on_close: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>,
    on_error: Arc<Mutex<Option<Box<dyn Fn(anyhow::Error) + Send + Sync>>>>,
    on_message: Arc<Mutex<Option<Box<dyn Fn(JsonRpcMessage) + Send + Sync>>>>,
}

impl SdkControlClientTransport {
    /// Create a new SDK control client transport.
    pub fn new(server_name: String, send_mcp_message: SendMcpMessageCallback) -> Self {
        Self {
            server_name,
            send_mcp_message,
            is_closed: Arc::new(Mutex::new(false)),
            on_close: Arc::new(Mutex::new(None)),
            on_error: Arc::new(Mutex::new(None)),
            on_message: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the transport (no-op).
    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Send a message through the control channel and deliver the response
    /// back to the MCP client via on_message.
    pub async fn send(&self, message: JsonRpcMessage) -> anyhow::Result<()> {
        let closed = self.is_closed.lock().await;
        if *closed {
            return Err(anyhow::anyhow!("Transport is closed"));
        }
        drop(closed);

        // Send the message and wait for the response
        let response = (self.send_mcp_message)(self.server_name.clone(), message).await?;

        // Pass the response back to the MCP client
        if let Some(on_message) = self.on_message.lock().await.as_ref() {
            on_message(response);
        }

        Ok(())
    }

    /// Close the transport.
    pub async fn close(&self) {
        let mut closed = self.is_closed.lock().await;
        if *closed {
            return;
        }
        *closed = true;
        drop(closed);

        if let Some(on_close) = self.on_close.lock().await.as_ref() {
            on_close();
        }
    }

    /// Set the on_message callback.
    pub async fn set_on_message(&self, f: Box<dyn Fn(JsonRpcMessage) + Send + Sync>) {
        *self.on_message.lock().await = Some(f);
    }

    /// Set the on_close callback.
    pub async fn set_on_close(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self.on_close.lock().await = Some(f);
    }

    /// Set the on_error callback.
    pub async fn set_on_error(&self, f: Box<dyn Fn(anyhow::Error) + Send + Sync>) {
        *self.on_error.lock().await = Some(f);
    }
}

/// SDK-side transport for SDK MCP servers.
///
/// This transport is used in the SDK process to bridge communication between:
/// - Control requests coming from the CLI (via stdin)
/// - The actual MCP server running in the SDK process
///
/// It acts as a simple pass-through that forwards messages to the MCP server
/// and sends responses back via a callback.
pub struct SdkControlServerTransport {
    send_mcp_message: SendMcpMessageServerCallback,
    is_closed: Arc<Mutex<bool>>,
    on_close: Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>,
    on_error: Arc<Mutex<Option<Box<dyn Fn(anyhow::Error) + Send + Sync>>>>,
    on_message: Arc<Mutex<Option<Box<dyn Fn(JsonRpcMessage) + Send + Sync>>>>,
}

impl SdkControlServerTransport {
    /// Create a new SDK control server transport.
    pub fn new(send_mcp_message: SendMcpMessageServerCallback) -> Self {
        Self {
            send_mcp_message,
            is_closed: Arc::new(Mutex::new(false)),
            on_close: Arc::new(Mutex::new(None)),
            on_error: Arc::new(Mutex::new(None)),
            on_message: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the transport (no-op).
    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Send a response back through the callback.
    pub async fn send(&self, message: JsonRpcMessage) -> anyhow::Result<()> {
        let closed = self.is_closed.lock().await;
        if *closed {
            return Err(anyhow::anyhow!("Transport is closed"));
        }
        drop(closed);

        // Simply pass the response back through the callback
        (self.send_mcp_message)(message);
        Ok(())
    }

    /// Close the transport.
    pub async fn close(&self) {
        let mut closed = self.is_closed.lock().await;
        if *closed {
            return;
        }
        *closed = true;
        drop(closed);

        if let Some(on_close) = self.on_close.lock().await.as_ref() {
            on_close();
        }
    }

    /// Deliver an inbound message to the MCP server via on_message.
    pub async fn deliver_message(&self, message: JsonRpcMessage) {
        if let Some(on_message) = self.on_message.lock().await.as_ref() {
            on_message(message);
        }
    }

    /// Set the on_message callback.
    pub async fn set_on_message(&self, f: Box<dyn Fn(JsonRpcMessage) + Send + Sync>) {
        *self.on_message.lock().await = Some(f);
    }

    /// Set the on_close callback.
    pub async fn set_on_close(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self.on_close.lock().await = Some(f);
    }

    /// Set the on_error callback.
    pub async fn set_on_error(&self, f: Box<dyn Fn(anyhow::Error) + Send + Sync>) {
        *self.on_error.lock().await = Some(f);
    }
}
