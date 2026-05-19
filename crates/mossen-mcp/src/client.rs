//! MCP 客户端连接
//!
//! 实现 MCP 客户端的核心逻辑：初始化握手、能力协商、
//! 工具/资源/Prompt 列表获取、工具调用转发等。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;
use tokio::sync::RwLock;

use crate::config::ScopedMcpServerConfig;
use crate::protocol::*;
use crate::transport::McpTransport;

// ─── MCP 客户端 ──────────────────────────────────────────────────────────────

/// MCP 客户端——管理与单个 MCP 服务器的连接
pub struct McpClient {
    /// 传输层
    transport: Box<dyn McpTransport>,
    /// 服务端能力
    server_capabilities: RwLock<Option<ServerCapabilities>>,
    /// 服务端信息
    server_info: RwLock<Option<Implementation>>,
    /// 服务端指令
    instructions: RwLock<Option<String>>,
    /// 请求 ID 计数器
    next_id: AtomicI64,
    /// 待处理请求
    pending_requests: dashmap::DashMap<RequestId, tokio::sync::oneshot::Sender<JsonRpcResponse>>,
    /// 客户端信息
    client_info: Implementation,
}

impl McpClient {
    /// 创建新的 MCP 客户端
    pub fn new(transport: Box<dyn McpTransport>, client_info: Implementation) -> Self {
        Self {
            transport,
            server_capabilities: RwLock::new(None),
            server_info: RwLock::new(None),
            instructions: RwLock::new(None),
            next_id: AtomicI64::new(1),
            pending_requests: dashmap::DashMap::new(),
            client_info,
        }
    }

    /// 执行初始化握手
    pub async fn initialize(&self) -> anyhow::Result<InitializeResult> {
        let params = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {
                roots: Some(RootsCapability {
                    list_changed: Some(true),
                }),
                sampling: None,
                elicitation: Some(ElicitationCapability {}),
                experimental: None,
            },
            client_info: self.client_info.clone(),
        };

        let response = self
            .send_request(methods::INITIALIZE, Some(serde_json::to_value(&params)?))
            .await?;

        let result: InitializeResult = serde_json::from_value(response)?;

        // 保存服务端信息
        *self.server_capabilities.write().await = Some(result.capabilities.clone());
        *self.server_info.write().await = result.server_info.clone();
        *self.instructions.write().await = result.instructions.clone();

        // 发送 initialized 通知
        self.send_notification(methods::INITIALIZED, None).await?;

        Ok(result)
    }

    /// 列出可用工具
    pub async fn list_tools(&self) -> anyhow::Result<ListToolsResult> {
        let response = self.send_request(methods::LIST_TOOLS, None).await?;
        let result: ListToolsResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 调用工具
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<Value>,
    ) -> anyhow::Result<CallToolResult> {
        let params = CallToolParams {
            name: name.to_string(),
            arguments,
        };
        let response = self
            .send_request(methods::CALL_TOOL, Some(serde_json::to_value(&params)?))
            .await?;
        let result: CallToolResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 列出可用资源
    pub async fn list_resources(&self) -> anyhow::Result<ListResourcesResult> {
        let response = self.send_request(methods::LIST_RESOURCES, None).await?;
        let result: ListResourcesResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 读取资源
    pub async fn read_resource(&self, uri: &str) -> anyhow::Result<ReadResourceResult> {
        let params = ReadResourceParams {
            uri: uri.to_string(),
        };
        let response = self
            .send_request(methods::READ_RESOURCE, Some(serde_json::to_value(&params)?))
            .await?;
        let result: ReadResourceResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 列出可用 Prompt
    pub async fn list_prompts(&self) -> anyhow::Result<ListPromptsResult> {
        let response = self.send_request(methods::LIST_PROMPTS, None).await?;
        let result: ListPromptsResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 获取 Prompt
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> anyhow::Result<GetPromptResult> {
        let params = GetPromptParams {
            name: name.to_string(),
            arguments,
        };
        let response = self
            .send_request(methods::GET_PROMPT, Some(serde_json::to_value(&params)?))
            .await?;
        let result: GetPromptResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// 发送 ping
    pub async fn ping(&self) -> anyhow::Result<()> {
        let _ = self.send_request(methods::PING, None).await?;
        Ok(())
    }

    /// 获取服务端能力
    pub async fn capabilities(&self) -> Option<ServerCapabilities> {
        self.server_capabilities.read().await.clone()
    }

    /// 获取服务端信息
    pub async fn server_info(&self) -> Option<Implementation> {
        self.server_info.read().await.clone()
    }

    /// 获取服务端指令
    pub async fn instructions(&self) -> Option<String> {
        self.instructions.read().await.clone()
    }

    /// 关闭客户端
    pub async fn close(&self) -> anyhow::Result<()> {
        self.transport.close().await
    }

    // ─── 内部方法 ────────────────────────────────────────────────────────────

    /// 发送请求并等待响应
    async fn send_request(&self, method: &str, params: Option<Value>) -> anyhow::Result<Value> {
        let id = RequestId::Number(self.next_id.fetch_add(1, Ordering::SeqCst));

        let request = JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: id.clone(),
            method: method.to_string(),
            params,
        });

        // 创建 oneshot 通道等待响应
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_requests.insert(id.clone(), tx);

        // 发送请求
        self.transport.send(&request).await?;

        // 等待响应（带超时）
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| anyhow::anyhow!("MCP request timed out: {}", method))?
            .map_err(|_| anyhow::anyhow!("MCP response channel dropped"))?;

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error ({}): {}",
                error.code,
                error.message
            ));
        }

        response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))
    }

    /// 发送通知（无需响应）
    async fn send_notification(&self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
        let notification = JsonRpcMessage::Notification(JsonRpcNotification {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: method.to_string(),
            params,
        });
        self.transport.send(&notification).await
    }
}

