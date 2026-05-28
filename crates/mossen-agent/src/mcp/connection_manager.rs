//! MCP Connection Manager — context provider for MCP server connections.
//!
//! Translates `services/mcp/MCPConnectionManager.tsx` (React → pure logic).

use std::sync::Arc;

use crate::mcp::types::{McpServerConfig, ScopedMcpServerConfig};
use crate::mcp::utils::{Command, ServerResource, Tool};

/// Result from reconnecting an MCP server.
#[derive(Debug)]
pub struct ReconnectResult {
    pub tools: Vec<Tool>,
    pub commands: Vec<Command>,
    pub resources: Vec<ServerResource>,
}

/// MCP Connection Manager — manages reconnect and toggle operations.
pub struct McpConnectionManager {
    dynamic_config: Option<std::collections::HashMap<String, ScopedMcpServerConfig>>,
    is_strict_config: bool,
}

impl McpConnectionManager {
    pub fn new(
        dynamic_config: Option<std::collections::HashMap<String, ScopedMcpServerConfig>>,
        is_strict_config: bool,
    ) -> Self {
        Self {
            dynamic_config,
            is_strict_config,
        }
    }

    /// Get dynamic config reference.
    pub fn dynamic_config(
        &self,
    ) -> Option<&std::collections::HashMap<String, ScopedMcpServerConfig>> {
        self.dynamic_config.as_ref()
    }

    /// Check if strict config mode.
    pub fn is_strict_config(&self) -> bool {
        self.is_strict_config
    }
}

// === React-context style hook wrappers (TS `MCPConnectionManager.tsx`) ===

/// Function type returned by `use_mcp_reconnect` — invoke with a server name
/// to reconnect that MCP server, returning the reconnect result.
pub type McpReconnectFn = std::sync::Arc<
    dyn Fn(
            &str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = anyhow::Result<ReconnectResult>> + Send>,
        > + Send
        + Sync,
>;

/// Function type returned by `use_mcp_toggle_enabled` — toggles enabled state.
pub type McpToggleFn = std::sync::Arc<
    dyn Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Connection-manager context — analogous to TS `MCPConnectionContextValue`.
#[derive(Clone)]
pub struct McpConnectionContextValue {
    pub reconnect_mcp_server: McpReconnectFn,
    pub toggle_mcp_server: McpToggleFn,
}

/// `useMcpReconnect()` — pull the `reconnectMcpServer` fn from the context.
/// Panics with a parity-message if invoked outside `MCPConnectionManager`.
pub fn use_mcp_reconnect(ctx: Option<&McpConnectionContextValue>) -> McpReconnectFn {
    ctx.expect("useMcpReconnect must be used within MCPConnectionManager")
        .reconnect_mcp_server
        .clone()
}

/// `useMcpToggleEnabled()` — pull the `toggleMcpServer` fn from the context.
pub fn use_mcp_toggle_enabled(ctx: Option<&McpConnectionContextValue>) -> McpToggleFn {
    ctx.expect("useMcpToggleEnabled must be used within MCPConnectionManager")
        .toggle_mcp_server
        .clone()
}
