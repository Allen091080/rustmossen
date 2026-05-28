//! In-process linked transport pair for running an MCP server and client
//! in the same process without spawning a subprocess.
//!
//! `send()` on one side delivers to `onmessage` on the other.
//! `close()` on either side calls `onclose` on both.

use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};

/// A JSON-RPC message (opaque Value for transport layer).
pub type JsonRpcMessage = Value;

/// Callback type for message reception.
pub type OnMessageFn = Box<dyn Fn(JsonRpcMessage) + Send + Sync>;
/// Callback type for close event.
pub type OnCloseFn = Box<dyn Fn() + Send + Sync>;
/// Callback type for error event.
pub type OnErrorFn = Box<dyn Fn(anyhow::Error) + Send + Sync>;

/// One side of an in-process transport pair.
pub struct InProcessTransport {
    tx: mpsc::UnboundedSender<JsonRpcMessage>,
    closed: Arc<Mutex<bool>>,
    on_close: Arc<Mutex<Option<OnCloseFn>>>,
    on_error: Arc<Mutex<Option<OnErrorFn>>>,
    on_message: Arc<Mutex<Option<OnMessageFn>>>,
    _recv_handle: tokio::task::JoinHandle<()>,
}

impl InProcessTransport {
    /// Start the transport (no-op for in-process).
    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Send a message to the peer side.
    pub async fn send(&self, message: JsonRpcMessage) -> anyhow::Result<()> {
        let closed = self.closed.lock().await;
        if *closed {
            return Err(anyhow::anyhow!("Transport is closed"));
        }
        self.tx
            .send(message)
            .map_err(|_| anyhow::anyhow!("Peer channel dropped"))
    }

    /// Close this transport side.
    pub async fn close(&self) {
        let mut closed = self.closed.lock().await;
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
    pub async fn set_on_message(&self, f: OnMessageFn) {
        *self.on_message.lock().await = Some(f);
    }

    /// Set the on_close callback.
    pub async fn set_on_close(&self, f: OnCloseFn) {
        *self.on_close.lock().await = Some(f);
    }

    /// Set the on_error callback.
    pub async fn set_on_error(&self, f: OnErrorFn) {
        *self.on_error.lock().await = Some(f);
    }
}

/// Creates a pair of linked transports for in-process MCP communication.
/// Messages sent on one transport are delivered to the other's `on_message`.
///
/// Returns `(client_transport, server_transport)`.
pub fn create_linked_transport_pair() -> (InProcessTransport, InProcessTransport) {
    let (tx_a, mut rx_a) = mpsc::unbounded_channel::<JsonRpcMessage>();
    let (tx_b, mut rx_b) = mpsc::unbounded_channel::<JsonRpcMessage>();

    let on_message_a: Arc<Mutex<Option<OnMessageFn>>> = Arc::new(Mutex::new(None));
    let on_message_b: Arc<Mutex<Option<OnMessageFn>>> = Arc::new(Mutex::new(None));
    let on_close_a: Arc<Mutex<Option<OnCloseFn>>> = Arc::new(Mutex::new(None));
    let on_close_b: Arc<Mutex<Option<OnCloseFn>>> = Arc::new(Mutex::new(None));
    let on_error_a: Arc<Mutex<Option<OnErrorFn>>> = Arc::new(Mutex::new(None));
    let on_error_b: Arc<Mutex<Option<OnErrorFn>>> = Arc::new(Mutex::new(None));
    let closed_a = Arc::new(Mutex::new(false));
    let closed_b = Arc::new(Mutex::new(false));

    // Messages sent to tx_a are received by transport B's on_message
    let on_message_b_clone = on_message_b.clone();
    let handle_a = tokio::spawn(async move {
        while let Some(msg) = rx_a.recv().await {
            if let Some(handler) = on_message_b_clone.lock().await.as_ref() {
                handler(msg);
            }
        }
    });

    // Messages sent to tx_b are received by transport A's on_message
    let on_message_a_clone = on_message_a.clone();
    let handle_b = tokio::spawn(async move {
        while let Some(msg) = rx_b.recv().await {
            if let Some(handler) = on_message_a_clone.lock().await.as_ref() {
                handler(msg);
            }
        }
    });

    let transport_a = InProcessTransport {
        tx: tx_b, // A sends to B's receiver
        closed: closed_a,
        on_close: on_close_a,
        on_error: on_error_a,
        on_message: on_message_a,
        _recv_handle: handle_b,
    };

    let transport_b = InProcessTransport {
        tx: tx_a, // B sends to A's receiver
        closed: closed_b,
        on_close: on_close_b,
        on_error: on_error_b,
        on_message: on_message_b,
        _recv_handle: handle_a,
    };

    (transport_a, transport_b)
}
