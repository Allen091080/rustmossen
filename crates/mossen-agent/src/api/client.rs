//! # Mossen Client Factory
//!
//! 翻译自 `services/api/client.ts` (481行)
//! 创建 Mossen API 客户端（支持多后端路由）。

use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;
use uuid::Uuid;

/// Client request ID header name.
pub const CLIENT_REQUEST_ID_HEADER: &str = "x-client-request-id";

/// Known gateway types detected from response headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnownGateway {
    Litellm,
    Helicone,
    Portkey,
    CloudflareAiGateway,
    Kong,
    Braintrust,
    Databricks,
}

impl KnownGateway {
    pub fn as_str(&self) -> &str {
        match self {
            KnownGateway::Litellm => "litellm",
            KnownGateway::Helicone => "helicone",
            KnownGateway::Portkey => "portkey",
            KnownGateway::CloudflareAiGateway => "cloudflare-ai-gateway",
            KnownGateway::Kong => "kong",
            KnownGateway::Braintrust => "braintrust",
            KnownGateway::Databricks => "databricks",
        }
    }
}

/// Gateway fingerprints for detecting AI gateways from response headers.
const GATEWAY_FINGERPRINTS: &[(&str, &[&str])] = &[
    ("litellm", &["x-litellm-"]),
    ("helicone", &["helicone-"]),
    ("portkey", &["x-portkey-"]),
    ("cloudflare-ai-gateway", &["cf-aig-"]),
    ("kong", &["x-kong-"]),
    ("braintrust", &["x-bt-"]),
];

/// Gateways that use provider-owned domains.
const GATEWAY_HOST_SUFFIXES: &[(&str, &[&str])] = &[(
    "databricks",
    &[
        ".cloud.databricks.com",
        ".azuredatabricks.net",
        ".gcp.databricks.com",
    ],
)];

/// Detect gateway from response headers or base URL.
pub fn detect_gateway(headers: Option<&HeaderMap>, base_url: Option<&str>) -> Option<KnownGateway> {
    if let Some(headers) = headers {
        let header_names: Vec<String> = headers.keys().map(|k| k.as_str().to_lowercase()).collect();
        for (gw, prefixes) in GATEWAY_FINGERPRINTS {
            if prefixes
                .iter()
                .any(|p| header_names.iter().any(|h| h.starts_with(p)))
            {
                return Some(match *gw {
                    "litellm" => KnownGateway::Litellm,
                    "helicone" => KnownGateway::Helicone,
                    "portkey" => KnownGateway::Portkey,
                    "cloudflare-ai-gateway" => KnownGateway::CloudflareAiGateway,
                    "kong" => KnownGateway::Kong,
                    "braintrust" => KnownGateway::Braintrust,
                    _ => continue,
                });
            }
        }
    }

    if let Some(base_url) = base_url {
        if let Ok(parsed) = url::Url::parse(base_url) {
            if let Some(host) = parsed.host_str() {
                let host_lower = host.to_lowercase();
                for (gw, suffixes) in GATEWAY_HOST_SUFFIXES {
                    if suffixes.iter().any(|s| host_lower.ends_with(s)) {
                        return Some(match *gw {
                            "databricks" => KnownGateway::Databricks,
                            _ => continue,
                        });
                    }
                }
            }
        }
    }

    None
}

/// Configuration for creating the Mossen client.
pub struct MossenClientConfig {
    pub api_key: Option<String>,
    pub auth_token: Option<String>,
    pub base_url: String,
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub default_headers: HashMap<String, String>,
    pub session_id: String,
    pub user_agent: String,
    pub container_id: Option<String>,
    pub remote_session_id: Option<String>,
    pub client_app: Option<String>,
    pub additional_protection: bool,
    pub is_custom_backend: bool,
    pub custom_backend_protocol: Option<String>,
}

/// Parse custom headers from MOSSEN_CODE_CUSTOM_HEADERS environment variable.
pub fn parse_custom_headers(env_value: Option<&str>) -> HashMap<String, String> {
    let mut custom_headers = HashMap::new();

    let Some(value) = env_value else {
        return custom_headers;
    };

    // Split by newlines to support multiple headers
    for header_string in value.split('\n') {
        let trimmed = header_string.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse header in format "Name: Value" (curl style)
        if let Some(colon_idx) = trimmed.find(':') {
            let name = trimmed[..colon_idx].trim();
            let value = trimmed[colon_idx + 1..].trim();
            if !name.is_empty() {
                custom_headers.insert(name.to_string(), value.to_string());
            }
        }
    }

    custom_headers
}

/// Build default headers for the Mossen client.
pub fn build_default_headers(config: &MossenClientConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert("x-app", HeaderValue::from_static("cli"));

    if let Ok(ua) = HeaderValue::from_str(&config.user_agent) {
        headers.insert("User-Agent", ua);
    }

    if let Ok(sid) = HeaderValue::from_str(&config.session_id) {
        headers.insert("X-Mossen-Code-Session-Id", sid);
    }

    for (key, value) in &config.default_headers {
        if let (Ok(k), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(k, v);
        }
    }

    if let Some(ref container_id) = config.container_id {
        if let Ok(v) = HeaderValue::from_str(container_id) {
            headers.insert("x-mossen-remote-container-id", v);
        }
    }

    if let Some(ref remote_session_id) = config.remote_session_id {
        if let Ok(v) = HeaderValue::from_str(remote_session_id) {
            headers.insert("x-mossen-remote-session-id", v);
        }
    }

    if let Some(ref client_app) = config.client_app {
        if let Ok(v) = HeaderValue::from_str(client_app) {
            headers.insert("x-client-app", v);
        }
    }

    if config.additional_protection {
        headers.insert(
            "x-mossen-additional-protection",
            HeaderValue::from_static("true"),
        );
    }

    headers
}

