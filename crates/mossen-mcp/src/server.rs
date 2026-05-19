//! MCP 服务器发现与生命周期管理
//!
//! 管理多个 MCP 服务器连接的创建、维护、重连与清理。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing;

/// Process-wide singleton holding the active `McpServerManager`. The CLI
/// launcher installs it once after `connect_all()` so cross-crate consumers
/// (notably `mossen-tools::mcp_tool`) can resolve a server client without
/// taking a circular dependency on `mossen-cli`.
static GLOBAL_MANAGER: OnceLock<Arc<McpServerManager>> = OnceLock::new();

/// Install the process-global manager. Idempotent — first writer wins, later
/// calls are ignored (matching `OnceLock` semantics).
pub fn set_global_manager(manager: Arc<McpServerManager>) {
    let _ = GLOBAL_MANAGER.set(manager);
}

/// Fetch the process-global manager, if one was installed.
pub fn global_manager() -> Option<Arc<McpServerManager>> {
    GLOBAL_MANAGER.get().cloned()
}

use crate::client::{
    ConnectedServer, DisabledServer, FailedServer, McpClient, McpServerConnection, PendingServer,
    ServerResource,
};
use crate::config::{McpServerConfig, ScopedMcpServerConfig};
use crate::protocol::Implementation;
use crate::transport::{HttpTransport, SseTransport, StdioTransport, WsTransport};

// ─── 服务器管理器 ────────────────────────────────────────────────────────────

/// MCP 服务器连接管理器
///
/// 负责所有 MCP 服务器的连接生命周期管理
pub struct McpServerManager {
    /// 活跃连接映射
    connections: DashMap<String, McpServerConnection>,
    /// 活跃客户端（仅 Connected 状态）
    clients: DashMap<String, Arc<McpClient>>,
    /// 服务器配置
    configs: RwLock<HashMap<String, ScopedMcpServerConfig>>,
    /// 客户端信息
    client_info: Implementation,
    /// 最大重连尝试次数
    max_reconnect_attempts: u32,
}

impl McpServerManager {
    /// 创建新的服务器管理器
    pub fn new(client_info: Implementation) -> Self {
        Self {
            connections: DashMap::new(),
            clients: DashMap::new(),
            configs: RwLock::new(HashMap::new()),
            client_info,
            max_reconnect_attempts: 3,
        }
    }

    /// 设置最大重连尝试次数
    pub fn with_max_reconnect_attempts(mut self, max: u32) -> Self {
        self.max_reconnect_attempts = max;
        self
    }

    /// 更新服务器配置列表
    pub async fn update_configs(&self, configs: HashMap<String, ScopedMcpServerConfig>) {
        let mut stored = self.configs.write().await;
        *stored = configs;
    }

    /// 连接到指定服务器
    pub async fn connect_server(&self, name: &str) -> anyhow::Result<()> {
        let configs = self.configs.read().await;
        let config = configs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in configs", name))?
            .clone();
        drop(configs);

        // 设为 Pending 状态
        self.connections.insert(
            name.to_string(),
            McpServerConnection::Pending(PendingServer {
                name: name.to_string(),
                config: config.clone(),
                reconnect_attempt: Some(0),
                max_reconnect_attempts: Some(self.max_reconnect_attempts),
            }),
        );

        match self.create_client(&config).await {
            Ok(client) => {
                // 执行初始化
                match client.initialize().await {
                    Ok(result) => {
                        let connected = ConnectedServer {
                            name: name.to_string(),
                            capabilities: result.capabilities,
                            server_info: result.server_info,
                            instructions: result.instructions,
                            config,
                        };
                        self.connections
                            .insert(name.to_string(), McpServerConnection::Connected(connected));
                        self.clients.insert(name.to_string(), Arc::new(client));
                        tracing::info!("MCP server '{}' connected successfully", name);
                        Ok(())
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        tracing::error!("MCP server '{}' initialization failed: {}", name, err_msg);
                        self.connections.insert(
                            name.to_string(),
                            McpServerConnection::Failed(FailedServer {
                                name: name.to_string(),
                                config,
                                error: Some(err_msg),
                            }),
                        );
                        Err(e)
                    }
                }
            }
            Err(e) => {
                let err_msg = e.to_string();
                tracing::error!(
                    "MCP server '{}' transport creation failed: {}",
                    name,
                    err_msg
                );
                self.connections.insert(
                    name.to_string(),
                    McpServerConnection::Failed(FailedServer {
                        name: name.to_string(),
                        config,
                        error: Some(err_msg),
                    }),
                );
                Err(e)
            }
        }
    }

    /// 连接所有已配置的服务器
    pub async fn connect_all(&self) {
        let configs = self.configs.read().await;
        let names: Vec<String> = configs.keys().cloned().collect();
        drop(configs);

        for name in names {
            let _ = self.connect_server(&name).await;
        }
    }

