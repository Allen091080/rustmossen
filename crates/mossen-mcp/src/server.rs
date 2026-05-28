//! MCP 服务器发现与生命周期管理
//!
//! 管理多个 MCP 服务器连接的创建、维护、重连与清理。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock as StdRwLock};

use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing;

/// Process-wide slot holding the active `McpServerManager`. The slot itself is
/// initialized once, but the manager inside it is replaceable so `/reload-plugins`
/// and MCP config reloads can swap in a freshly connected runtime.
static GLOBAL_MANAGER: OnceLock<StdRwLock<Option<Arc<McpServerManager>>>> = OnceLock::new();

fn global_manager_slot() -> &'static StdRwLock<Option<Arc<McpServerManager>>> {
    GLOBAL_MANAGER.get_or_init(|| StdRwLock::new(None))
}

/// Install or replace the process-global manager.
pub fn set_global_manager(manager: Arc<McpServerManager>) {
    if let Ok(mut guard) = global_manager_slot().write() {
        *guard = Some(manager);
    }
}

/// Clear the process-global manager after shutdown or a reload that finds no
/// configured MCP servers.
pub fn clear_global_manager() {
    if let Ok(mut guard) = global_manager_slot().write() {
        *guard = None;
    }
}

/// Fetch the process-global manager, if one was installed.
pub fn global_manager() -> Option<Arc<McpServerManager>> {
    global_manager_slot()
        .read()
        .ok()
        .and_then(|guard| guard.clone())
}

use crate::client::{
    ConnectedServer, DisabledServer, FailedServer, McpClient, McpServerConnection, PendingServer,
    ServerResource,
};
use crate::config::{McpServerConfig, ScopedMcpServerConfig};
use crate::normalization::normalize_name_for_mcp;
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

    /// Resolve a client by the normalized server name embedded in a
    /// model-visible MCP tool name (`mcp__<server>__<tool>`).
    pub fn get_client_by_normalized_name(
        &self,
        normalized_name: &str,
    ) -> Option<(String, Arc<McpClient>)> {
        for entry in self.clients.iter() {
            if normalize_name_for_mcp(entry.key()) == normalized_name {
                return Some((entry.key().clone(), entry.value().clone()));
            }
        }
        None
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

    /// 获取所有已连接服务器的 Prompt。
    pub async fn get_all_prompts(&self) -> Vec<(String, Vec<crate::protocol::PromptDefinition>)> {
        let mut results = Vec::new();
        for entry in self.clients.iter() {
            let name = entry.key().clone();
            let client = entry.value().clone();
            if let Ok(prompts_result) = client.list_prompts().await {
                results.push((name, prompts_result.prompts));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigScope, StdioServerConfig};
    use serde_json::json;
    use std::path::PathBuf;

    fn stdio_config(command: impl Into<String>, args: Vec<String>) -> ScopedMcpServerConfig {
        ScopedMcpServerConfig {
            config: McpServerConfig::Stdio(StdioServerConfig {
                transport_type: Some("stdio".to_string()),
                command: command.into(),
                args,
                env: None,
            }),
            scope: ConfigScope::Local,
            plugin_source: None,
        }
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("crate is under crates/mossen-mcp")
            .to_path_buf()
    }

    fn global_manager_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("global manager test lock poisoned")
    }

    #[test]
    fn global_manager_can_be_replaced_for_reload() {
        let _guard = global_manager_test_lock();
        clear_global_manager();
        let first = Arc::new(McpServerManager::new(Implementation {
            name: "first".to_string(),
            version: "0.0.0".to_string(),
        }));
        let second = Arc::new(McpServerManager::new(Implementation {
            name: "second".to_string(),
            version: "0.0.0".to_string(),
        }));

        set_global_manager(first.clone());
        assert!(Arc::ptr_eq(
            &global_manager().expect("first manager"),
            &first
        ));
        set_global_manager(second.clone());
        assert!(Arc::ptr_eq(
            &global_manager().expect("second manager"),
            &second
        ));
        clear_global_manager();
        assert!(global_manager().is_none());
    }

    #[tokio::test]
    async fn connect_all_keeps_good_server_when_another_server_fails() {
        let mock_server = repo_root().join("scripts/harness_mock_mcp_server.py");
        assert!(mock_server.exists(), "mock MCP server fixture exists");

        let manager = McpServerManager::new(Implementation {
            name: "mossen-test".to_string(),
            version: "0.0.0".to_string(),
        })
        .with_max_reconnect_attempts(1);

        manager
            .update_configs(HashMap::from([
                (
                    "m34_good_server".to_string(),
                    stdio_config("python3", vec![mock_server.to_string_lossy().to_string()]),
                ),
                (
                    "m34_bad_server".to_string(),
                    stdio_config("/bin/this-binary-does-not-exist-M34-zzz", vec![]),
                ),
            ]))
            .await;

        manager.connect_all().await;

        let connections = manager.get_all_connections();
        assert_eq!(manager.total_count(), 2);
        assert_eq!(manager.connected_count(), 1);
        assert!(connections.iter().any(|conn| matches!(
            conn,
            McpServerConnection::Connected(server) if server.name == "m34_good_server"
        )));
        assert!(connections.iter().any(|conn| matches!(
            conn,
            McpServerConnection::Failed(server)
                if server.name == "m34_bad_server"
                    && server.error.as_deref().unwrap_or_default().contains("No such file")
        )));

        let tools = manager.get_all_tools().await;
        assert!(tools.iter().any(|(server, tools)| {
            server == "m34_good_server" && tools.iter().any(|tool| tool.name == "echo_M3_2")
        }));

        let client = manager
            .get_client("m34_good_server")
            .expect("good server remains connected");
        let result = crate::tools::execute_mcp_tool_call(
            &client,
            "echo_M3_2",
            Some(json!({ "text": "M3_2_PAYLOAD_unique_xyz" })),
        )
        .await
        .expect("tool call through connected manager client succeeds");
        assert!(!result.is_error);
        assert!(result.text.contains("ECHO_TAG_FROM_MOCK_MCP"));
        assert!(result.text.contains("M3_2_PAYLOAD_unique_xyz"));

        manager.disconnect_all().await;
    }
}
