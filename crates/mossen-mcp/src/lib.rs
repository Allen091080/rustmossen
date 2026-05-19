//! # mossen-mcp
//!
//! Mossen MCP (Model Context Protocol) 完整实现。
//!
//! 提供 MCP 协议的客户端实现，支持多种传输方式（stdio, SSE, HTTP, WebSocket），
//! 以及服务器发现、OAuth 认证、工具转发、资源访问等完整功能。
//!
//! ## 模块结构
//!
//! - [`protocol`] — MCP 协议消息定义（JSON-RPC 2.0）
//! - [`config`] — 服务器配置类型与加载
//! - [`transport`] — 传输层抽象（stdio, SSE, HTTP, WebSocket）
//! - [`client`] — MCP 客户端连接与状态管理
//! - [`server`] — 服务器生命周期管理
//! - [`tools`] — 工具调用转发
//! - [`resources`] — 资源访问
//! - [`prompts`] — Prompt 模板管理
//! - [`discovery`] — 服务器自动发现
//! - [`auth`] — OAuth 认证
//! - [`normalization`] — 名称规范化工具

pub mod auth;
pub mod auth_ext;
pub mod channels;
pub mod client;
pub mod client_ext;
pub mod config;
pub mod config_ext;
pub mod discovery;
pub mod elicitation;
pub mod extras;
pub mod hosted;
pub mod normalization;
pub mod plans;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;
pub mod transport;
pub mod types_schemas;
pub mod utils;
pub mod xaa;
pub mod xaa_idp_login;

// Re-export 核心类型
pub use client::{McpCliState, McpClient, McpServerConnection, SerializedClient, SerializedTool};
pub use config::{ConfigScope, McpServerConfig, ScopedMcpServerConfig, TransportType};
pub use discovery::{McpDiscovery, OfficialRegistry};
pub use normalization::normalize_name_for_mcp;
pub use prompts::{BuiltinTemplate, McpPrompt, McpPromptManager};
pub use protocol::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, ServerCapabilities,
    ToolDefinition,
};
pub use resources::McpResourceManager;
pub use server::McpServerManager;
pub use tools::{McpTool, McpToolCallResult, McpToolRegistry};
pub use transport::McpTransport;
