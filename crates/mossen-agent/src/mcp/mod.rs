//! MCP 服务层实现
//!
//! 翻译自 `services/mcp/` 目录下的 TypeScript 源码，提供 MCP 服务器的
//! 连接管理、认证、配置加载、通道通知、权限中继等完整功能。

pub mod auth;
pub mod builtin_template_plan;
pub mod builtin_templates;
pub mod channel_allowlist;
pub mod channel_notification;
pub mod channel_permissions;
pub mod client;
pub mod config;
pub mod connection_manager;
pub mod elicitation_handler;
pub mod env_expansion;
pub mod headers_helper;
pub mod hosted;
pub mod in_process_transport;
pub mod mcp_string_utils;
pub mod normalization;
pub mod oauth_port;
pub mod official_registry;
pub mod remote_install_plan;
pub mod sdk_control_transport;
pub mod slash_add_plan;
pub mod types;
pub mod use_manage_connections;
pub mod utils;
pub mod vscode_sdk_mcp;
pub mod xaa;
pub mod xaa_idp_login;
