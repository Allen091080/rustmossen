//! MCP OAuth 认证
//!
//! 实现 MCP 服务器的 OAuth 2.0 认证流程，包括：
//! - 授权服务器元数据发现
//! - PKCE 授权码流程
//! - Token 刷新
//! - 凭证存储

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ─── 常量 ────────────────────────────────────────────────────────────────────

/// OAuth 请求超时
const AUTH_REQUEST_TIMEOUT_MS: u64 = 30_000;

/// 最大锁重试次数
#[allow(dead_code)]
const MAX_LOCK_RETRIES: u32 = 5;

/// OAuth 回调端口范围
const REDIRECT_PORT_RANGE_MIN: u16 = 49152;
const REDIRECT_PORT_RANGE_MAX: u16 = 65535;
const REDIRECT_PORT_FALLBACK: u16 = 3118;

// ─── OAuth Token 类型 ────────────────────────────────────────────────────────

/// OAuth Token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// 访问令牌
    pub access_token: String,
    /// 令牌类型
    pub token_type: String,
    /// 刷新令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// 过期时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// 权限范围
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// OAuth 客户端信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientInfo {
    /// 客户端 ID
    pub client_id: String,
    /// 客户端密钥（动态注册时获得）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// 注册时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<u64>,
}

/// 授权服务器元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    /// 发行者
    pub issuer: String,
    /// 授权端点
    pub authorization_endpoint: String,
    /// Token 端点
    pub token_endpoint: String,
    /// 注册端点
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    /// 支持的响应类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_types_supported: Option<Vec<String>>,
    /// 支持的 code challenge 方法
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,
}

// ─── MCP OAuth 管理器 ────────────────────────────────────────────────────────

/// MCP OAuth 认证管理器
pub struct McpOAuthManager {
    /// HTTP 客户端
    http_client: reqwest::Client,
    /// Token 缓存：server_key → tokens
    token_cache: RwLock<HashMap<String, OAuthTokens>>,
    /// 客户端信息缓存
    client_info_cache: RwLock<HashMap<String, OAuthClientInfo>>,
    /// 元数据缓存
    metadata_cache: RwLock<HashMap<String, AuthorizationServerMetadata>>,
}

impl McpOAuthManager {
    /// 创建新的 OAuth 管理器
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            token_cache: RwLock::new(HashMap::new()),
            client_info_cache: RwLock::new(HashMap::new()),
            metadata_cache: RwLock::new(HashMap::new()),
        }
    }

    /// 发现授权服务器元数据
    pub async fn discover_metadata(
        &self,
        server_url: &str,
    ) -> anyhow::Result<AuthorizationServerMetadata> {
        // 检查缓存
        let cache = self.metadata_cache.read().await;
        if let Some(metadata) = cache.get(server_url) {
            return Ok(metadata.clone());
        }
        drop(cache);

        // 尝试 RFC 8414 标准路径
        let well_known_url = format!(
            "{}/.well-known/oauth-authorization-server",
            server_url.trim_end_matches('/')
        );

        let response = self
            .http_client
            .get(&well_known_url)
            .timeout(Duration::from_millis(AUTH_REQUEST_TIMEOUT_MS))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to discover OAuth metadata: HTTP {}",
                response.status()
            ));
        }

        let metadata: AuthorizationServerMetadata = response.json().await?;

        // 缓存
        self.metadata_cache
            .write()
            .await
            .insert(server_url.to_string(), metadata.clone());

        Ok(metadata)
    }

    /// 获取缓存的 Token
    pub async fn get_cached_token(&self, server_key: &str) -> Option<OAuthTokens> {
        self.token_cache.read().await.get(server_key).cloned()
    }

    /// 存储 Token
    pub async fn store_token(&self, server_key: &str, tokens: OAuthTokens) {
        self.token_cache
            .write()
            .await
            .insert(server_key.to_string(), tokens);
    }

    /// 刷新 Token
    pub async fn refresh_token(
        &self,
        server_key: &str,
        metadata: &AuthorizationServerMetadata,
        client_info: &OAuthClientInfo,
    ) -> anyhow::Result<OAuthTokens> {
        let cached = self.get_cached_token(server_key).await;
        let refresh_token = cached
            .and_then(|t| t.refresh_token)
            .ok_or_else(|| anyhow::anyhow!("No refresh token available for {}", server_key))?;

        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", &refresh_token);
        params.insert("client_id", &client_info.client_id);

        let response = self
            .http_client
            .post(&metadata.token_endpoint)
            .form(&params)
            .timeout(Duration::from_millis(AUTH_REQUEST_TIMEOUT_MS))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Token refresh failed: HTTP {} - {}",
                status,
                body
            ));
        }

        let tokens: OAuthTokens = response.json().await?;
        self.store_token(server_key, tokens.clone()).await;
        Ok(tokens)
    }

    /// 使用授权码交换 Token
    pub async fn exchange_code(
        &self,
        server_key: &str,
        metadata: &AuthorizationServerMetadata,
        client_info: &OAuthClientInfo,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> anyhow::Result<OAuthTokens> {
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", redirect_uri);
        params.insert("client_id", &client_info.client_id);
        params.insert("code_verifier", code_verifier);

        let response = self
            .http_client
            .post(&metadata.token_endpoint)
            .form(&params)
            .timeout(Duration::from_millis(AUTH_REQUEST_TIMEOUT_MS))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Token exchange failed: HTTP {} - {}",
                status,
                body
            ));
        }

        let tokens: OAuthTokens = response.json().await?;
        self.store_token(server_key, tokens.clone()).await;
        Ok(tokens)
    }

    /// 清除指定服务器的缓存
    pub async fn clear_cache(&self, server_key: &str) {
        self.token_cache.write().await.remove(server_key);
        self.client_info_cache.write().await.remove(server_key);
        self.metadata_cache.write().await.remove(server_key);
    }

    /// 清除所有缓存
    pub async fn clear_all_cache(&self) {
        self.token_cache.write().await.clear();
        self.client_info_cache.write().await.clear();
        self.metadata_cache.write().await.clear();
    }
}

