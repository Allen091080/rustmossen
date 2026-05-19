//! Remote session hook (useRemoteSession.ts).
//! Manages connection to a remote/SSH session.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSessionStatus { Disconnected, Connecting, Connected, Error }

#[derive(Debug, Clone)]
pub struct RemoteSessionState {
    pub status: RemoteSessionStatus,
    pub session_id: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub connected_at: Option<Instant>,
    pub error: Option<String>,
    pub reconnect_count: u32,
}

impl RemoteSessionState {
    pub fn new() -> Self {
        Self { status: RemoteSessionStatus::Disconnected, session_id: None, host: None, port: None, connected_at: None, error: None, reconnect_count: 0 }
    }
    pub fn connecting(&mut self, host: &str, port: u16) {
        self.status = RemoteSessionStatus::Connecting; self.host = Some(host.to_string()); self.port = Some(port); self.error = None;
    }
    pub fn connected(&mut self, session_id: String) {
        self.status = RemoteSessionStatus::Connected; self.session_id = Some(session_id); self.connected_at = Some(Instant::now()); self.error = None;
    }
    pub fn disconnected(&mut self) { self.status = RemoteSessionStatus::Disconnected; self.session_id = None; self.connected_at = None; }
    pub fn error(&mut self, msg: String) { self.status = RemoteSessionStatus::Error; self.error = Some(msg); }
    pub fn reconnecting(&mut self) { self.status = RemoteSessionStatus::Connecting; self.reconnect_count += 1; }
    pub fn is_connected(&self) -> bool { self.status == RemoteSessionStatus::Connected }
}
impl Default for RemoteSessionState { fn default() -> Self { Self::new() } }

/// Localized warning copy shown when the remote session reconnect timer
/// fires. The "hosted workspace" variant is used when a custom backend is
/// enabled; otherwise the generic message is shown.
///
/// TS source: `getRemoteReconnectWarningCopy()` in useRemoteSession.ts.
pub fn get_remote_reconnect_warning_copy(custom_backend_enabled: bool) -> &'static str {
    if custom_backend_enabled {
        "Hosted workspace session may be unresponsive. Attempting to reconnect…"
    } else {
        "Remote session may be unresponsive. Attempting to reconnect…"
    }
}
