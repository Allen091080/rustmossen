//! MCP 服务器自动发现
//!
//! 实现 MCP 服务器的自动发现机制：
//! - 从项目 `.mcp.json` 文件
//! - 从用户全局配置
//! - 从企业受管配置
//! - 从插件提供
//! - 从官方注册表

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::sync::RwLock;
use tracing;

use crate::config::{
    add_scope_to_servers, get_enterprise_mcp_file_path, get_project_mcp_file_path, ConfigScope,
    McpJsonConfig, ScopedMcpServerConfig,
};

// ─── 发现器 ──────────────────────────────────────────────────────────────────

/// MCP 服务器发现器
pub struct McpDiscovery {
    /// 当前工作目录
    cwd: PathBuf,
    /// 全局配置目录
    global_config_dir: PathBuf,
    /// 受管配置目录
    managed_dir: Option<PathBuf>,
    /// 已发现的服务器
    discovered: RwLock<HashMap<String, ScopedMcpServerConfig>>,
}

impl McpDiscovery {
    /// 创建新的发现器
    pub fn new(cwd: PathBuf, global_config_dir: PathBuf) -> Self {
        Self {
            cwd,
            global_config_dir,
            managed_dir: None,
            discovered: RwLock::new(HashMap::new()),
        }
    }

    /// 设置受管配置目录
    pub fn with_managed_dir(mut self, dir: PathBuf) -> Self {
        self.managed_dir = Some(dir);
        self
    }

    /// 执行完整发现流程
    pub async fn discover_all(&self) -> HashMap<String, ScopedMcpServerConfig> {
        let mut all_configs: HashMap<String, ScopedMcpServerConfig> = HashMap::new();

        // 1. 企业受管配置（最低优先级）
        if let Some(managed_dir) = &self.managed_dir {
            if let Ok(configs) = self.load_enterprise_configs(managed_dir).await {
                all_configs.extend(configs);
            }
        }

        // 2. 用户全局配置
        if let Ok(configs) = self.load_user_configs().await {
            all_configs.extend(configs);
        }

        // 3. 项目级配置（最高优先级）
        if let Ok(configs) = self.load_project_configs().await {
            all_configs.extend(configs);
        }

        // 更新缓存
        *self.discovered.write().await = all_configs.clone();
        all_configs
    }

    /// 获取已发现的配置（缓存）
    pub async fn get_discovered(&self) -> HashMap<String, ScopedMcpServerConfig> {
        self.discovered.read().await.clone()
    }

    /// 按名称查找配置
    pub async fn get_config_by_name(&self, name: &str) -> Option<ScopedMcpServerConfig> {
        self.discovered.read().await.get(name).cloned()
    }

    /// 检查指定名称的服务器是否存在
    pub async fn has_server(&self, name: &str) -> bool {
        self.discovered.read().await.contains_key(name)
    }

    // ─── 内部方法 ────────────────────────────────────────────────────────────

    /// 加载企业受管配置
    async fn load_enterprise_configs(
        &self,
        managed_dir: &Path,
    ) -> anyhow::Result<HashMap<String, ScopedMcpServerConfig>> {
        let path = get_enterprise_mcp_file_path(managed_dir);
        let contents = tokio::fs::read_to_string(&path).await?;
        let config: McpJsonConfig = serde_json::from_str(&contents)?;
        Ok(add_scope_to_servers(
            &config.mcp_servers,
            ConfigScope::Enterprise,
        ))
    }

    /// 加载用户全局配置
    async fn load_user_configs(&self) -> anyhow::Result<HashMap<String, ScopedMcpServerConfig>> {
        let path = self.global_config_dir.join("mcp.json");
        let contents = tokio::fs::read_to_string(&path).await?;
        let config: McpJsonConfig = serde_json::from_str(&contents)?;
        Ok(add_scope_to_servers(&config.mcp_servers, ConfigScope::User))
    }

    /// 加载项目级配置
    async fn load_project_configs(&self) -> anyhow::Result<HashMap<String, ScopedMcpServerConfig>> {
        let path = get_project_mcp_file_path(&self.cwd);
        let contents = tokio::fs::read_to_string(&path).await?;
        let config: McpJsonConfig = serde_json::from_str(&contents)?;
        Ok(add_scope_to_servers(
            &config.mcp_servers,
            ConfigScope::Local,
        ))
    }
}

// ─── 官方注册表 ──────────────────────────────────────────────────────────────

/// MCP 官方服务器注册表
pub struct OfficialRegistry {
    /// 已知的官方 URL 集合
    official_urls: RwLock<Option<std::collections::HashSet<String>>>,
    /// 注册表 URL
    registry_url: String,
}

impl OfficialRegistry {
    /// 创建新的注册表客户端
    pub fn new(remote_base_url: &str) -> Self {
        Self {
            official_urls: RwLock::new(None),
            registry_url: format!(
                "{}/mcp-registry/v0/servers?version=latest&visibility=commercial",
                remote_base_url
            ),
        }
    }

    /// 预取官方 MCP URL
    pub async fn prefetch(&self) {
        let client = reqwest::Client::new();
        match client
            .get(&self.registry_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(body) = response.json::<RegistryResponse>().await {
                    let mut urls = std::collections::HashSet::new();
                    for entry in body.servers {
                        if let Some(remotes) = entry.server.remotes {
                            for remote in remotes {
                                if let Some(normalized) = normalize_url(&remote.url) {
                                    urls.insert(normalized);
                                }
                            }
                        }
                    }
                    tracing::debug!("[mcp-registry] Loaded {} official MCP URLs", urls.len());
                    *self.official_urls.write().await = Some(urls);
                }
            }
            Err(e) => {
                tracing::debug!("Failed to fetch MCP registry: {}", e);
            }
        }
    }

    /// 检查 URL 是否为官方注册的
    pub async fn is_official_url(&self, normalized_url: &str) -> bool {
        self.official_urls
            .read()
            .await
            .as_ref()
            .map(|urls| urls.contains(normalized_url))
            .unwrap_or(false)
    }
}

// ─── 注册表响应类型 ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegistryResponse {
    servers: Vec<RegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    server: RegistryServer,
}

#[derive(Debug, Deserialize)]
struct RegistryServer {
    remotes: Option<Vec<RegistryRemote>>,
}

#[derive(Debug, Deserialize)]
struct RegistryRemote {
    url: String,
}

/// 规范化 URL（去除查询参数和尾部斜杠）
fn normalize_url(url: &str) -> Option<String> {
    // 简化实现：去除查询参数和尾部斜杠
    let base = url.split('?').next().unwrap_or(url);
    let trimmed = base.trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}
