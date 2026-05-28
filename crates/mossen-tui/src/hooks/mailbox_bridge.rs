//! Mailbox bridge hook (useMailboxBridge.ts).
//!
//! Bridges the mailbox system with the React render cycle,
//! forwarding messages between the mailbox and UI state.

use std::collections::VecDeque;

/// A mailbox message.
#[derive(Debug, Clone)]
pub struct MailboxMessage {
    pub id: String,
    pub channel: String,
    pub payload: serde_json::Value,
    pub timestamp: u64,
}

/// State for the mailbox bridge.
#[derive(Debug, Clone)]
pub struct MailboxBridgeState {
    pub inbox: VecDeque<MailboxMessage>,
    pub outbox: VecDeque<MailboxMessage>,
    pub subscribed_channels: Vec<String>,
    pub connected: bool,
}

impl MailboxBridgeState {
    pub fn new() -> Self {
        Self {
            inbox: VecDeque::new(),
            outbox: VecDeque::new(),
            subscribed_channels: Vec::new(),
            connected: false,
        }
    }

    /// Subscribe to a channel.
    pub fn subscribe(&mut self, channel: String) {
        if !self.subscribed_channels.contains(&channel) {
            self.subscribed_channels.push(channel);
        }
    }

    /// Unsubscribe from a channel.
    pub fn unsubscribe(&mut self, channel: &str) {
        self.subscribed_channels.retain(|c| c != channel);
    }

    /// Receive a message into the inbox.
    pub fn receive(&mut self, message: MailboxMessage) {
        if self.subscribed_channels.contains(&message.channel) {
            self.inbox.push_back(message);
        }
    }

    /// Send a message (add to outbox).
    pub fn send(&mut self, channel: String, payload: serde_json::Value) {
        self.outbox.push_back(MailboxMessage {
            id: uuid::Uuid::new_v4().to_string(),
            channel,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        });
    }

    /// Take all inbox messages.
    pub fn take_inbox(&mut self) -> Vec<MailboxMessage> {
        self.inbox.drain(..).collect()
    }

    /// Take all outbox messages.
    pub fn take_outbox(&mut self) -> Vec<MailboxMessage> {
        self.outbox.drain(..).collect()
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }
}

impl Default for MailboxBridgeState {
    fn default() -> Self {
        Self::new()
    }
}
