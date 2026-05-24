//! Direct connect hook (useDirectConnect.ts).
//!
//! Manages direct connection to a remote session, handling permission
//! confirmations and remote permission responses.

use std::collections::VecDeque;

/// A permission request from a remote session.
#[derive(Debug, Clone)]
pub struct RemotePermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub args: String,
    pub session_id: String,
}

/// Response to a remote permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemotePermissionResponse {
    Allow,
    Deny,
    AllowAlways,
}

/// State for direct connection management.
#[derive(Debug, Clone)]
pub struct DirectConnectState {
    pub is_connected: bool,
    pub session_id: Option<String>,
    pub pending_permissions: VecDeque<RemotePermissionRequest>,
    pub permission_history: Vec<(String, RemotePermissionResponse)>,
    pub auto_approve_tools: Vec<String>,
}

impl DirectConnectState {
    pub fn new() -> Self {
        Self {
            is_connected: false,
            session_id: None,
            pending_permissions: VecDeque::new(),
            permission_history: Vec::new(),
            auto_approve_tools: Vec::new(),
        }
    }

    /// Connect to a remote session.
    pub fn connect(&mut self, session_id: String) {
        self.is_connected = true;
        self.session_id = Some(session_id);
    }

    /// Disconnect from the remote session.
    pub fn disconnect(&mut self) {
        self.is_connected = false;
        self.session_id = None;
        self.pending_permissions.clear();
    }

    /// Add a permission request to the queue.
    pub fn add_permission_request(&mut self, request: RemotePermissionRequest) {
        // Check auto-approve list first
        if self.auto_approve_tools.contains(&request.tool_name) {
            self.permission_history
                .push((request.id, RemotePermissionResponse::Allow));
            return;
        }
        self.pending_permissions.push_back(request);
    }

    /// Respond to the current permission request.
    pub fn respond(
        &mut self,
        response: RemotePermissionResponse,
    ) -> Option<RemotePermissionRequest> {
        if let Some(request) = self.pending_permissions.pop_front() {
            if response == RemotePermissionResponse::AllowAlways {
                self.auto_approve_tools.push(request.tool_name.clone());
            }
            self.permission_history.push((request.id.clone(), response));
            Some(request)
        } else {
            None
        }
    }

    /// Get the current pending permission request.
    pub fn current_permission(&self) -> Option<&RemotePermissionRequest> {
        self.pending_permissions.front()
    }

    /// Check if there are pending permissions.
    pub fn has_pending_permissions(&self) -> bool {
        !self.pending_permissions.is_empty()
    }
}

impl Default for DirectConnectState {
    fn default() -> Self {
        Self::new()
    }
}
