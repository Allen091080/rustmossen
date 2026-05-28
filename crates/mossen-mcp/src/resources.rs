//! MCP 资源访问
//!
//! 提供对 MCP 服务器暴露的资源的读取和列举能力。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::client::McpClient;
use crate::protocol::{Resource, ResourceContent};

// ─── 资源管理器 ──────────────────────────────────────────────────────────────

/// MCP 资源管理器——聚合多个服务器的资源
pub struct McpResourceManager {
    /// 服务器名 → 客户端
    clients: HashMap<String, Arc<McpClient>>,
}

impl McpResourceManager {
    /// 创建新的资源管理器
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// 注册服务器客户端
    pub fn register_client(&mut self, server_name: &str, client: Arc<McpClient>) {
        self.clients.insert(server_name.to_string(), client);
    }

    /// 移除服务器客户端
    pub fn remove_client(&mut self, server_name: &str) {
        self.clients.remove(server_name);
    }

    /// 列出指定服务器的资源
    pub async fn list_resources(&self, server_name: &str) -> anyhow::Result<Vec<ServerResource>> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        let result = client.list_resources().await?;
        Ok(result
            .resources
            .into_iter()
            .map(|r| ServerResource {
                resource: r,
                server: server_name.to_string(),
            })
            .collect())
    }

    /// 列出所有服务器的资源
    pub async fn list_all_resources(&self) -> HashMap<String, Vec<ServerResource>> {
        let mut results = HashMap::new();
        for (name, client) in &self.clients {
            if let Ok(result) = client.list_resources().await {
                let resources: Vec<ServerResource> = result
                    .resources
                    .into_iter()
                    .map(|r| ServerResource {
                        resource: r,
                        server: name.clone(),
                    })
                    .collect();
                results.insert(name.clone(), resources);
            }
        }
        results
    }

    /// 读取资源内容
    pub async fn read_resource(
        &self,
        server_name: &str,
        uri: &str,
    ) -> anyhow::Result<Vec<ResourceContent>> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        let result = client.read_resource(uri).await?;
        Ok(result.contents)
    }
}

impl Default for McpResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 资源类型 ────────────────────────────────────────────────────────────────

/// 带服务器归属的资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResource {
    /// 资源信息
    #[serde(flatten)]
    pub resource: Resource,
    /// 所属服务器名称
    pub server: String,
}

/// 资源内容类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceContentType {
    /// 纯文本
    Text,
    /// 二进制 (Base64)
    Binary,
}

/// 判断资源内容类型
pub fn determine_content_type(content: &ResourceContent) -> ResourceContentType {
    if content.blob.is_some() {
        ResourceContentType::Binary
    } else {
        ResourceContentType::Text
    }
}

/// 从资源内容中提取文本
pub fn extract_text(content: &ResourceContent) -> Option<&str> {
    content.text.as_deref()
}

/// 从资源内容中提取 Base64 二进制数据
pub fn extract_blob(content: &ResourceContent) -> Option<&str> {
    content.blob.as_deref()
}
