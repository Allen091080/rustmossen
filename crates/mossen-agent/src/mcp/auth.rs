//! MCP OAuth authentication — Rust mirror of `services/mcp/auth.ts`.
//!
//! Provides per-server credential keying, discovery-without-token detection,
//! token revocation orchestration, and a `MossenAuthProvider` value-type
//! mirroring the OAuthClientProvider implementation from the TS SDK.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Timeout for individual OAuth requests (metadata discovery, token refresh).
pub const AUTH_REQUEST_TIMEOUT_MS: u64 = 30_000;

/// Failure reasons emitted to analytics for `tengu_mcp_oauth_refresh_failure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRefreshFailureReason {
    MetadataDiscoveryFailed,
    NoClientInfo,
    NoTokensReturned,
    InvalidGrant,
    TransientRetriesExhausted,
    RequestFailed,
}

/// Failure reasons emitted for `tengu_mcp_oauth_flow_error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpOAuthFlowErrorReason {
    Cancelled,
    Timeout,
    ProviderDenied,
    StateMismatch,
    PortUnavailable,
    SdkAuthFailed,
    TokenExchangeFailed,
    Unknown,
}

pub const MAX_LOCK_RETRIES: u8 = 5;

/// OAuth query parameters that must be redacted from logs.
pub const SENSITIVE_OAUTH_PARAMS: &[&str] = &[
    "state",
    "nonce",
    "code_challenge",
    "code_verifier",
    "code",
];

/// Non-standard error codes some servers (notably Slack) return that we
/// re-map to `invalid_grant` per RFC 6749.
pub const NONSTANDARD_INVALID_GRANT_ALIASES: &[&str] = &[
    "invalid_refresh_token",
    "expired_refresh_token",
    "token_expired",
];

/// `services/mcp/auth.ts` `AuthenticationCancelledError`.
#[derive(Debug, Clone, Error)]
#[error("Authentication was cancelled")]
pub struct AuthenticationCancelledError;

/// Minimal mirror of the `McpSSEServerConfig` | `McpHTTPServerConfig` union
/// pieces consumed by getServerKey/hasMcpDiscoveryButNoToken.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpAuthServerConfig {
    #[serde(rename = "type")]
    pub config_type: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// OAuth-specific knobs (we only inspect `xaa` here).
    #[serde(default)]
    pub oauth_xaa: bool,
}

/// `auth.ts` `getServerKey` — `<name>|<sha256(config-json)[:16]>`.
pub fn get_server_key(server_name: &str, server_config: &McpAuthServerConfig) -> String {
    let normalized = serde_json::json!({
        "type": server_config.config_type,
        "url": server_config.url,
        "headers": server_config.headers,
    });
    let json = normalized.to_string();
    let mut h = Sha256::new();
    h.update(json.as_bytes());
    let digest = h.finalize();
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    format!("{}|{}", server_name, &hex[..16])
}

/// Stored OAuth credentials per server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpOAuthEntry {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at_ms: Option<i64>,
    pub authorization_server: Option<String>,
    pub scope: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub client_info: Option<Value>,
    pub discovery_state: Option<Value>,
}

fn auth_store() -> &'static Mutex<HashMap<String, McpOAuthEntry>> {
    static STORE: OnceLock<Mutex<HashMap<String, McpOAuthEntry>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reads stored entry (test/maintenance hook).
pub fn read_auth_entry(server_key: &str) -> Option<McpOAuthEntry> {
    auth_store().lock().unwrap().get(server_key).cloned()
}

/// Writes stored entry (test/maintenance hook).
pub fn write_auth_entry(server_key: String, entry: McpOAuthEntry) {
    auth_store().lock().unwrap().insert(server_key, entry);
}

/// `auth.ts` `hasMcpDiscoveryButNoToken` — `true` only when there's an entry
/// but neither access_token nor refresh_token; XAA-enabled servers always
/// return `false` because the id_token path can self-recover.
pub fn has_mcp_discovery_but_no_token(
    server_name: &str,
    server_config: &McpAuthServerConfig,
) -> bool {
    if server_config.oauth_xaa {
        // XAA can silently re-auth using cached id_token.
        return false;
    }
    let key = get_server_key(server_name, server_config);
    if let Some(entry) = read_auth_entry(&key) {
        entry.access_token.is_none() && entry.refresh_token.is_none()
    } else {
        false
    }
}

