//! IDE connection status hook (useIdeConnectionStatus.ts).
//!
//! Monitors the connection status to an IDE (VS Code, etc.).

use std::time::Instant;

/// IDE connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdeConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

/// State for IDE connection status monitoring.
#[derive(Debug, Clone)]
pub struct IdeConnectionStatusState {
    pub status: IdeConnectionStatus,
    pub ide_name: Option<String>,
    pub connected_at: Option<Instant>,
    pub last_heartbeat: Option<Instant>,
    pub reconnect_attempts: u32,
    pub error_message: Option<String>,
}

impl IdeConnectionStatusState {
    pub fn new() -> Self {
        Self {
            status: IdeConnectionStatus::Disconnected,
            ide_name: None,
            connected_at: None,
            last_heartbeat: None,
            reconnect_attempts: 0,
            error_message: None,
        }
    }

    /// Mark as connecting.
    pub fn connecting(&mut self) {
        self.status = IdeConnectionStatus::Connecting;
        self.error_message = None;
    }

    /// Mark as connected.
    pub fn connected(&mut self, ide_name: String) {
        self.status = IdeConnectionStatus::Connected;
        self.ide_name = Some(ide_name);
        self.connected_at = Some(Instant::now());
        self.last_heartbeat = Some(Instant::now());
        self.reconnect_attempts = 0;
        self.error_message = None;
    }

    /// Process a heartbeat.
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Some(Instant::now());
    }

    /// Mark as disconnected.
    pub fn disconnected(&mut self) {
        self.status = IdeConnectionStatus::Disconnected;
        self.connected_at = None;
    }

    /// Mark as reconnecting.
    pub fn reconnecting(&mut self) {
        self.status = IdeConnectionStatus::Reconnecting;
        self.reconnect_attempts += 1;
    }

    /// Mark as error.
    pub fn error(&mut self, message: String) {
        self.status = IdeConnectionStatus::Error;
        self.error_message = Some(message);
    }

    /// Check if connection may be stale (no heartbeat for 30s).
    pub fn is_stale(&self) -> bool {
        match self.last_heartbeat {
            Some(last) => last.elapsed().as_secs() > 30,
            None => false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.status == IdeConnectionStatus::Connected
    }
}

impl Default for IdeConnectionStatusState {
    fn default() -> Self {
        Self::new()
    }
}
