//! MCP connection management — manages lifecycle of MCP server connections.
//!
//! Translates `services/mcp/useManageMCPConnections.ts` (React hook → struct + impl).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::mcp::channel_notification::{
    find_channel_entry, gate_channel_server, ChannelEntry, ChannelGateResult, ChannelGateSkipKind,
    ServerCapabilities,
};
use crate::mcp::channel_permissions::{
    create_channel_permission_callbacks, ChannelPermissionCallbacks,
};
use crate::mcp::types::{ConfigScope, McpServerConfig, ScopedMcpServerConfig};
use crate::mcp::utils::{Command, ServerResource, Tool};

/// Constants for reconnection with exponential backoff.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 30000;

/// Connection state for an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Pending {
        reconnect_attempt: Option<u32>,
        max_reconnect_attempts: Option<u32>,
    },
    Connected,
    Failed,
    Disabled,
}

/// A managed MCP server connection.
#[derive(Debug, Clone)]
pub struct ManagedConnection {
    pub name: String,
    pub state: ConnectionState,
    pub config: ScopedMcpServerConfig,
    pub tools: Vec<Tool>,
    pub commands: Vec<Command>,
    pub resources: Vec<ServerResource>,
}

/// Event emitted when connection state changes.
#[derive(Debug, Clone)]
pub struct ConnectionStateChange {
    pub server_name: String,
    pub new_state: ConnectionState,
}

/// Trait for the underlying MCP client connection logic.
#[async_trait::async_trait]
pub trait McpConnector: Send + Sync {
    async fn connect(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
    ) -> Result<ConnectResult, Box<dyn std::error::Error + Send + Sync>>;

    async fn disconnect(&self, name: &str, config: &ScopedMcpServerConfig);

    fn is_server_disabled(&self, name: &str) -> bool;

    async fn get_configs(
        &self,
        dynamic_config: Option<&HashMap<String, ScopedMcpServerConfig>>,
        is_strict: bool,
    ) -> HashMap<String, ScopedMcpServerConfig>;
}

/// Result of connecting to an MCP server.
pub struct ConnectResult {
    pub state: ConnectionState,
    pub tools: Vec<Tool>,
    pub commands: Vec<Command>,
    pub resources: Vec<ServerResource>,
}

/// MCP connection manager state.
pub struct McpConnectionState {
    connections: Arc<RwLock<HashMap<String, ManagedConnection>>>,
    reconnect_cancellers: Arc<Mutex<HashMap<String, CancellationToken>>>,
    channel_perm_callbacks: Option<ChannelPermissionCallbacks>,
    state_tx: broadcast::Sender<ConnectionStateChange>,
}

