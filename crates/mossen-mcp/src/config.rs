//! MCP 服务器配置
//!
//! 定义 MCP 服务器的各种配置格式（stdio, SSE, HTTP, WebSocket, SDK 等），
//! 以及配置加载、保存、作用域管理等功能。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ─── 配置作用域 ──────────────────────────────────────────────────────────────

/// MCP 服务器配置的作用域
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    /// 项目级别（.mcp.json）
    Local,
    /// 用户级别（全局配置）
    User,
    /// 项目 settings 级别
    Project,
    /// 动态（运行时添加）
    Dynamic,
    /// 企业级别（managed 配置）
    Enterprise,
    /// 托管级别（hosted）
    Hosted,
    /// 受管级别
    Managed,
}

// ─── 传输类型 ─────────────────────────────────────────────────────────────────

/// MCP 支持的传输协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// 标准输入/输出
    Stdio,
    /// Server-Sent Events
    #[serde(rename = "sse")]
    Sse,
    /// IDE 内部 SSE
    #[serde(rename = "sse-ide")]
    SseIde,
    /// Streamable HTTP
    Http,
    /// WebSocket
    Ws,
    /// IDE 内部 WebSocket
    #[serde(rename = "ws-ide")]
    WsIde,
    /// SDK 内部传输
    Sdk,
    /// 托管代理
    #[serde(rename = "hosted-proxy")]
    HostedProxy,
}

// ─── OAuth 配置 ──────────────────────────────────────────────────────────────

/// MCP OAuth 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    /// 客户端 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// OAuth 回调端口
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    /// 授权服务器元数据 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_server_metadata_url: Option<String>,
    /// 是否启用 XAA（跨应用访问）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xaa: Option<bool>,
}

// ─── 服务器配置变体 ──────────────────────────────────────────────────────────

/// stdio 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioServerConfig {
    /// 传输类型标记
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// 启动命令
    pub command: String,
    /// 命令参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 环境变量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// SSE 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SseServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// 静态请求头
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// 动态请求头脚本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
    /// OAuth 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<McpOAuthConfig>,
}

/// SSE-IDE 传输配置（IDE 内部用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SseIdeServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// IDE 名称
    pub ide_name: String,
    /// 是否在 Windows 中运行
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide_running_in_windows: Option<bool>,
}

/// WebSocket-IDE 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsIdeServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// IDE 名称
    pub ide_name: String,
    /// 认证令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// 是否在 Windows 中运行
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide_running_in_windows: Option<bool>,
}

/// HTTP 传输配置（Streamable HTTP）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// 静态请求头
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// 动态请求头脚本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
    /// OAuth 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<McpOAuthConfig>,
}

/// WebSocket 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// 静态请求头
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// 动态请求头脚本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
}

/// SDK 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器名称
    pub name: String,
}

/// 托管代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedProxyServerConfig {
    /// 传输类型标记
    #[serde(rename = "type")]
    pub transport_type: String,
    /// 服务器 URL
    pub url: String,
    /// 服务器 ID
    pub id: String,
}

/// MCP 服务器配置联合类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerConfig {
    /// stdio 传输
    #[serde(rename = "stdio")]
    Stdio(StdioServerConfig),
    /// SSE 传输
    #[serde(rename = "sse")]
    Sse(SseServerConfig),
    /// IDE SSE 传输
    #[serde(rename = "sse-ide")]
    SseIde(SseIdeServerConfig),
    /// IDE WebSocket 传输
    #[serde(rename = "ws-ide")]
    WsIde(WsIdeServerConfig),
    /// HTTP 传输
    #[serde(rename = "http")]
    Http(HttpServerConfig),
    /// WebSocket 传输
    #[serde(rename = "ws")]
    Ws(WsServerConfig),
    /// SDK 传输
    #[serde(rename = "sdk")]
    Sdk(SdkServerConfig),
    /// 托管代理
    #[serde(rename = "hosted-proxy")]
    HostedProxy(HostedProxyServerConfig),
}

/// 带作用域的 MCP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedMcpServerConfig {
    /// 服务器配置
    #[serde(flatten)]
    pub config: McpServerConfig,
    /// 配置作用域
    pub scope: ConfigScope,
    /// 来源插件标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_source: Option<String>,
}

// ─── MCP JSON 配置文件 ───────────────────────────────────────────────────────

/// `.mcp.json` 文件格式
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpJsonConfig {
    /// 服务器配置映射
    #[serde(default)]
    pub mcp_servers: HashMap<String, serde_json::Value>,
}

// ─── 配置操作 ────────────────────────────────────────────────────────────────

