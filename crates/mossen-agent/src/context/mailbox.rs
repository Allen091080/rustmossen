//! Mailbox context — inter-component message passing.
//!
//! Translates: context/mailbox.tsx
//! React context → struct-based mailbox.

use std::sync::Arc;
use tokio::sync::mpsc;

/// A simple mailbox for passing messages between components.
///
/// Messages are strings; the receiver processes them asynchronously.
#[derive(Debug, Clone)]
pub struct Mailbox {
    tx: mpsc::UnboundedSender<String>,
    rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>>,
}

impl Mailbox {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
        }
    }

    /// Send a message to the mailbox.
    pub fn send(&self, message: String) -> Result<(), mpsc::error::SendError<String>> {
        self.tx.send(message)
    }

    /// Receive the next message from the mailbox (async).
    pub async fn recv(&self) -> Option<String> {
        self.rx.lock().await.recv().await
    }

    /// Try to receive a message without waiting.
    pub fn try_recv(&self) -> Option<String> {
        self.rx
            .try_lock()
            .ok()
            .and_then(|mut rx| rx.try_recv().ok())
    }
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of the mailbox context. Mirrors React `useMailbox()`.
pub fn use_mailbox(ctx: &Mailbox) -> Mailbox {
    ctx.clone()
}

/// Provider entry-point — initialises a fresh mailbox. Mirrors
/// React `MailboxProvider`.
pub fn mailbox_provider() -> Mailbox {
    Mailbox::default()
}
