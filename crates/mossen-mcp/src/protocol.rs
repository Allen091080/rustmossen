//! MCP 协议消息定义
//!
//! 实现 Model Context Protocol (MCP) 的 JSON-RPC 2.0 消息格式，
//! 包括请求、响应、通知等核心协议类型。

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── JSON-RPC 2.0 基础类型 ───────────────────────────────────────────────────

/// JSON-RPC 2.0 版本标识
pub const JSONRPC_VERSION: &str = "2.0";

/// MCP 协议版本
pub const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

/// JSON-RPC 请求 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// 整数 ID
    Number(i64),
    /// 字符串 ID
    String(String),
}

/// JSON-RPC 2.0 请求消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 响应消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 通知消息（无 id）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 错误对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC 消息联合类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// 请求消息
    Request(JsonRpcRequest),
    /// 响应消息
    Response(JsonRpcResponse),
    /// 通知消息
    Notification(JsonRpcNotification),
}

// ─── MCP 错误码 ──────────────────────────────────────────────────────────────

/// MCP 标准错误码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// JSON-RPC 解析错误
    ParseError = -32700,
    /// 无效请求
    InvalidRequest = -32600,
    /// 方法未找到
    MethodNotFound = -32601,
    /// 无效参数
    InvalidParams = -32602,
    /// 内部错误
    InternalError = -32603,
    /// 请求超时
    RequestTimeout = -32001,
    /// 资源未找到
    ResourceNotFound = -32002,
}

impl ErrorCode {
    /// 获取错误码的数值
    pub fn code(self) -> i32 {
        self as i32
    }
}

// ─── MCP 能力声明 ────────────────────────────────────────────────────────────

/// 服务端能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// 工具能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    /// 资源能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    /// Prompt 能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    /// 日志能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
    /// 实验性能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

/// 工具能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    /// 工具列表是否可能变化
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 资源能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    /// 是否支持订阅
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// 资源列表是否可能变化
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Prompt 能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    /// Prompt 列表是否可能变化
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 日志能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// 客户端能力声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// 根目录能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// 采样能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
    /// 引出能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,
    /// 实验性能力
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

/// 根目录能力
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    /// 是否支持根目录列表变更通知
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// 采样能力
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// 引出能力
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElicitationCapability {}

// ─── MCP 初始化 ──────────────────────────────────────────────────────────────

/// 初始化请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// 协议版本
    pub protocol_version: String,
    /// 客户端能力
    pub capabilities: ClientCapabilities,
    /// 客户端信息
    pub client_info: Implementation,
}

/// 初始化响应结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// 协议版本
    pub protocol_version: String,
    /// 服务端能力
    pub capabilities: ServerCapabilities,
    /// 服务端信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_info: Option<Implementation>,
    /// 服务端指令
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// 实现信息（客户端或服务端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

// ─── MCP 工具相关类型 ────────────────────────────────────────────────────────

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    /// 工具名称
    pub name: String,
    /// 工具描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入参数 JSON Schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

/// 工具调用请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    /// 工具名称
    pub name: String,
    /// 工具参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// 内容块列表
    pub content: Vec<ContentBlock>,
    /// 是否为错误结果
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// 内容块类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    /// 文本内容
    #[serde(rename = "text")]
    Text { text: String },
    /// 图片内容
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// 资源链接
    #[serde(rename = "resource")]
    Resource { resource: ResourceReference },
}

/// 资源引用
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// 工具列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

// ─── MCP 资源相关类型 ────────────────────────────────────────────────────────

/// 资源定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// 资源 URI
    pub uri: String,
    /// 资源名称
    pub name: String,
    /// 资源描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// 资源内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// 资源列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 读取资源请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    pub uri: String,
}

/// 读取资源结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContent>,
}

// ─── MCP Prompt 相关类型 ─────────────────────────────────────────────────────

/// Prompt 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDefinition {
    /// Prompt 名称
    pub name: String,
    /// Prompt 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 参数定义
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt 参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Prompt 列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    pub prompts: Vec<PromptDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// 获取 Prompt 请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<std::collections::HashMap<String, String>>,
}

/// Prompt 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: ContentBlock,
}

/// Prompt 角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptRole {
    User,
    Assistant,
}

/// 获取 Prompt 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

// ─── MCP 引出 (Elicitation) 类型 ─────────────────────────────────────────────

/// 引出请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitRequestParams {
    /// 向用户展示的消息
    pub message: String,
    /// 请求的 Schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_schema: Option<Value>,
}

/// 引出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    /// 用户响应动作
    pub action: ElicitAction,
    /// 用户输入的内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

/// 引出动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitAction {
    Accept,
    Deny,
    Dismiss,
}

// ─── MCP 根目录相关 ──────────────────────────────────────────────────────────

/// 根目录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// 根目录列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsResult {
    pub roots: Vec<Root>,
}

// ─── MCP 进度与日志 ──────────────────────────────────────────────────────────

/// 进度通知参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotification {
    /// 进度 token
    pub progress_token: Value,
    /// 当前进度
    pub progress: f64,
    /// 总进度
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// 日志消息通知
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub level: LogLevel,
    pub logger: Option<String>,
    pub data: Value,
}

// ─── 方法名常量 ──────────────────────────────────────────────────────────────

/// MCP 标准方法名
pub mod methods {
    pub const INITIALIZE: &str = "initialize";
    pub const INITIALIZED: &str = "notifications/initialized";
    pub const PING: &str = "ping";
    pub const LIST_TOOLS: &str = "tools/list";
    pub const CALL_TOOL: &str = "tools/call";
    pub const LIST_RESOURCES: &str = "resources/list";
    pub const READ_RESOURCE: &str = "resources/read";
    pub const LIST_PROMPTS: &str = "prompts/list";
    pub const GET_PROMPT: &str = "prompts/get";
    pub const LIST_ROOTS: &str = "roots/list";
    pub const ROOTS_LIST_CHANGED: &str = "notifications/roots/list_changed";
    pub const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
    pub const RESOURCES_LIST_CHANGED: &str = "notifications/resources/list_changed";
    pub const PROGRESS: &str = "notifications/progress";
    pub const LOG_MESSAGE: &str = "notifications/message";
    pub const ELICIT: &str = "elicitation/create";
}