/// Create a reqwest client configured for the Mossen API.
pub fn create_mossen_http_client(config: &MossenClientConfig) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .default_headers(build_default_headers(config));

    // Add auth header
    if let Some(ref auth_token) = config.auth_token {
        let mut headers = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(&format!("Bearer {}", auth_token)) {
            headers.insert("Authorization", v);
        }
        builder = builder.default_headers(headers);
    } else if let Some(ref api_key) = config.api_key {
        let mut headers = HeaderMap::new();
        if let Ok(v) = HeaderValue::from_str(api_key) {
            headers.insert("x-api-key", v);
        }
        builder = builder.default_headers(headers);
    }

    builder.build()
}

/// Get the API base URL based on configuration.
pub fn get_mossen_api_base_url(
    env_base_url: Option<&str>,
    is_hosted_auth_adapter: bool,
    oauth_base_url: &str,
) -> Result<String, anyhow::Error> {
    if let Some(base_url) = env_base_url {
        return Ok(base_url.to_string());
    }
    if is_hosted_auth_adapter {
        return Ok(oauth_base_url.to_string());
    }
    Err(anyhow::anyhow!(
        "No Mossen backend is configured. For personal edition, set MOSSEN_CODE_CUSTOM_BASE_URL \
         and MOSSEN_CODE_CUSTOM_API_KEY (or MOSSEN_CODE_CUSTOM_AUTH_TOKEN), or set \
         MOSSEN_CODE_API_BASE_URL for an explicit hosted adapter."
    ))
}

/// A wrapper for fetch that injects the x-client-request-id header.
pub fn inject_client_request_id(headers: &mut HeaderMap, source: Option<&str>) {
    if !headers.contains_key(CLIENT_REQUEST_ID_HEADER) {
        let uuid = Uuid::new_v4().to_string();
        if let Ok(v) = HeaderValue::from_str(&uuid) {
            headers.insert(CLIENT_REQUEST_ID_HEADER, v);
        }
    }

    // Log for debugging
    if let Some(id) = headers.get(CLIENT_REQUEST_ID_HEADER) {
        debug!(
            "[API REQUEST] {}={} source={}",
            CLIENT_REQUEST_ID_HEADER,
            id.to_str().unwrap_or("?"),
            source.unwrap_or("unknown")
        );
    }
}

/// Provider types for the Mossen client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiProvider {
    FirstParty,
    Bedrock,
    Vertex,
    Foundry,
    CustomBackend,
}

/// Determine which API provider to use based on environment configuration.
pub fn determine_api_provider(
    use_bedrock: bool,
    use_vertex: bool,
    use_foundry: bool,
    is_custom_backend: bool,
) -> ApiProvider {
    if is_custom_backend {
        ApiProvider::CustomBackend
    } else if use_bedrock {
        ApiProvider::Bedrock
    } else if use_foundry {
        ApiProvider::Foundry
    } else if use_vertex {
        ApiProvider::Vertex
    } else {
        ApiProvider::FirstParty
    }
}

/// Bedrock configuration.
pub struct BedrockConfig {
    pub aws_region: String,
    pub base_url: Option<String>,
    pub skip_auth: bool,
    pub bearer_token: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
}

/// Vertex configuration.
pub struct VertexConfig {
    pub region: String,
    pub project_id: Option<String>,
    pub base_url: Option<String>,
    pub skip_auth: bool,
}

/// Foundry (Azure) configuration.
pub struct FoundryConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub resource: Option<String>,
    pub skip_auth: bool,
}

/// Create a stderr logger for the provider SDK (debug mode).
pub struct StderrLogger;

impl StderrLogger {
    pub fn error(msg: &str) {
        eprintln!("[Provider SDK ERROR] {}", msg);
    }

    pub fn warn(msg: &str) {
        eprintln!("[Provider SDK WARN] {}", msg);
    }

    pub fn info(msg: &str) {
        eprintln!("[Provider SDK INFO] {}", msg);
    }

    pub fn debug(msg: &str) {
        eprintln!("[Provider SDK DEBUG] {}", msg);
    }
}

/// TS `getMossenClient` — entry-point that resolves the active Mossen client
/// config from the environment + provided overrides. The Rust port simply
/// returns the supplied config (or a minimal default) — call sites build the
/// HTTP client themselves from the config.
pub fn get_mossen_client(overrides: Option<MossenClientConfig>) -> MossenClientConfig {
    overrides.unwrap_or_else(|| MossenClientConfig {
        api_key: None,
        auth_token: None,
        base_url: String::new(),
        max_retries: 0,
        timeout_ms: 0,
        default_headers: HashMap::new(),
        session_id: String::new(),
        user_agent: String::new(),
        container_id: None,
        remote_session_id: None,
        client_app: None,
        additional_protection: false,
        is_custom_backend: false,
        custom_backend_protocol: None,
    })
}
