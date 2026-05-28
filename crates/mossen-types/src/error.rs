//! # error — 错误类型层次
//!
//! 使用 `thiserror` 定义 Mossen 错误类型层次。

use thiserror::Error;

/// Mossen 核心错误类型。
#[derive(Debug, Error)]
pub enum MossenError {
    /// API 错误。
    #[error("API error: {message} (status: {status_code})")]
    Api {
        status_code: u16,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 认证错误。
    #[error("Authentication error: {0}")]
    Auth(String),

    /// 速率限制。
    #[error("Rate limited: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    /// 过载。
    #[error("API overloaded: {0}")]
    Overloaded(String),

    /// 上下文窗口超出。
    #[error("Context window exceeded: {0}")]
    ContextWindowExceeded(String),

    /// 无效请求。
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// 网络错误。
    #[error("Network error: {0}")]
    Network(String),

    /// 超时。
    #[error("Timeout: {0}")]
    Timeout(String),

    /// 工具执行错误。
    #[error("Tool execution error: {tool_name} — {message}")]
    ToolExecution { tool_name: String, message: String },

    /// 权限拒绝。
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// 配置错误。
    #[error("Configuration error: {0}")]
    Config(String),

    /// 插件错误。
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// 文件操作错误。
    #[error("File error: {0}")]
    FileOperation(String),

    /// 序列化/反序列化错误。
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO 错误。
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 内部错误。
    #[error("Internal error: {0}")]
    Internal(String),

    /// 用户中断。
    #[error("User cancelled")]
    UserCancelled,

    /// 未知错误。
    #[error("{0}")]
    Other(String),
}

/// 错误 ID 常量（用于生产追踪）。
pub mod error_ids {
    pub const E_TOOL_USE_SUMMARY_GENERATION_FAILED: u32 = 344;
}