/// 获取企业级 MCP 配置文件路径
pub fn get_enterprise_mcp_file_path(managed_dir: &Path) -> PathBuf {
    managed_dir.join("managed-mcp.json")
}

/// 获取项目级 MCP 配置文件路径
pub fn get_project_mcp_file_path(cwd: &Path) -> PathBuf {
    cwd.join(".mcp.json")
}

/// 为服务器配置添加作用域
pub fn add_scope_to_servers(
    servers: &HashMap<String, serde_json::Value>,
    scope: ConfigScope,
) -> HashMap<String, ScopedMcpServerConfig> {
    let mut scoped = HashMap::new();
    for (name, value) in servers {
        if let Ok(config) = serde_json::from_value::<McpServerConfig>(value.clone()) {
            scoped.insert(
                name.clone(),
                ScopedMcpServerConfig {
                    config,
                    scope,
                    plugin_source: None,
                },
            );
        }
    }
    scoped
}

/// 加载并合并所有 MCP 服务器配置
///
/// 按优先级从低到高合并：enterprise < user < project < local < dynamic
pub async fn load_merged_configs(
    cwd: &Path,
    global_config_dir: &Path,
) -> anyhow::Result<HashMap<String, ScopedMcpServerConfig>> {
    let mut merged: HashMap<String, ScopedMcpServerConfig> = HashMap::new();

    // 加载全局用户配置
    let user_config_path = global_config_dir.join("mcp.json");
    if let Ok(contents) = tokio::fs::read_to_string(&user_config_path).await {
        if let Ok(config) = serde_json::from_str::<McpJsonConfig>(&contents) {
            let scoped = add_scope_to_servers(&config.mcp_servers, ConfigScope::User);
            merged.extend(scoped);
        }
    }

    // 加载项目级配置
    let project_config_path = get_project_mcp_file_path(cwd);
    if let Ok(contents) = tokio::fs::read_to_string(&project_config_path).await {
        if let Ok(config) = serde_json::from_str::<McpJsonConfig>(&contents) {
            let scoped = add_scope_to_servers(&config.mcp_servers, ConfigScope::Local);
            merged.extend(scoped);
        }
    }

    Ok(merged)
}

/// 保存 MCP 服务器配置到项目文件
pub async fn save_project_mcp_config(cwd: &Path, config: &McpJsonConfig) -> anyhow::Result<()> {
    let path = get_project_mcp_file_path(cwd);
    let contents = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&path, contents).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn load_merged_configs_surfaces_user_and_project_scopes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cwd = temp.path().join("project");
        let global = temp.path().join("global");
        tokio::fs::create_dir_all(&cwd).await.expect("project dir");
        tokio::fs::create_dir_all(&global)
            .await
            .expect("global dir");

        tokio::fs::write(
            global.join("mcp.json"),
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "m34_user_server": {
                        "type": "stdio",
                        "command": "python3",
                        "args": ["user.py"]
                    },
                    "m34_overlap": {
                        "type": "stdio",
                        "command": "python3",
                        "args": ["user-overlap.py"]
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .expect("write user config");

        tokio::fs::write(
            cwd.join(".mcp.json"),
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "m34_project_server": {
                        "type": "stdio",
                        "command": "python3",
                        "args": ["project.py"]
                    },
                    "m34_overlap": {
                        "type": "stdio",
                        "command": "python3",
                        "args": ["project-overlap.py"]
                    }
                }
            }))
            .unwrap(),
        )
        .await
        .expect("write project config");

        let merged = load_merged_configs(&cwd, &global)
            .await
            .expect("merged configs load");

        assert_eq!(
            merged.get("m34_user_server").map(|cfg| cfg.scope),
            Some(ConfigScope::User)
        );
        assert_eq!(
            merged.get("m34_project_server").map(|cfg| cfg.scope),
            Some(ConfigScope::Local)
        );
        assert_eq!(
            merged.get("m34_overlap").map(|cfg| cfg.scope),
            Some(ConfigScope::Local)
        );
    }

    #[test]
    fn add_scope_to_servers_ignores_invalid_server_configs() {
        let servers = HashMap::from([
            (
                "valid".to_string(),
                json!({
                    "type": "stdio",
                    "command": "python3",
                    "args": ["server.py"]
                }),
            ),
            (
                "invalid".to_string(),
                json!({
                    "type": "stdio",
                    "args": ["missing-command.py"]
                }),
            ),
        ]);

        let scoped = add_scope_to_servers(&servers, ConfigScope::Local);

        assert!(scoped.contains_key("valid"));
        assert!(!scoped.contains_key("invalid"));
    }
}
