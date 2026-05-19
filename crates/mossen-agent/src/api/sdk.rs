//! Mossen SDK types and error definitions.
//! Translated from `services/api/mossenSdk.ts`.

use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Core type aliases (TS used `any` for most SDK types)
// ---------------------------------------------------------------------------

pub type MossenClientOptions = HashMap<String, Value>;
pub type MossenBetaContentBlock = Value;
pub type MossenBetaContentBlockParam = Value;
pub type MossenBetaImageBlockParam = Value;
pub type MossenBetaJSONOutputFormat = Value;
pub type MossenBetaMessageDeltaUsage = Value;
pub type MossenBetaMessageParam = Value;
pub type MossenBetaMessageStreamParams = Value;
pub type MossenBetaOutputConfig = Value;
pub type MossenBetaRawMessageStreamEvent = Value;
pub type MossenBetaRequestDocumentBlock = Value;
pub type MossenBetaThinkingConfigParam = Value;
pub type MossenBetaTool = Value;
pub type MossenBetaToolChoice = Value;
pub type MossenBetaToolChoiceAuto = Value;
pub type MossenBetaToolChoiceTool = Value;
pub type MossenBetaToolUseBlock = Value;
pub type MossenBetaToolUnion = Value;
pub type MossenBase64ImageSource = Value;
pub type MossenContentBlock = Value;
pub type MossenContentBlockParam = Value;
pub type MossenImageBlockParam = Value;
pub type MossenMessageParam = Value;
pub type MossenTextBlockParam = Value;
pub type MossenThinkingBlock = Value;
pub type MossenThinkingBlockParam = Value;
pub type MossenTool = Value;
pub type MossenToolResultBlockParam = Value;
pub type MossenToolUseBlock = Value;
pub type MossenToolUseBlockParam = Value;

/// Stop reason returned by the API.
pub type MossenBetaStopReason = Option<String>;

/// Usage stats from API response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MossenBetaUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

/// A complete message from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MossenBetaMessage {
    pub id: String,
    pub content: Vec<Value>,
    pub model: String,
    pub role: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub message_type: String,
    pub usage: MossenBetaUsage,
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// API error with status code, body, and response headers.
#[derive(Error, Debug, Clone)]
pub struct MossenAPIError {
    pub status: u16,
    pub body: Value,
    pub message: String,
    pub headers: HeaderMap,
    pub request_id: Option<String>,
    /// Connection error code (e.g. ECONNRESET, ETIMEDOUT, SSL codes).
    pub error_code: Option<String>,
    /// Raw body as a JSON value for nested error extraction.
    pub raw_body: Option<Value>,
}

impl fmt::Display for MossenAPIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "API request failed with status {}: {}", self.status, self.message)
    }
}

impl MossenAPIError {
    pub fn new(status: u16, body: Value, message: Option<String>, headers: HeaderMap) -> Self {
        let msg = message.unwrap_or_else(|| format!("API request failed with status {}", status));
        let request_id = headers
            .get("request-id")
            .or_else(|| headers.get("x-request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        Self {
            status,
            raw_body: Some(body.clone()),
            body,
            message: msg,
            headers,
            request_id,
            error_code: None,
        }
    }

    pub fn generate(status: u16, body: Value, message: Option<String>, headers: HeaderMap) -> Self {
        Self::new(status, body, message, headers)
    }
}

/// Connection error (non-timeout).
#[derive(Error, Debug, Clone)]
#[error("{message}")]
pub struct MossenAPIConnectionError {
    pub message: String,
}

impl MossenAPIConnectionError {
    pub fn new(message: Option<String>) -> Self {
        Self {
            message: message.unwrap_or_else(|| "API connection error".to_string()),
        }
    }
}

/// Connection timeout error.
#[derive(Error, Debug, Clone)]
#[error("{message}")]
pub struct MossenAPIConnectionTimeoutError {
    pub message: String,
}

impl MossenAPIConnectionTimeoutError {
    pub fn new(message: Option<String>) -> Self {
        Self {
            message: message.unwrap_or_else(|| "API connection timed out".to_string()),
        }
    }
}

/// User abort error.
#[derive(Error, Debug, Clone)]
#[error("Request was aborted")]
pub struct MossenAPIUserAbortError;

/// Unified API error enum for pattern matching.
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    #[error("{0}")]
    Api(#[from] MossenAPIError),
    #[error("{0}")]
    Connection(#[from] MossenAPIConnectionError),
    #[error("{0}")]
    ConnectionTimeout(#[from] MossenAPIConnectionTimeoutError),
    #[error("{0}")]
    UserAbort(#[from] MossenAPIUserAbortError),
    #[error("{0}")]
    Other(String),
}

impl ApiError {
    pub fn is_connection_timeout(&self) -> bool {
        matches!(self, ApiError::ConnectionTimeout(_))
    }

    pub fn is_connection_error(&self) -> bool {
        matches!(self, ApiError::Connection(_) | ApiError::ConnectionTimeout(_))
    }

    pub fn is_user_abort(&self) -> bool {
        matches!(self, ApiError::UserAbort(_))
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Api(e) => Some(e.status),
            _ => None,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            ApiError::Api(e) => &e.message,
            ApiError::Connection(e) => &e.message,
            ApiError::ConnectionTimeout(e) => &e.message,
            ApiError::UserAbort(_) => "Request was aborted",
            ApiError::Other(s) => s,
        }
    }

    /// Check if the error message contains timeout text.
    pub fn is_timeout_message(&self) -> bool {
        self.message().to_lowercase().contains("timeout")
    }
}

/// SDK client trait — represents a Mossen provider SDK client.
#[async_trait::async_trait]
pub trait MossenSdkClient: Send + Sync {
    async fn create_message(
        &self,
        params: Value,
        options: Option<Value>,
    ) -> Result<MossenBetaMessage, ApiError>;

    async fn create_message_stream(
        &self,
        params: Value,
        options: Option<Value>,
    ) -> Result<MossenStreamHandle, ApiError>;
}

/// Handle for a streaming response.
pub struct MossenStreamHandle {
    pub request_id: String,
    pub response_headers: HeaderMap,
    pub events: tokio::sync::mpsc::Receiver<Result<MossenBetaRawMessageStreamEvent, ApiError>>,
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/api/mossenSdk.ts` SDK loaders.
// ---------------------------------------------------------------------------

/// `mossenSdk.ts` `MossenProviderSDK` — value-shape marker.
#[derive(Debug, Clone)]
pub struct MossenProviderSDK {
    pub provider: String,
    pub module_path: String,
}

impl MossenProviderSDK {
    pub fn new(provider: impl Into<String>, module_path: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            module_path: module_path.into(),
        }
    }
}

/// `mossenSdk.ts` `loadMossenBedrockProviderSDK`.
pub async fn load_mossen_bedrock_provider_sdk() -> MossenProviderSDK {
    MossenProviderSDK::new("bedrock", "@mossen-ai/sdk/bedrock")
}

/// `mossenSdk.ts` `loadMossenFoundryProviderSDK`.
pub async fn load_mossen_foundry_provider_sdk() -> MossenProviderSDK {
    MossenProviderSDK::new("foundry", "@mossen-ai/sdk/foundry")
}

/// `mossenSdk.ts` `loadMossenVertexProviderSDK`.
pub async fn load_mossen_vertex_provider_sdk() -> MossenProviderSDK {
    MossenProviderSDK::new("vertex", "@mossen-ai/sdk/vertex")
}