impl McpConnectionState {
    pub fn new(enable_channel_permissions: bool) -> Self {
        let (state_tx, _) = broadcast::channel(256);
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            reconnect_cancellers: Arc::new(Mutex::new(HashMap::new())),
            channel_perm_callbacks: if enable_channel_permissions {
                Some(create_channel_permission_callbacks())
            } else {
                None
            },
            state_tx,
        }
    }

    /// Get a reference to the channel permission callbacks.
    pub fn channel_permission_callbacks(&self) -> Option<&ChannelPermissionCallbacks> {
        self.channel_perm_callbacks.as_ref()
    }

    /// Subscribe to connection state changes.
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionStateChange> {
        self.state_tx.subscribe()
    }

    /// Get all current connections.
    pub async fn get_connections(&self) -> HashMap<String, ManagedConnection> {
        self.connections.read().await.clone()
    }

    /// Get a single connection by name.
    pub async fn get_connection(&self, name: &str) -> Option<ManagedConnection> {
        self.connections.read().await.get(name).cloned()
    }

    /// Update a server's state.
    pub async fn update_server(&self, connection: ManagedConnection) {
        let name = connection.name.clone();
        let state = connection.state.clone();
        self.connections
            .write()
            .await
            .insert(name.clone(), connection);
        let _ = self.state_tx.send(ConnectionStateChange {
            server_name: name,
            new_state: state,
        });
    }

    /// Initialize connections from configs.
    pub async fn initialize(
        &self,
        connector: &dyn McpConnector,
        dynamic_config: Option<&HashMap<String, ScopedMcpServerConfig>>,
        is_strict: bool,
    ) {
        let configs = connector.get_configs(dynamic_config, is_strict).await;

        for (name, config) in &configs {
            if connector.is_server_disabled(name) {
                self.update_server(ManagedConnection {
                    name: name.clone(),
                    state: ConnectionState::Disabled,
                    config: config.clone(),
                    tools: Vec::new(),
                    commands: Vec::new(),
                    resources: Vec::new(),
                })
                .await;
                continue;
            }

            // Set pending state
            self.update_server(ManagedConnection {
                name: name.clone(),
                state: ConnectionState::Pending {
                    reconnect_attempt: None,
                    max_reconnect_attempts: None,
                },
                config: config.clone(),
                tools: Vec::new(),
                commands: Vec::new(),
                resources: Vec::new(),
            })
            .await;

            // Connect
            match connector.connect(name, config).await {
                Ok(result) => {
                    self.update_server(ManagedConnection {
                        name: name.clone(),
                        state: result.state,
                        config: config.clone(),
                        tools: result.tools,
                        commands: result.commands,
                        resources: result.resources,
                    })
                    .await;
                }
                Err(e) => {
                    tracing::error!(name, error = %e, "Failed to connect MCP server");
                    self.update_server(ManagedConnection {
                        name: name.clone(),
                        state: ConnectionState::Failed,
                        config: config.clone(),
                        tools: Vec::new(),
                        commands: Vec::new(),
                        resources: Vec::new(),
                    })
                    .await;
                }
            }
        }
    }

    /// Reconnect a specific MCP server.
    pub async fn reconnect_server(
        &self,
        name: &str,
        connector: &dyn McpConnector,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Cancel any existing reconnect
        {
            let mut cancellers = self.reconnect_cancellers.lock().await;
            if let Some(cancel) = cancellers.remove(name) {
                cancel.cancel();
            }
        }

        let conn = self.get_connection(name).await;
        let config = match conn {
            Some(c) => c.config,
            None => return Err(format!("Server {} not found", name).into()),
        };

        self.update_server(ManagedConnection {
            name: name.to_string(),
            state: ConnectionState::Pending {
                reconnect_attempt: None,
                max_reconnect_attempts: None,
            },
            config: config.clone(),
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
        .await;

        match connector.connect(name, &config).await {
            Ok(result) => {
                self.update_server(ManagedConnection {
                    name: name.to_string(),
                    state: result.state,
                    config,
                    tools: result.tools,
                    commands: result.commands,
                    resources: result.resources,
                })
                .await;
                Ok(())
            }
            Err(e) => {
                self.update_server(ManagedConnection {
                    name: name.to_string(),
                    state: ConnectionState::Failed,
                    config,
                    tools: Vec::new(),
                    commands: Vec::new(),
                    resources: Vec::new(),
                })
                .await;
                Err(e)
            }
        }
    }

    /// Reconnect with exponential backoff (for remote transports).
    pub async fn reconnect_with_backoff(
        &self,
        name: &str,
        config: ScopedMcpServerConfig,
        connector: Arc<dyn McpConnector>,
    ) {
        let cancel = CancellationToken::new();
        {
            let mut cancellers = self.reconnect_cancellers.lock().await;
            if let Some(old) = cancellers.insert(name.to_string(), cancel.clone()) {
                old.cancel();
            }
        }

        let connections = Arc::clone(&self.connections);
        let state_tx = self.state_tx.clone();
        let name = name.to_string();
        let cancellers = Arc::clone(&self.reconnect_cancellers);

        tokio::spawn(async move {
            for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
                if cancel.is_cancelled() || connector.is_server_disabled(&name) {
                    break;
                }

                // Update state to pending with attempt info
                let conn = ManagedConnection {
                    name: name.clone(),
                    state: ConnectionState::Pending {
                        reconnect_attempt: Some(attempt),
                        max_reconnect_attempts: Some(MAX_RECONNECT_ATTEMPTS),
                    },
                    config: config.clone(),
                    tools: Vec::new(),
                    commands: Vec::new(),
                    resources: Vec::new(),
                };
                connections.write().await.insert(name.clone(), conn);
                let _ = state_tx.send(ConnectionStateChange {
                    server_name: name.clone(),
                    new_state: ConnectionState::Pending {
                        reconnect_attempt: Some(attempt),
                        max_reconnect_attempts: Some(MAX_RECONNECT_ATTEMPTS),
                    },
                });

                match connector.connect(&name, &config).await {
                    Ok(result) if result.state == ConnectionState::Connected => {
                        let conn = ManagedConnection {
                            name: name.clone(),
                            state: ConnectionState::Connected,
                            config: config.clone(),
                            tools: result.tools,
                            commands: result.commands,
                            resources: result.resources,
                        };
                        connections.write().await.insert(name.clone(), conn);
                        let _ = state_tx.send(ConnectionStateChange {
                            server_name: name.clone(),
                            new_state: ConnectionState::Connected,
                        });
                        cancellers.lock().await.remove(&name);
                        return;
                    }
                    _ => {
                        if attempt == MAX_RECONNECT_ATTEMPTS {
                            let conn = ManagedConnection {
                                name: name.clone(),
                                state: ConnectionState::Failed,
                                config: config.clone(),
                                tools: Vec::new(),
                                commands: Vec::new(),
                                resources: Vec::new(),
                            };
                            connections.write().await.insert(name.clone(), conn);
                            let _ = state_tx.send(ConnectionStateChange {
                                server_name: name.clone(),
                                new_state: ConnectionState::Failed,
                            });
                            cancellers.lock().await.remove(&name);
                            return;
                        }
                    }
                }

                // Exponential backoff
                let backoff =
                    std::cmp::min(INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1), MAX_BACKOFF_MS);

                tokio::select! {
                    _ = sleep(Duration::from_millis(backoff)) => {}
                    _ = cancel.cancelled() => { return; }
                }
            }
        });
    }

    /// Toggle a server enabled/disabled.
    pub async fn toggle_server(
        &self,
        name: &str,
        connector: &dyn McpConnector,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = self.get_connection(name).await;
        match conn {
            Some(c) => {
                if c.state == ConnectionState::Disabled {
                    // Enable and reconnect
                    self.reconnect_server(name, connector).await
                } else {
                    // Disable
                    connector.disconnect(name, &c.config).await;
                    self.update_server(ManagedConnection {
                        name: name.to_string(),
                        state: ConnectionState::Disabled,
                        config: c.config,
                        tools: Vec::new(),
                        commands: Vec::new(),
                        resources: Vec::new(),
                    })
                    .await;
                    Ok(())
                }
            }
            None => Err(format!("Server {} not found", name).into()),
        }
    }
}

/// `useManageMCPConnections(dynamicMcpConfig, isStrictMcpConfig)` — entry-point
/// React hook returning `{ reconnectMcpServer, toggleMcpServer }`. Returns a
/// fresh `McpConnectionState`; `dynamic_mcp_config` and `is_strict_mcp_config`
/// are accepted for parity with the TS API.
pub fn use_manage_mcp_connections(
    _dynamic_mcp_config: std::collections::HashMap<
        String,
        crate::mcp::types::ScopedMcpServerConfig,
    >,
    _is_strict_mcp_config: bool,
) -> McpConnectionState {
    McpConnectionState::new(false)
}