// ─── 连接状态 ────────────────────────────────────────────────────────────────

/// MCP 服务器连接状态
#[derive(Debug, Clone)]
pub enum McpServerConnection {
    /// 已连接
    Connected(ConnectedServer),
    /// 连接失败
    Failed(FailedServer),
    /// 需要认证
    NeedsAuth(NeedsAuthServer),
    /// 连接中
    Pending(PendingServer),
    /// 已禁用
    Disabled(DisabledServer),
}

/// 已连接的服务器
#[derive(Debug, Clone)]
pub struct ConnectedServer {
    /// 服务器名称
    pub name: String,
    /// 服务端能力
    pub capabilities: ServerCapabilities,
    /// 服务端信息
    pub server_info: Option<Implementation>,
    /// 服务端指令
    pub instructions: Option<String>,
    /// 配置
    pub config: ScopedMcpServerConfig,
}

/// 连接失败的服务器
#[derive(Debug, Clone)]
pub struct FailedServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
    pub error: Option<String>,
}

/// 需要认证的服务器
#[derive(Debug, Clone)]
pub struct NeedsAuthServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
}

/// 连接中的服务器
#[derive(Debug, Clone)]
pub struct PendingServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
    pub reconnect_attempt: Option<u32>,
    pub max_reconnect_attempts: Option<u32>,
}

/// 已禁用的服务器
#[derive(Debug, Clone)]
pub struct DisabledServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
}

impl McpServerConnection {
    /// 获取服务器名称
    pub fn name(&self) -> &str {
        match self {
            Self::Connected(s) => &s.name,
            Self::Failed(s) => &s.name,
            Self::NeedsAuth(s) => &s.name,
            Self::Pending(s) => &s.name,
            Self::Disabled(s) => &s.name,
        }
    }

    /// 是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected(_))
    }

    /// 获取连接状态字符串
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Connected(_) => "connected",
            Self::Failed(_) => "failed",
            Self::NeedsAuth(_) => "needs-auth",
            Self::Pending(_) => "pending",
            Self::Disabled(_) => "disabled",
        }
    }
}

// ─── 序列化状态（用于 CLI 交互）────────────────────────────────────────────────

/// 序列化的工具信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedTool {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mcp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_tool_name: Option<String>,
}

/// 序列化的客户端信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SerializedClient {
    pub name: String,
    #[serde(rename = "type")]
    pub connection_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ServerCapabilities>,
}

/// MCP CLI 状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCliState {
    pub clients: Vec<SerializedClient>,
    pub configs: HashMap<String, ScopedMcpServerConfig>,
    pub tools: Vec<SerializedTool>,
    pub resources: HashMap<String, Vec<ServerResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_names: Option<HashMap<String, String>>,
}

/// 带服务器归属的资源
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerResource {
    #[serde(flatten)]
    pub resource: Resource,
    pub server: String,
}
