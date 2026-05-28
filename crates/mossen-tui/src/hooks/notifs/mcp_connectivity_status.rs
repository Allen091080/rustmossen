//! McpConnectivityStatus notification (mcp_connectivity_status.ts).
//! Shows MCP server connectivity status notifications.

#[derive(Debug, Clone)]
pub struct McpConnectivityStatusNotificationState {
    pub shown: bool,
    pub dismissed: bool,
    pub notification_key: String,
    pub message: Option<String>,
}

impl McpConnectivityStatusNotificationState {
    pub fn new() -> Self {
        Self {
            shown: false,
            dismissed: false,
            notification_key: "mcp-connectivity-status".to_string(),
            message: None,
        }
    }

    /// Check conditions and set notification message if needed.
    pub fn check(&mut self, should_show: bool, message: &str) {
        if should_show && !self.shown && !self.dismissed {
            self.shown = true;
            self.message = Some(message.to_string());
        }
    }

    /// Dismiss the notification.
    pub fn dismiss(&mut self) {
        self.dismissed = true;
        self.message = None;
    }

    /// Get the notification message if active.
    pub fn active_message(&self) -> Option<&str> {
        if self.shown && !self.dismissed {
            self.message.as_deref()
        } else {
            None
        }
    }

    /// Reset state for re-evaluation.
    pub fn reset(&mut self) {
        self.shown = false;
        self.message = None;
    }
}

impl Default for McpConnectivityStatusNotificationState {
    fn default() -> Self {
        Self::new()
    }
}

/// One MCP client's connection state, scoped to the fields the
/// notification hook cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpClientStatus {
    pub name: String,
    pub kind: McpClientKind,
    pub config_kind: McpConfigKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpClientKind {
    Connected,
    Failed,
    NeedsAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConfigKind {
    Stdio,
    Sse,
    SseIde,
    Ws,
    WsIde,
    HostedProxy,
}

/// One notification to surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConnectivityNotification {
    pub key: String,
    pub text: String,
    pub color: String,
    pub priority: String,
}

/// `useMcpConnectivityStatus` — pure-logic translation.
///
/// TS source: `useMcpConnectivityStatus({ mcpClients })`.
pub fn use_mcp_connectivity_status(
    mcp_clients: &[McpClientStatus],
    is_remote_mode: bool,
    hosted_mcp_ever_connected: impl Fn(&str) -> bool,
) -> Vec<McpConnectivityNotification> {
    let mut out = Vec::new();
    if is_remote_mode {
        return out;
    }

    let failed_local = mcp_clients
        .iter()
        .filter(|c| {
            c.kind == McpClientKind::Failed
                && c.config_kind != McpConfigKind::SseIde
                && c.config_kind != McpConfigKind::WsIde
                && c.config_kind != McpConfigKind::HostedProxy
        })
        .count();

    let failed_hosted = mcp_clients
        .iter()
        .filter(|c| {
            c.kind == McpClientKind::Failed
                && c.config_kind == McpConfigKind::HostedProxy
                && hosted_mcp_ever_connected(&c.name)
        })
        .count();

    let needs_auth_local = mcp_clients
        .iter()
        .filter(|c| {
            c.kind == McpClientKind::NeedsAuth && c.config_kind != McpConfigKind::HostedProxy
        })
        .count();

    let needs_auth_hosted = mcp_clients
        .iter()
        .filter(|c| {
            c.kind == McpClientKind::NeedsAuth
                && c.config_kind == McpConfigKind::HostedProxy
                && hosted_mcp_ever_connected(&c.name)
        })
        .count();

    if failed_local > 0 {
        let word = if failed_local == 1 {
            "server"
        } else {
            "servers"
        };
        out.push(McpConnectivityNotification {
            key: "mcp-failed".to_string(),
            text: format!("{} MCP {} failed · /mcp", failed_local, word),
            color: "error".to_string(),
            priority: "medium".to_string(),
        });
    }
    if failed_hosted > 0 {
        let word = if failed_hosted == 1 {
            "connector"
        } else {
            "connectors"
        };
        out.push(McpConnectivityNotification {
            key: "mcp-hosted-failed".to_string(),
            text: format!("{} hosted {} unavailable · /mcp", failed_hosted, word),
            color: "error".to_string(),
            priority: "medium".to_string(),
        });
    }
    if needs_auth_local > 0 {
        let phrase = if needs_auth_local == 1 {
            "server needs"
        } else {
            "servers need"
        };
        out.push(McpConnectivityNotification {
            key: "mcp-needs-auth".to_string(),
            text: format!("{} MCP {} auth · /mcp", needs_auth_local, phrase),
            color: "warning".to_string(),
            priority: "medium".to_string(),
        });
    }
    if needs_auth_hosted > 0 {
        let phrase = if needs_auth_hosted == 1 {
            "connector needs"
        } else {
            "connectors need"
        };
        out.push(McpConnectivityNotification {
            key: "mcp-hosted-needs-auth".to_string(),
            text: format!("{} hosted {} auth · /mcp", needs_auth_hosted, phrase),
            color: "warning".to_string(),
            priority: "medium".to_string(),
        });
    }
    out
}