impl Default for McpOAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── PKCE 工具 ───────────────────────────────────────────────────────────────

/// 生成 PKCE code verifier
pub fn generate_code_verifier() -> String {
    let bytes: [u8; 32] = rand_bytes();
    let mut result = String::with_capacity(64);
    for byte in &bytes {
        result.push_str(&format!("{:02x}", byte));
    }
    result
}

/// 生成 PKCE code challenge (S256)
pub fn generate_code_challenge(verifier: &str) -> String {
    // SHA-256 哈希
    let digest = sha256(verifier.as_bytes());
    // Base64url 编码
    base64_url_encode(&digest)
}

/// 构建重定向 URI
pub fn build_redirect_uri(port: u16) -> String {
    format!("http://localhost:{}/callback", port)
}

/// 寻找可用端口
pub async fn find_available_port() -> anyhow::Result<u16> {
    // 检查环境变量配置的端口
    if let Ok(port_str) = std::env::var("MCP_OAUTH_CALLBACK_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            if port > 0 {
                return Ok(port);
            }
        }
    }

    // 随机选择端口
    let range = REDIRECT_PORT_RANGE_MAX - REDIRECT_PORT_RANGE_MIN + 1;
    let max_attempts = std::cmp::min(range as usize, 100);

    for _ in 0..max_attempts {
        let port = REDIRECT_PORT_RANGE_MIN + (rand_u16() % range);
        if is_port_available(port).await {
            return Ok(port);
        }
    }

    // 尝试后备端口
    if is_port_available(REDIRECT_PORT_FALLBACK).await {
        return Ok(REDIRECT_PORT_FALLBACK);
    }

    Err(anyhow::anyhow!("No available ports for OAuth redirect"))
}

/// 检查端口是否可用
async fn is_port_available(port: u16) -> bool {
    tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

// ─── 辅助函数 ────────────────────────────────────────────────────────────────

/// 生成随机字节
fn rand_bytes() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    // 使用简单的时间戳 + 递增计数器作为随机源
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = ((now >> (i % 16)) ^ (i as u128 * 251)) as u8;
    }
    bytes
}

/// 生成随机 u16
fn rand_u16() -> u16 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (now % 65536) as u16
}

/// SHA-256 哈希。使用 `sha2` crate 的标准实现。
fn sha256(data: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Base64url 编码
fn base64_url_encode(data: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as u32
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);

        if i + 1 < data.len() {
            result.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        }
        if i + 2 < data.len() {
            result.push(CHARSET[(triple & 0x3F) as usize] as char);
        }

        i += 3;
    }
    result
}

// ─── OAuth 刷新失败原因 ─────────────────────────────────────────────────────

/// OAuth 刷新失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshFailureReason {
    /// 元数据发现失败
    MetadataDiscoveryFailed,
    /// 无客户端信息
    NoClientInfo,
    /// 无 Token 返回
    NoTokensReturned,
    /// 无效的 grant
    InvalidGrant,
    /// 瞬态重试耗尽
    TransientRetriesExhausted,
    /// 请求失败
    RequestFailed,
}

/// OAuth 流程错误原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthFlowErrorReason {
    /// 用户取消
    Cancelled,
    /// 超时
    Timeout,
    /// 提供商拒绝
    ProviderDenied,
    /// 状态不匹配
    StateMismatch,
    /// 端口不可用
    PortUnavailable,
    /// SDK 认证失败
    SdkAuthFailed,
    /// Token 交换失败
    TokenExchangeFailed,
    /// 未知错误
    Unknown,
}