    /// 断开指定服务器连接
    pub async fn disconnect_server(&self, name: &str) -> anyhow::Result<()> {
        if let Some((_, client)) = self.clients.remove(name) {
            client.close().await?;
        }
        self.connections.remove(name);
        Ok(())
    }

    /// 断开所有服务器连接
    pub async fn disconnect_all(&self) {
        let names: Vec<String> = self.clients.iter().map(|e| e.key().clone()).collect();
        for name in names {
            let _ = self.disconnect_server(&name).await;
        }
    }

    /// 禁用指定服务器
    pub fn disable_server(&self, name: &str) {
        if let Some(entry) = self.connections.get(name) {
            let config = match entry.value() {
                McpServerConnection::Connected(s) => s.config.clone(),
                McpServerConnection::Failed(s) => s.config.clone(),
                McpServerConnection::NeedsAuth(s) => s.config.clone(),
                McpServerConnection::Pending(s) => s.config.clone(),
                McpServerConnection::Disabled(s) => s.config.clone(),
            };
            drop(entry);
            self.connections.insert(
                name.to_string(),
                McpServerConnection::Disabled(DisabledServer {
                    name: name.to_string(),
                    config,
                }),
            );
            self.clients.remove(name);
        }
    }

    /// 获取指定服务器的客户端
    pub fn get_client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.get(name).map(|entry| entry.value().clone())
    }

    /// 获取所有连接状态
    pub fn get_all_connections(&self) -> Vec<McpServerConnection> {
        self.connections
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// 获取所有已连接服务器的工具
    pub async fn get_all_tools(&self) -> Vec<(String, Vec<crate::protocol::ToolDefinition>)> {
        let mut results = Vec::new();
        for entry in self.clients.iter() {
            let name = entry.key().clone();
            let client = entry.value().clone();
            if let Ok(tools_result) = client.list_tools().await {
                results.push((name, tools_result.tools));
            }
        }
        results
    }

    /// 获取所有已连接服务器的资源
    pub async fn get_all_resources(&self) -> HashMap<String, Vec<ServerResource>> {
        let mut results: HashMap<String, Vec<ServerResource>> = HashMap::new();
        for entry in self.clients.iter() {
            let name = entry.key().clone();
            let client = entry.value().clone();
            if let Ok(resources_result) = client.list_resources().await {
                let server_resources: Vec<ServerResource> = resources_result
                    .resources
                    .into_iter()
                    .map(|r| ServerResource {
                        resource: r,
                        server: name.clone(),
                    })
                    .collect();
                results.insert(name, server_resources);
            }
        }
        results
    }

    /// 获取已连接服务器数量
    pub fn connected_count(&self) -> usize {
        self.connections
            .iter()
            .filter(|e| e.value().is_connected())
            .count()
    }

    /// 获取总服务器数量
    pub fn total_count(&self) -> usize {
        self.connections.len()
    }

    // ─── 内部方法 ────────────────────────────────────────────────────────────

    /// 根据配置创建传输层并构建客户端
    async fn create_client(&self, config: &ScopedMcpServerConfig) -> anyhow::Result<McpClient> {
        let transport: Box<dyn crate::transport::McpTransport> = match &config.config {
            McpServerConfig::Stdio(cfg) => {
                let t = StdioTransport::spawn(&cfg.command, &cfg.args, cfg.env.as_ref()).await?;
                Box::new(t)
            }
            McpServerConfig::Sse(cfg) => {
                let headers = cfg.headers.clone().unwrap_or_default();
                let t = SseTransport::connect(&cfg.url, headers).await?;
                Box::new(t)
            }
            McpServerConfig::Http(cfg) => {
                let headers = cfg.headers.clone().unwrap_or_default();
                let t = HttpTransport::new(&cfg.url, headers);
                Box::new(t)
            }
            McpServerConfig::Ws(cfg) => {
                let headers = cfg.headers.clone().unwrap_or_default();
                let t = WsTransport::connect(&cfg.url, headers).await?;
                Box::new(t)
            }
            McpServerConfig::SseIde(cfg) => {
                let t = SseTransport::connect(&cfg.url, HashMap::new()).await?;
                Box::new(t)
            }
            McpServerConfig::WsIde(cfg) => {
                let mut headers = HashMap::new();
                if let Some(token) = &cfg.auth_token {
                    headers.insert("Authorization".to_string(), format!("Bearer {}", token));
                }
                let t = WsTransport::connect(&cfg.url, headers).await?;
                Box::new(t)
            }
            McpServerConfig::Sdk(_) => {
                return Err(anyhow::anyhow!(
                    "SDK transport not supported in standalone mode"
                ));
            }
            McpServerConfig::HostedProxy(cfg) => {
                let t = HttpTransport::new(&cfg.url, HashMap::new());
                Box::new(t)
            }
        };

        Ok(McpClient::new(transport, self.client_info.clone()))
    }
}