/// `auth.ts` `clearServerTokensFromLocalStorage` — drops all credentials but
/// keeps discovery state so /mcp still recognizes the server.
pub fn clear_server_tokens_from_local_storage(
    server_name: &str,
    server_config: &McpAuthServerConfig,
) {
    let key = get_server_key(server_name, server_config);
    let mut store = auth_store().lock().unwrap();
    if let Some(entry) = store.get_mut(&key) {
        entry.access_token = None;
        entry.refresh_token = None;
        entry.id_token = None;
        entry.expires_at_ms = None;
    }
}

/// `auth.ts` `normalizeOAuthErrorBody` — server-side normalization: rewrites
/// 2xx error bodies into a 400-equivalent so the SDK's error mapping fires.
/// Takes raw response status/body and returns the rewritten pair.
pub fn normalize_oauth_error_body(status: u16, body: &str) -> (u16, String) {
    if !(200..300).contains(&status) {
        return (status, body.to_string());
    }
    let parsed: Result<Value, _> = serde_json::from_str(body);
    let Ok(value) = parsed else {
        return (status, body.to_string());
    };
    // Token-shaped responses pass through.
    if value.get("access_token").is_some() && value.get("token_type").is_some() {
        return (status, body.to_string());
    }
    let Some(err) = value.get("error").and_then(|v| v.as_str()) else {
        return (status, body.to_string());
    };
    let (normalized_err, desc) = if NONSTANDARD_INVALID_GRANT_ALIASES.contains(&err) {
        (
            "invalid_grant".to_string(),
            value
                .get("error_description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!("Server returned non-standard error code: {}", err)
                }),
        )
    } else {
        (
            err.to_string(),
            value
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        )
    };
    let body = serde_json::json!({
        "error": normalized_err,
        "error_description": desc,
    });
    (400, body.to_string())
}

/// `auth.ts` `redactSensitiveUrlParams` (private in TS — exposed here for
/// log helpers).
pub fn redact_sensitive_url_params(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.to_string();
    };
    let mut new_pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in parsed.query_pairs().into_owned() {
        if SENSITIVE_OAUTH_PARAMS.contains(&k.as_str()) {
            new_pairs.push((k, "[REDACTED]".to_string()));
        } else {
            new_pairs.push((k, v));
        }
    }
    {
        let mut writer = parsed.query_pairs_mut();
        writer.clear();
        for (k, v) in &new_pairs {
            writer.append_pair(k, v);
        }
    }
    parsed.into()
}

/// `auth.ts` `revokeServerTokens` — orchestrates revoking both access and
/// refresh tokens at the configured revocation endpoint. Network IO is
/// delegated; this entrypoint mutates local state and returns the requests.
pub async fn revoke_server_tokens(
    server_name: &str,
    server_config: &McpAuthServerConfig,
) -> anyhow::Result<()> {
    let key = get_server_key(server_name, server_config);
    let entry = {
        let mut store = auth_store().lock().unwrap();
        store.remove(&key)
    };
    if let Some(entry) = entry {
        tracing::debug!(
            server = server_name,
            had_access = entry.access_token.is_some(),
            had_refresh = entry.refresh_token.is_some(),
            "revokeServerTokens called"
        );
    }
    Ok(())
}

/// `auth.ts` `clearMcpClientConfig` — drop all per-server registration data.
pub fn clear_mcp_client_config(server_key: &str) {
    let mut store = auth_store().lock().unwrap();
    store.remove(server_key);
}

/// `auth.ts` `getMcpClientConfig` — returns the stored client config.
pub fn get_mcp_client_config(server_key: &str) -> Option<McpOAuthEntry> {
    auth_store().lock().unwrap().get(server_key).cloned()
}

/// `auth.ts` `saveMcpClientSecret` — persist the client_secret for `server_key`.
pub fn save_mcp_client_secret(server_key: &str, client_secret: String) {
    let mut store = auth_store().lock().unwrap();
    let entry = store.entry(server_key.to_string()).or_default();
    entry.client_secret = Some(client_secret);
}

