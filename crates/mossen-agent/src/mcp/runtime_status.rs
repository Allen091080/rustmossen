//! Runtime MCP status snapshots for user-facing UI surfaces.
//!
//! The CLI installs `mossen_mcp::server::McpServerManager` as a process-wide
//! singleton after startup. This module gives the TUI a small, stable view of
//! that live state without depending directly on the `mossen-mcp` crate.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMcpServerStatus {
    pub name: String,
    pub state: RuntimeMcpConnectionState,
    pub transport: String,
    pub tools_count: usize,
    pub prompts_count: usize,
    pub resources_count: usize,
    pub scope: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMcpConnectionState {
    Connected,
    Pending,
    Failed,
    NeedsAuth,
    Disabled,
}

pub async fn snapshot() -> Option<Vec<RuntimeMcpServerStatus>> {
    let manager = mossen_mcp::server::global_manager()?;
    let connections = manager.get_all_connections();
    if connections.is_empty() {
        return Some(Vec::new());
    }

    let tool_counts: HashMap<String, usize> = manager
        .get_all_tools()
        .await
        .into_iter()
        .map(|(name, tools)| (name, tools.len()))
        .collect();
    let prompt_counts: HashMap<String, usize> = manager
        .get_all_prompts()
        .await
        .into_iter()
        .map(|(name, prompts)| (name, prompts.len()))
        .collect();
    let resource_counts: HashMap<String, usize> = manager
        .get_all_resources()
        .await
        .into_iter()
        .map(|(name, resources)| (name, resources.len()))
        .collect();

    let mut statuses: Vec<RuntimeMcpServerStatus> = connections
        .into_iter()
        .map(|connection| {
            let name = connection.name().to_string();
            let config = connection_config(&connection);
            RuntimeMcpServerStatus {
                state: runtime_state(&connection),
                transport: transport_label(&config.config).to_string(),
                scope: format!("{:?}", config.scope).to_lowercase(),
                tools_count: tool_counts.get(&name).copied().unwrap_or(0),
                prompts_count: prompt_counts.get(&name).copied().unwrap_or(0),
                resources_count: resource_counts.get(&name).copied().unwrap_or(0),
                last_error: connection_error(&connection),
                name,
            }
        })
        .collect();
    statuses.sort_by(|a, b| a.name.cmp(&b.name));
    Some(statuses)
}

fn connection_config(
    connection: &mossen_mcp::client::McpServerConnection,
) -> mossen_mcp::config::ScopedMcpServerConfig {
    match connection {
        mossen_mcp::client::McpServerConnection::Connected(s) => s.config.clone(),
        mossen_mcp::client::McpServerConnection::Failed(s) => s.config.clone(),
        mossen_mcp::client::McpServerConnection::NeedsAuth(s) => s.config.clone(),
        mossen_mcp::client::McpServerConnection::Pending(s) => s.config.clone(),
        mossen_mcp::client::McpServerConnection::Disabled(s) => s.config.clone(),
    }
}

fn runtime_state(
    connection: &mossen_mcp::client::McpServerConnection,
) -> RuntimeMcpConnectionState {
    match connection {
        mossen_mcp::client::McpServerConnection::Connected(_) => {
            RuntimeMcpConnectionState::Connected
        }
        mossen_mcp::client::McpServerConnection::Failed(_) => RuntimeMcpConnectionState::Failed,
        mossen_mcp::client::McpServerConnection::NeedsAuth(_) => {
            RuntimeMcpConnectionState::NeedsAuth
        }
        mossen_mcp::client::McpServerConnection::Pending(_) => RuntimeMcpConnectionState::Pending,
        mossen_mcp::client::McpServerConnection::Disabled(_) => RuntimeMcpConnectionState::Disabled,
    }
}

fn connection_error(connection: &mossen_mcp::client::McpServerConnection) -> Option<String> {
    match connection {
        mossen_mcp::client::McpServerConnection::Failed(s) => s.error.clone(),
        mossen_mcp::client::McpServerConnection::Pending(s) => {
            match (s.reconnect_attempt, s.max_reconnect_attempts) {
                (Some(attempt), Some(max)) => Some(format!("reconnect {}/{}", attempt, max)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn transport_label(config: &mossen_mcp::config::McpServerConfig) -> &'static str {
    match config {
        mossen_mcp::config::McpServerConfig::Stdio(_) => "stdio",
        mossen_mcp::config::McpServerConfig::Sse(_) => "sse",
        mossen_mcp::config::McpServerConfig::SseIde(_) => "sse-ide",
        mossen_mcp::config::McpServerConfig::Http(_) => "http",
        mossen_mcp::config::McpServerConfig::Ws(_) => "ws",
        mossen_mcp::config::McpServerConfig::WsIde(_) => "ws-ide",
        mossen_mcp::config::McpServerConfig::Sdk(_) => "sdk",
        mossen_mcp::config::McpServerConfig::HostedProxy(_) => "hosted",
    }
}
