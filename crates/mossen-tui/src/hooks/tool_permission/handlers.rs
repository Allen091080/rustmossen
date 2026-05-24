//! Permission handlers (handlers/*.ts).
//! Different handlers for interactive, coordinator, and swarm worker modes.

/// Handler type for permission processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionHandlerType {
    Interactive,
    Coordinator,
    SwarmWorker,
}

/// A permission request to be handled.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub description: String,
    pub handler_type: PermissionHandlerType,
}

/// Response to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    Allow,
    Deny,
    AllowAlways,
    AllowSession,
}

/// State for permission handler management.
#[derive(Debug, Clone)]
pub struct PermissionHandlerState {
    pub handler_type: PermissionHandlerType,
    pub pending_request: Option<PermissionRequest>,
    pub auto_approve_patterns: Vec<String>,
}

impl PermissionHandlerState {
    pub fn new(handler_type: PermissionHandlerType) -> Self {
        Self {
            handler_type,
            pending_request: None,
            auto_approve_patterns: Vec::new(),
        }
    }

    /// Submit a permission request for handling.
    pub fn submit_request(&mut self, request: PermissionRequest) {
        // Check auto-approve patterns
        if self.should_auto_approve(&request.tool_name) {
            // Auto-approved, no need to queue
            return;
        }
        self.pending_request = Some(request);
    }

    /// Respond to the pending request.
    pub fn respond(&mut self, response: PermissionResponse) -> Option<PermissionRequest> {
        let request = self.pending_request.take();
        if let Some(ref req) = request {
            if response == PermissionResponse::AllowAlways {
                self.auto_approve_patterns.push(req.tool_name.clone());
            }
        }
        request
    }

    /// Check if a tool should be auto-approved.
    fn should_auto_approve(&self, tool_name: &str) -> bool {
        self.auto_approve_patterns
            .iter()
            .any(|p| p == tool_name || p == "*")
    }

    /// Check if there is a pending request.
    pub fn has_pending(&self) -> bool {
        self.pending_request.is_some()
    }

    /// Get the pending request for display.
    pub fn pending(&self) -> Option<&PermissionRequest> {
        self.pending_request.as_ref()
    }
}

impl Default for PermissionHandlerState {
    fn default() -> Self {
        Self::new(PermissionHandlerType::Interactive)
    }
}