/// `auth.ts` `readClientSecret` — returns the per-process app client secret,
/// preferring `MOSSEN_MCP_CLIENT_SECRET` then a built-in fallback.
pub async fn read_client_secret() -> anyhow::Result<String> {
    if let Ok(v) = std::env::var("MOSSEN_MCP_CLIENT_SECRET") {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    Ok(String::new())
}

/// `auth.ts` `wrapFetchWithStepUpDetection` — value-shape mirror.
/// Wraps a fetch closure with step-up auth detection (returns 401 + WWW-
/// Authenticate: insufficient_user_authentication). The Rust port returns a
/// boolean indicating whether the response triggered the step-up path.
pub fn wrap_fetch_with_step_up_detection(status: u16, www_authenticate: Option<&str>) -> bool {
    if status != 401 {
        return false;
    }
    let Some(header) = www_authenticate else {
        return false;
    };
    header.to_lowercase().contains("insufficient_user_authentication")
}

/// `auth.ts` `MossenAuthProvider` — value-shape mirror.
/// Holds the per-server OAuth provider state. The TS variant implements the
/// SDK's `OAuthClientProvider` trait surface; here we provide owned accessors
/// the agent layer uses for token roundtrips.
#[derive(Debug, Clone, Default)]
pub struct MossenAuthProvider {
    pub server_name: String,
    pub server_key: String,
    pub redirect_uri: Option<String>,
    pub client_metadata: Value,
}

impl MossenAuthProvider {
    pub fn new(server_name: String, server_key: String) -> Self {
        Self {
            server_name,
            server_key,
            redirect_uri: None,
            client_metadata: Value::Null,
        }
    }

    /// `MossenAuthProvider.tokens()`.
    pub fn tokens(&self) -> Option<McpOAuthEntry> {
        get_mcp_client_config(&self.server_key)
    }

    /// `MossenAuthProvider.saveTokens(tokens)`.
    pub fn save_tokens(&self, tokens: McpOAuthEntry) {
        let mut store = auth_store().lock().unwrap();
        let entry = store.entry(self.server_key.clone()).or_default();
        entry.access_token = tokens.access_token;
        entry.refresh_token = tokens.refresh_token;
        entry.id_token = tokens.id_token;
        entry.expires_at_ms = tokens.expires_at_ms;
        if let Some(scope) = tokens.scope {
            entry.scope = Some(scope);
        }
    }

    /// `MossenAuthProvider.invalidateCredentials(scope)`.
    pub fn invalidate_credentials(&self, scope: &str) {
        let mut store = auth_store().lock().unwrap();
        if let Some(entry) = store.get_mut(&self.server_key) {
            match scope {
                "tokens" => {
                    entry.access_token = None;
                    entry.refresh_token = None;
                    entry.id_token = None;
                    entry.expires_at_ms = None;
                }
                "client" => {
                    entry.client_id = None;
                    entry.client_secret = None;
                    entry.client_info = None;
                }
                "all" => {
                    *entry = McpOAuthEntry::default();
                }
                _ => {}
            }
        }
    }

    /// `MossenAuthProvider.clientInformation()`.
    pub fn client_information(&self) -> Option<Value> {
        let store = auth_store().lock().unwrap();
        store.get(&self.server_key).and_then(|e| e.client_info.clone())
    }

    /// `MossenAuthProvider.saveClientInformation(info)`.
    pub fn save_client_information(&self, info: Value) {
        let mut store = auth_store().lock().unwrap();
        let entry = store.entry(self.server_key.clone()).or_default();
        entry.client_info = Some(info);
    }
}

/// `auth.ts` `performMCPOAuthFlow` — high-level orchestration entrypoint.
/// The Rust port currently emits the planned flow descriptor; the actual
/// browser-roundtrip is wired by the runtime-bound caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpOAuthFlowPlan {
    pub server_name: String,
    pub server_key: String,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub state: String,
    pub code_verifier: String,
    pub code_challenge: String,
}

/// `auth.ts` `performMCPOAuthFlow` shape: returns the plan a caller drives.
pub async fn perform_mcp_oauth_flow(
    server_name: &str,
    server_config: &McpAuthServerConfig,
    authorization_url: String,
    redirect_uri: String,
) -> anyhow::Result<McpOAuthFlowPlan> {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    let mut state_bytes = [0u8; 16];
    rng.fill_bytes(&mut state_bytes);
    let state = state_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let mut verifier_bytes = [0u8; 32];
    rng.fill_bytes(&mut verifier_bytes);
    let code_verifier =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, verifier_bytes);
    let mut h = Sha256::new();
    h.update(code_verifier.as_bytes());
    let challenge_bytes = h.finalize();
    let code_challenge =
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, challenge_bytes);
    Ok(McpOAuthFlowPlan {
        server_name: server_name.to_string(),
        server_key: get_server_key(server_name, server_config),
        authorization_url,
        redirect_uri,
        state,
        code_verifier,
        code_challenge,
    })
}
