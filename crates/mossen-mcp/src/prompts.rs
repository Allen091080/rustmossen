//! MCP Prompt 模板
//!
//! 管理 MCP 服务器提供的 Prompt 模板，支持列举和获取。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::client::McpClient;
use crate::normalization::normalize_name_for_mcp;
use crate::protocol::{GetPromptResult, PromptArgument};

// ─── Prompt 管理器 ───────────────────────────────────────────────────────────

/// MCP Prompt 管理器——聚合多个服务器的 Prompt
pub struct McpPromptManager {
    /// 服务器名 → 客户端
    clients: HashMap<String, Arc<McpClient>>,
    /// 缓存的 Prompt 列表
    cached_prompts: HashMap<String, Vec<McpPrompt>>,
}

/// 来自 MCP 服务器的 Prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    /// 完全限定名（mcp__server__prompt 格式）
    pub qualified_name: String,
    /// 原始名称
    pub original_name: String,
    /// 服务器名称
    pub server_name: String,
    /// 描述
    pub description: Option<String>,
    /// 参数定义
    pub arguments: Option<Vec<PromptArgument>>,
}

impl McpPromptManager {
    /// 创建新的 Prompt 管理器
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            cached_prompts: HashMap::new(),
        }
    }

    /// 注册服务器客户端
    pub fn register_client(&mut self, server_name: &str, client: Arc<McpClient>) {
        self.clients.insert(server_name.to_string(), client);
    }

    /// 移除服务器客户端
    pub fn remove_client(&mut self, server_name: &str) {
        self.clients.remove(server_name);
        self.cached_prompts.remove(server_name);
    }

    /// 刷新指定服务器的 Prompt 缓存
    pub async fn refresh_prompts(&mut self, server_name: &str) -> anyhow::Result<()> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?
            .clone();

        let result = client.list_prompts().await?;
        let prompts = result
            .prompts
            .into_iter()
            .map(|p| McpPrompt {
                qualified_name: build_mcp_prompt_name(server_name, &p.name),
                original_name: p.name,
                server_name: server_name.to_string(),
                description: p.description,
                arguments: p.arguments,
            })
            .collect();

        self.cached_prompts.insert(server_name.to_string(), prompts);
        Ok(())
    }

    /// 获取所有已缓存的 Prompt
    pub fn all_prompts(&self) -> Vec<&McpPrompt> {
        self.cached_prompts.values().flatten().collect()
    }

    /// 获取指定服务器的 Prompt
    pub fn prompts_for_server(&self, server_name: &str) -> Vec<&McpPrompt> {
        self.cached_prompts
            .get(server_name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// 获取指定 Prompt 的内容
    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> anyhow::Result<GetPromptResult> {
        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

        client.get_prompt(prompt_name, arguments).await
    }
}

impl Default for McpPromptManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── 名称构建 ────────────────────────────────────────────────────────────────

/// 构建完全限定的 MCP Prompt 名称
fn build_mcp_prompt_name(server_name: &str, prompt_name: &str) -> String {
    format!(
        "mcp__{}__{}",
        normalize_name_for_mcp(server_name),
        normalize_name_for_mcp(prompt_name)
    )
}

// ─── 内置模板 ────────────────────────────────────────────────────────────────

/// 内置 MCP 服务器模板定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinTemplate {
    /// 模板名称
    pub name: String,
    /// 模板描述
    pub description: String,
    /// 服务器配置（JSON 值）
    pub config: serde_json::Value,
    /// 安装说明
    pub install_instructions: Option<String>,
}

/// 获取内置模板列表
pub fn get_builtin_templates() -> Vec<BuiltinTemplate> {
    vec![
        BuiltinTemplate {
            name: "filesystem".to_string(),
            description: "Access to local filesystem".to_string(),
            config: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
            }),
            install_instructions: Some("Requires Node.js and npm/npx".to_string()),
        },
        BuiltinTemplate {
            name: "github".to_string(),
            description: "GitHub API integration".to_string(),
            config: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-github"],
                "env": {
                    "GITHUB_PERSONAL_ACCESS_TOKEN": "${GITHUB_TOKEN}"
                }
            }),
            install_instructions: Some("Requires GITHUB_TOKEN environment variable".to_string()),
        },
        BuiltinTemplate {
            name: "postgres".to_string(),
            description: "PostgreSQL database access".to_string(),
            config: serde_json::json!({
                "type": "stdio",
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-postgres"],
                "env": {
                    "DATABASE_URL": "${DATABASE_URL}"
                }
            }),
            install_instructions: Some("Requires DATABASE_URL environment variable".to_string()),
        },
    ]
}
