//! # api — API 配置类型
//!
//! 定义 API 提供商、配置、OAuth 等类型。
//! 对应 TypeScript 中 API 相关的配置和常量。

use serde::{Deserialize, Serialize};

/// API 提供商。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiProvider {
    /// Mossen 官方 API。
    Mossen,
    /// Bedrock。
    Bedrock,
    /// Vertex AI。
    Vertex,
    /// 自定义后端。
    Custom,
}

/// API 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiConfiguration {
    /// API 密钥。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 基础 URL。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// 模型 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 提供商。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ApiProvider>,
    /// 最大 token 数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// 温度。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

/// OAuth 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OauthConfig {
    pub base_api_url: String,
    pub console_authorize_url: String,
    pub hosted_authorize_url: String,
    pub hosted_origin: String,
    pub token_url: String,
    pub api_key_url: String,
    pub roles_url: String,
    pub console_success_url: String,
    pub hosted_success_url: String,
    pub manual_redirect_url: String,
    pub client_id: String,
    pub oauth_file_suffix: String,
    pub mcp_proxy_url: String,
    pub mcp_proxy_path: String,
}

/// OAuth 配置类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OauthConfigType {
    Prod,
    Staging,
    Local,
}

/// OAuth 范围常量。
pub const HOSTED_INFERENCE_SCOPE: &str = "user:inference";
pub const HOSTED_PROFILE_SCOPE: &str = "user:profile";
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

/// Console OAuth 范围。
pub const CONSOLE_OAUTH_SCOPES: &[&str] = &["org:create_api_key", HOSTED_PROFILE_SCOPE];

/// Hosted OAuth 范围。
pub const HOSTED_OAUTH_SCOPES: &[&str] = &[
    HOSTED_PROFILE_SCOPE,
    HOSTED_INFERENCE_SCOPE,
    "user:sessions:mossen_code",
    "user:mcp_servers",
    "user:file_upload",
];

/// MCP 客户端元数据 URL。
pub const MCP_CLIENT_METADATA_URL: &str =
    "https://platform.mossen.invalid/oauth/mossen-code-client-metadata";

/// GitHub 工作流就绪状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubWorkflowReadiness {
    pub bootstrap_url: String,
    pub issues: Vec<String>,
    pub ready: bool,
}

/// 输出样式配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_coding_instructions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_for_plugin: Option<bool>,
}

/// 默认输出样式名称。
pub const DEFAULT_OUTPUT_STYLE_NAME: &str = "default";

/// 系统提示分段类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptSection {
    pub name: String,
    pub cache_break: bool,
}
