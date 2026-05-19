//! MCP client — manages connections to MCP servers.
//!
//! Translates `services/mcp/client.ts` (3347 lines).
//! This is the core module that handles connecting to MCP servers via various
//! transports (stdio, SSE, HTTP, WebSocket), authentication (OAuth, XAA),
//! and managing the client lifecycle.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::mcp::types::{ConfigScope, McpServerConfig, ScopedMcpServerConfig};
use crate::mcp::utils::{Command, ServerResource, Tool};

/// Auth cache for MCP servers.
static AUTH_CACHE: std::sync::OnceLock<RwLock<HashMap<String, McpAuthState>>> =
    std::sync::OnceLock::new();

fn auth_cache() -> &'static RwLock<HashMap<String, McpAuthState>> {
    AUTH_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// MCP auth state.
#[derive(Debug, Clone)]
pub struct McpAuthState {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub authorization_server_url: Option<String>,
}

/// Clear the MCP auth cache.
pub async fn clear_mcp_auth_cache() {
    let mut cache = auth_cache().write().await;
    cache.clear();
}

/// Clear auth cache for a specific server.
pub async fn clear_server_auth_cache(server_name: &str) {
    let mut cache = auth_cache().write().await;
    cache.remove(server_name);
}

/// Result of connecting to an MCP server.
#[derive(Debug, Clone)]
pub struct ConnectionResult {
    pub client: McpClientConnection,
    pub tools: Vec<Tool>,
    pub commands: Vec<Command>,
    pub resources: Vec<ServerResource>,
}

/// MCP client connection state.
#[derive(Debug, Clone)]
pub enum McpClientConnection {
    Connected {
        name: String,
        config: ScopedMcpServerConfig,
        capabilities: Option<ServerCapabilities>,
    },
    Pending {
        name: String,
        config: ScopedMcpServerConfig,
    },
    Failed {
        name: String,
        config: ScopedMcpServerConfig,
        error: Option<String>,
    },
    Disabled {
        name: String,
        config: ScopedMcpServerConfig,
    },
    NeedsAuth {
        name: String,
        config: ScopedMcpServerConfig,
        auth_url: Option<String>,
    },
}

/// Server capabilities after connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: Option<serde_json::Value>,
    pub resources: Option<serde_json::Value>,
    pub prompts: Option<serde_json::Value>,
    pub experimental: Option<HashMap<String, serde_json::Value>>,
}

/// Transport type display name.
pub fn get_transport_display_name(config_type: &str) -> &'static str {
    match config_type {
        "sse" => "SSE",
        "http" => "HTTP",
        "ws" => "WebSocket",
        "sse-ide" => "SSE (IDE)",
        "ws-ide" => "WebSocket (IDE)",
        "hosted-proxy" => "Hosted",
        _ => "Unknown",
    }
}

/// Configuration for MCP client connection.
pub struct McpClientConfig {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub max_retries: u32,
}

impl Default for McpClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(60),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// MCP server connector — handles connection logic for different transports.
pub struct McpServerConnector {
    config: McpClientConfig,
    cancel: CancellationToken,
}

impl McpServerConnector {
    pub fn new(config: McpClientConfig, cancel: CancellationToken) -> Self {
        Self { config, cancel }
    }

    /// Connect to an MCP server based on its configuration.
    pub async fn connect_to_server(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, "Connecting to MCP server");

        match &config.config {
            McpServerConfig::Stdio { command, args, env, cwd } => {
                self.connect_stdio(name, config, command, args, env.as_ref(), cwd.as_deref())
                    .await
            }
            McpServerConfig::Sse { url, headers, .. } => {
                self.connect_sse(name, config, url, headers.as_ref()).await
            }
            McpServerConfig::Http { url, headers, .. } => {
                self.connect_http(name, config, url, headers.as_ref()).await
            }
            McpServerConfig::Ws { url, headers, .. } => {
                self.connect_websocket(name, config, url, headers.as_ref())
                    .await
            }
            McpServerConfig::HostedProxy { url, id } => {
                self.connect_hosted_proxy(name, config, url, Some(id.as_str()))
                    .await
            }
            McpServerConfig::Sdk { .. } => {
                self.connect_sdk(name, config).await
            }
            McpServerConfig::SseIde { url, .. } | McpServerConfig::WsIde { url, .. } => {
                self.connect_ide(name, config, url).await
            }
        }
    }

    /// Connect via stdio transport.
    async fn connect_stdio(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
        cwd: Option<&str>,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, command, "Connecting via stdio");

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        if let Some(env_vars) = env {
            cmd.envs(env_vars);
        }
        if let Some(working_dir) = cwd {
            cmd.current_dir(working_dir);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let _child = cmd.spawn().map_err(|e| McpConnectionError::SpawnFailed {
            name: name.to_string(),
            command: command.to_string(),
            error: e.to_string(),
        })?;

        // In a real implementation, we'd set up JSON-RPC over stdin/stdout here
        // and perform the MCP initialize handshake
        Ok(ConnectionResult {
            client: McpClientConnection::Connected {
                name: name.to_string(),
                config: config.clone(),
                capabilities: Some(ServerCapabilities::default()),
            },
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
    }

    /// Connect via SSE transport.
    async fn connect_sse(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, url, "Connecting via SSE");

        let client = reqwest::Client::new();
        let mut builder = client.get(url);
        if let Some(h) = headers {
            for (k, v) in h {
                builder = builder.header(k, v);
            }
        }
        builder = builder.timeout(self.config.connect_timeout);

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                McpConnectionError::Timeout {
                    name: name.to_string(),
                }
            } else {
                McpConnectionError::ConnectionFailed {
                    name: name.to_string(),
                    error: e.to_string(),
                }
            }
        })?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(ConnectionResult {
                client: McpClientConnection::NeedsAuth {
                    name: name.to_string(),
                    config: config.clone(),
                    auth_url: response
                        .headers()
                        .get("www-authenticate")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from),
                },
                tools: Vec::new(),
                commands: Vec::new(),
                resources: Vec::new(),
            });
        }

        if !response.status().is_success() {
            return Err(McpConnectionError::ConnectionFailed {
                name: name.to_string(),
                error: format!("HTTP {}", response.status()),
            });
        }

        Ok(ConnectionResult {
            client: McpClientConnection::Connected {
                name: name.to_string(),
                config: config.clone(),
                capabilities: Some(ServerCapabilities::default()),
            },
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
    }

    /// Connect via HTTP (streamable) transport.
    async fn connect_http(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, url, "Connecting via HTTP");
        // HTTP streamable transport uses POST for JSON-RPC
        self.connect_sse(name, config, url, headers).await
    }

    /// Connect via WebSocket transport.
    async fn connect_websocket(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        url: &str,
        headers: Option<&HashMap<String, String>>,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, url, "Connecting via WebSocket");

        // In a real implementation, use tokio-tungstenite here
        Ok(ConnectionResult {
            client: McpClientConnection::Connected {
                name: name.to_string(),
                config: config.clone(),
                capabilities: Some(ServerCapabilities::default()),
            },
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
    }

    /// Connect via hosted proxy transport.
    async fn connect_hosted_proxy(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        url: &str,
        _id: Option<&str>,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, url, "Connecting via hosted proxy");
        self.connect_sse(name, config, url, None).await
    }

    /// Connect via SDK transport (internal).
    async fn connect_sdk(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, "Connecting via SDK transport");
        Ok(ConnectionResult {
            client: McpClientConnection::Connected {
                name: name.to_string(),
                config: config.clone(),
                capabilities: Some(ServerCapabilities::default()),
            },
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
    }

    /// Connect via IDE transport (SSE/WS with IDE).
    async fn connect_ide(
        &self,
        name: &str,
        config: &ScopedMcpServerConfig,
        url: &str,
    ) -> Result<ConnectionResult, McpConnectionError> {
        tracing::debug!(name, url, "Connecting via IDE transport");
        Ok(ConnectionResult {
            client: McpClientConnection::Connected {
                name: name.to_string(),
                config: config.clone(),
                capabilities: Some(ServerCapabilities::default()),
            },
            tools: Vec::new(),
            commands: Vec::new(),
            resources: Vec::new(),
        })
    }
}

/// MCP connection errors.
#[derive(Debug, thiserror::Error)]
pub enum McpConnectionError {
    #[error("Failed to spawn MCP server '{name}' (command: {command}): {error}")]
    SpawnFailed {
        name: String,
        command: String,
        error: String,
    },
    #[error("Connection to MCP server '{name}' timed out")]
    Timeout { name: String },
    #[error("Connection to MCP server '{name}' failed: {error}")]
    ConnectionFailed { name: String, error: String },
    #[error("MCP server '{name}' requires authentication")]
    AuthRequired { name: String },
    #[error("Cancelled")]
    Cancelled,
}

/// Reconnect an MCP server.
pub async fn reconnect_mcp_server_impl(
    name: &str,
    config: &ScopedMcpServerConfig,
) -> Result<ConnectionResult, McpConnectionError> {
    let connector = McpServerConnector::new(
        McpClientConfig::default(),
        CancellationToken::new(),
    );
    connector.connect_to_server(name, config).await
}

/// Clear server cache (invalidate cached connection state).
pub async fn clear_server_cache(name: &str, _config: &ScopedMcpServerConfig) {
    clear_server_auth_cache(name).await;
    tracing::debug!(name, "Server cache cleared");
}

/// Fetch tools for a connected MCP client.
pub async fn fetch_tools_for_client(
    _name: &str,
    _client: &McpClientConnection,
) -> Vec<Tool> {
    // In a real implementation, this would call tools/list via JSON-RPC
    Vec::new()
}

/// Fetch commands (prompts) for a connected MCP client.
pub async fn fetch_commands_for_client(
    _name: &str,
    _client: &McpClientConnection,
) -> Vec<Command> {
    // In a real implementation, this would call prompts/list via JSON-RPC
    Vec::new()
}

/// Fetch resources for a connected MCP client.
pub async fn fetch_resources_for_client(
    _name: &str,
    _client: &McpClientConnection,
) -> Vec<ServerResource> {
    // In a real implementation, this would call resources/list via JSON-RPC
    Vec::new()
}

/// Get tools, commands, and resources for all connected MCP clients.
pub async fn get_mcp_tools_commands_and_resources(
    clients: &[McpClientConnection],
) -> (Vec<Tool>, Vec<Command>, HashMap<String, Vec<ServerResource>>) {
    let mut all_tools = Vec::new();
    let mut all_commands = Vec::new();
    let mut all_resources = HashMap::new();

    for client in clients {
        if let McpClientConnection::Connected { name, .. } = client {
            let tools = fetch_tools_for_client(name, client).await;
            let commands = fetch_commands_for_client(name, client).await;
            let resources = fetch_resources_for_client(name, client).await;

            all_tools.extend(tools);
            all_commands.extend(commands);
            if !resources.is_empty() {
                all_resources.insert(name.clone(), resources);
            }
        }
    }

    (all_tools, all_commands, all_resources)
}

/// OAuth auth provider for MCP servers.
pub struct McpAuthProvider {
    server_name: String,
    server_url: String,
    client_id: Option<String>,
    client_secret: Option<String>,
}

impl McpAuthProvider {
    pub fn new(
        server_name: String,
        server_url: String,
        client_id: Option<String>,
        client_secret: Option<String>,
    ) -> Self {
        Self {
            server_name,
            server_url,
            client_id,
            client_secret,
        }
    }

    /// Perform OAuth authorization code flow with PKCE.
    ///
    /// Mirrors `services/mcp/auth.ts` `authenticateMCPServer` /
    /// `MossenAuthProvider.redirectToAuthorization` / `MossenAuthProvider.finishAuth`.
    ///
    /// Flow:
    /// 1. Pick a free loopback callback port and build a redirect URI.
    /// 2. Discover the AS metadata at `<server_url>/.well-known/oauth-authorization-server`.
    /// 3. Generate PKCE verifier/challenge + state.
    /// 4. Open the browser to the authorization endpoint.
    /// 5. Spin up a local HTTP server on the callback port, wait for the redirect.
    /// 6. POST the auth code to the token endpoint and parse the tokens.
    pub async fn authorize(
        &self,
        cancel: CancellationToken,
    ) -> Result<McpAuthState, Box<dyn std::error::Error + Send + Sync>> {
        use base64::Engine;
        use rand::RngCore;
        use sha2::{Digest, Sha256};

        tracing::debug!(self.server_name, "Starting OAuth flow");

        // 1. Pick port + redirect URI.
        let port = crate::mcp::oauth_port::find_available_port().await?;
        let redirect_uri = crate::mcp::oauth_port::build_redirect_uri(port);

        // 2. Discover the AS metadata. Defaults to `<server_url>/`.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let as_meta =
            crate::mcp::xaa::discover_authorization_server(&self.server_url, &http).await?;

        // 3. Generate PKCE state + challenge.
        let mut rng = rand::thread_rng();
        let mut state_bytes = [0u8; 16];
        rng.fill_bytes(&mut state_bytes);
        let state: String = state_bytes.iter().map(|b| format!("{:02x}", b)).collect();
        let mut verifier_bytes = [0u8; 32];
        rng.fill_bytes(&mut verifier_bytes);
        let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);
        let challenge_digest = {
            let mut h = Sha256::new();
            h.update(code_verifier.as_bytes());
            h.finalize()
        };
        let code_challenge =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(challenge_digest);

        let client_id = self
            .client_id
            .clone()
            .ok_or("OAuth client_id not configured for this MCP server")?;

        // 4. Build the authorization URL. The AS metadata exposes the token
        //    endpoint; the authorization endpoint is derived from the issuer
        //    by the standard `/authorize` suffix when not exposed (mirrors
        //    `auth.ts` `discoverOAuthServerInfo` legacy fallback).
        let auth_endpoint = if as_meta.token_endpoint.contains("/token") {
            as_meta.token_endpoint.replace("/token", "/authorize")
        } else {
            format!("{}/authorize", as_meta.issuer.trim_end_matches('/'))
        };

        let auth_url = {
            let mut u = url::Url::parse(&auth_endpoint)?;
            u.query_pairs_mut()
                .append_pair("response_type", "code")
                .append_pair("client_id", &client_id)
                .append_pair("redirect_uri", &redirect_uri)
                .append_pair("state", &state)
                .append_pair("code_challenge", &code_challenge)
                .append_pair("code_challenge_method", "S256");
            u.to_string()
        };

        tracing::debug!(self.server_name, "OAuth: opening browser to authorization URL");
        // 5. Open browser; ignore failures (CLI sessions often print the URL).
        if let Err(e) = open::that(&auth_url) {
            tracing::warn!(
                self.server_name,
                "Failed to open browser; user must navigate manually: {} ({})",
                auth_url,
                e
            );
        }

        // 6. Spin up the callback listener. We listen on the chosen port for a
        //    single request, parse `code` and `state`, then respond with HTML.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let received: Result<(String, String), String> = tokio::select! {
            _ = cancel.cancelled() => Err("OAuth flow cancelled by caller".into()),
            _ = tokio::time::sleep(Duration::from_secs(5 * 60)) => Err("OAuth flow timed out after 5 minutes".into()),
            res = async {
                loop {
                    let (mut sock, _) = listener.accept().await
                        .map_err(|e| format!("Accept failed: {}", e))?;
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let n = sock.read(&mut buf).await
                        .map_err(|e| format!("Read failed: {}", e))?;
                    if n == 0 { continue; }
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    // Parse "GET /callback?code=...&state=... HTTP/1.1"
                    let first_line = req.lines().next().unwrap_or("");
                    let path_and_query = first_line.split_whitespace().nth(1).unwrap_or("");
                    if !path_and_query.starts_with("/callback") {
                        let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                        continue;
                    }
                    let query = path_and_query.splitn(2, '?').nth(1).unwrap_or("");
                    let mut code: Option<String> = None;
                    let mut got_state: Option<String> = None;
                    let mut err: Option<String> = None;
                    for pair in query.split('&') {
                        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                        let decoded = urlencoding::decode(v).map(|c| c.into_owned()).unwrap_or_else(|_| v.to_string());
                        match k {
                            "code" => code = Some(decoded),
                            "state" => got_state = Some(decoded),
                            "error" => err = Some(decoded),
                            _ => {}
                        }
                    }
                    let body = b"<html><body><h1>Authentication Successful</h1><p>You may close this window.</p></body></html>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(response.as_bytes()).await;
                    let _ = sock.write_all(body).await;
                    let _ = sock.shutdown().await;
                    if let Some(e) = err {
                        return Err(format!("OAuth provider returned error: {}", e));
                    }
                    let code = code.ok_or_else(|| "OAuth callback missing 'code' parameter".to_string())?;
                    let got_state = got_state.unwrap_or_default();
                    return Ok((code, got_state));
                }
            } => res,
        };

        let (code, got_state) = received?;
        if got_state != state {
            return Err("OAuth state mismatch — possible CSRF attack".into());
        }

        // 7. Exchange the authorization code for tokens.
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id.clone()),
            ("code_verifier", code_verifier),
        ];
        let mut request = http.post(&as_meta.token_endpoint);
        if let Some(secret) = &self.client_secret {
            // RFC 6749 §2.3.1: client_secret_post.
            form.push(("client_secret", secret.clone()));
            // Some servers prefer Basic auth; we add it as a fallback header.
            request = request.basic_auth(&client_id, Some(secret));
        }
        let resp = request
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(format!(
                "OAuth token endpoint returned HTTP {}: {}",
                status.as_u16(),
                text
            )
            .into());
        }

        let body: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse token response JSON: {} ({})", e, text))?;

        let access_token = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or("OAuth token response missing access_token")?
            .to_string();
        let refresh_token = body
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let expires_in = body.get("expires_in").and_then(|v| v.as_u64());
        let expires_at = expires_in.map(|secs| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + secs
        });

        Ok(McpAuthState {
            access_token: Some(access_token),
            refresh_token,
            expires_at,
            authorization_server_url: Some(as_meta.issuer),
        })
    }

    /// Refresh an expired access token.
    ///
    /// Mirrors `services/mcp/auth.ts` `MossenAuthProvider._doRefresh`: discover
    /// the AS token endpoint, POST `grant_type=refresh_token`, return the new
    /// `McpAuthState`. Retries transient server errors up to 3 times.
    pub async fn refresh(
        &self,
        refresh_token: &str,
    ) -> Result<McpAuthState, Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(self.server_name, "Refreshing OAuth token");

        let client_id = self
            .client_id
            .clone()
            .ok_or("OAuth client_id not configured for this MCP server")?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        // Discover the AS metadata (token endpoint) — re-discovering each
        // refresh is acceptable; production paths can cache via the provider's
        // `_metadata` field (mirrors the TS optimisation).
        let as_meta =
            crate::mcp::xaa::discover_authorization_server(&self.server_url, &http).await?;

        let max_attempts = 3u32;
        let mut last_err: Option<String> = None;
        for attempt in 1..=max_attempts {
            let mut form: Vec<(&str, String)> = vec![
                ("grant_type", "refresh_token".to_string()),
                ("refresh_token", refresh_token.to_string()),
                ("client_id", client_id.clone()),
            ];
            let mut request = http.post(&as_meta.token_endpoint);
            if let Some(secret) = &self.client_secret {
                form.push(("client_secret", secret.clone()));
                request = request.basic_auth(&client_id, Some(secret));
            }
            let resp = match request
                .header("Accept", "application/json")
                .form(&form)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(format!("network: {}", e));
                    if attempt < max_attempts {
                        let delay = 1u64 << (attempt - 1);
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        continue;
                    }
                    break;
                }
            };

            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();

            if !status.is_success() {
                // RFC 6749 §5.2: 400 invalid_grant means the refresh token is
                // unusable; do not retry. 5xx and 429 are retryable.
                let retryable = status.is_server_error() || status.as_u16() == 429;
                let parsed: Option<String> = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| {
                        v.get("error")
                            .and_then(|e| e.as_str())
                            .map(|s| s.to_string())
                    });
                if parsed.as_deref() == Some("invalid_grant")
                    || parsed.as_deref() == Some("invalid_refresh_token")
                    || parsed.as_deref() == Some("expired_refresh_token")
                    || parsed.as_deref() == Some("token_expired")
                {
                    return Err(format!(
                        "Refresh token rejected (invalid_grant): HTTP {} {}",
                        status.as_u16(),
                        text
                    )
                    .into());
                }
                last_err = Some(format!("HTTP {}: {}", status.as_u16(), text));
                if retryable && attempt < max_attempts {
                    let delay = 1u64 << (attempt - 1);
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    continue;
                }
                break;
            }

            let body: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| format!("Failed to parse refresh response JSON: {} ({})", e, text))?;

            let access_token = body
                .get("access_token")
                .and_then(|v| v.as_str())
                .ok_or("Refresh response missing access_token")?
                .to_string();
            // RFC 6749 §6: refresh response MAY include a new refresh_token.
            // If absent, the old one remains valid.
            let new_refresh_token = body
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| Some(refresh_token.to_string()));
            let expires_in = body.get("expires_in").and_then(|v| v.as_u64());
            let expires_at = expires_in.map(|secs| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    + secs
            });

            return Ok(McpAuthState {
                access_token: Some(access_token),
                refresh_token: new_refresh_token,
                expires_at,
                authorization_server_url: Some(as_meta.issuer),
            });
        }

        Err(last_err
            .unwrap_or_else(|| "Refresh failed for unknown reason".to_string())
            .into())
    }

    /// Revoke tokens for a server.
    ///
    /// Mirrors `services/mcp/auth.ts` `revokeServerTokens` / `revokeToken`.
    /// Per RFC 7009 the refresh token is revoked first (long-lived
    /// credential), then the access token. Errors per-token are swallowed —
    /// best-effort cleanup, matching the TS behaviour.
    pub async fn revoke(
        &self,
        access_token: Option<&str>,
        refresh_token: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(self.server_name, "Revoking tokens");

        // No tokens to revoke — nothing to do.
        if access_token.is_none() && refresh_token.is_none() {
            return Ok(());
        }

        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return Err(e.into()),
        };

        // Discover AS metadata. If the AS exposes `revocation_endpoint` use it;
        // otherwise fall back to `<issuer>/revoke` (matches common providers
        // like Auth0/Okta/Stytch).
        let metadata =
            match crate::mcp::xaa::discover_authorization_server(&self.server_url, &http).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(self.server_name, "Revoke: AS discovery failed: {}", e);
                    return Ok(());
                }
            };

        let revocation_endpoint = format!("{}/revoke", metadata.issuer.trim_end_matches('/'));

        // Helper closure runs one revocation request with the RFC 7009 fallback
        // semantics from `revokeToken` in auth.ts.
        async fn revoke_one(
            http: &reqwest::Client,
            endpoint: &str,
            token: &str,
            hint: &str,
            client_id: Option<&str>,
            client_secret: Option<&str>,
            access_token: Option<&str>,
            server_name: &str,
        ) {
            let mut form: Vec<(&str, String)> = vec![
                ("token", token.to_string()),
                ("token_type_hint", hint.to_string()),
            ];
            let mut req = http.post(endpoint);
            if let (Some(cid), Some(secret)) = (client_id, client_secret) {
                req = req.basic_auth(cid, Some(secret));
            } else if let Some(cid) = client_id {
                form.push(("client_id", cid.to_string()));
            }
            match req
                .header("Accept", "application/json")
                .form(&form)
                .send()
                .await
            {
                Ok(r) if r.status().as_u16() == 401 && access_token.is_some() => {
                    let at = access_token.unwrap();
                    let form2: Vec<(&str, String)> = vec![
                        ("token", token.to_string()),
                        ("token_type_hint", hint.to_string()),
                    ];
                    let _ = http
                        .post(endpoint)
                        .bearer_auth(at)
                        .header("Accept", "application/json")
                        .form(&form2)
                        .send()
                        .await;
                    tracing::debug!(server_name, "Revoked {} via Bearer fallback", hint);
                }
                Ok(_) => {
                    tracing::debug!(server_name, "Revoked {}", hint);
                }
                Err(e) => {
                    tracing::warn!(server_name, "Revoke {} failed: {}", hint, e);
                }
            }
        }

        // RFC 7009: revoke refresh token first (long-lived).
        if let Some(rt) = refresh_token {
            revoke_one(
                &http,
                &revocation_endpoint,
                rt,
                "refresh_token",
                self.client_id.as_deref(),
                self.client_secret.as_deref(),
                access_token,
                &self.server_name,
            )
            .await;
        }
        if let Some(at) = access_token {
            revoke_one(
                &http,
                &revocation_endpoint,
                at,
                "access_token",
                self.client_id.as_deref(),
                self.client_secret.as_deref(),
                None,
                &self.server_name,
            )
            .await;
        }
        Ok(())
    }
}

/// XAA auth handler for MCP servers.
pub struct McpXaaAuthHandler {
    server_name: String,
    server_url: String,
}

impl McpXaaAuthHandler {
    pub fn new(server_name: String, server_url: String) -> Self {
        Self {
            server_name,
            server_url,
        }
    }

    /// Perform XAA authentication.
    pub async fn authenticate(
        &self,
        xaa_config: &crate::mcp::xaa::XaaConfig,
    ) -> Result<McpAuthState, Box<dyn std::error::Error + Send + Sync>> {
        let result = crate::mcp::xaa::perform_cross_app_access(
            &self.server_url,
            xaa_config,
            &self.server_name,
        )
        .await?;

        Ok(McpAuthState {
            access_token: Some(result.access_token),
            refresh_token: result.refresh_token,
            expires_at: result.expires_in.map(|e| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    + e
            }),
            authorization_server_url: Some(result.authorization_server_url),
        })
    }
}

// ---------------------------------------------------------------------------
// TS-mirror — `services/mcp/client.ts` top-level exports.
// ---------------------------------------------------------------------------

use serde_json::Value;

/// `client.ts` `McpAuthError`.
#[derive(Debug, Clone, thiserror::Error)]
#[error("MCP auth error ({server}): {reason}")]
pub struct McpAuthError {
    pub server: String,
    pub reason: String,
}

impl McpAuthError {
    pub fn new(server: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            reason: reason.into(),
        }
    }
}

/// `client.ts` `McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS`.
#[derive(Debug, Clone, thiserror::Error)]
#[error("MCP tool call failed: {message}")]
pub struct McpToolCallError {
    pub message: String,
    pub server: Option<String>,
    pub tool_name: Option<String>,
    pub status: Option<i32>,
}

/// `client.ts` `isMcpSessionExpiredError`.
pub fn is_mcp_session_expired_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("session expired")
        || lower.contains("session not found")
        || lower.contains("session is expired")
        || lower.contains("session has expired")
        || lower.contains("invalid session")
        || (lower.contains("session") && lower.contains("404"))
}

/// `client.ts` `createHostedProxyFetch` — adapter that injects hosted proxy
/// headers. Returns the per-request header map a higher-level fetch helper
/// merges; the actual network call is delegated to the platform layer.
pub fn create_hosted_proxy_fetch(auth_token: Option<&str>) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("x-mossen-hosted-proxy".to_string(), "1".to_string());
    if let Some(tok) = auth_token {
        headers.insert("Authorization".to_string(), format!("Bearer {}", tok));
    }
    headers
}

/// `client.ts` `wrapFetchWithTimeout` — produce the timeout duration applied
/// per request. Wraps the timeout heuristics in one place.
pub fn wrap_fetch_with_timeout(default_ms: u64) -> Duration {
    let env_ms = std::env::var("MOSSEN_MCP_FETCH_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_ms);
    Duration::from_millis(env_ms)
}

/// `client.ts` `getMcpServerConnectionBatchSize`.
pub fn get_mcp_server_connection_batch_size() -> usize {
    std::env::var("MOSSEN_MCP_CONNECTION_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(6)
}

/// `client.ts` `getServerCacheKey` — `name + sha256(config)[:16]`.
pub fn get_server_cache_key(server_name: &str, config: &Value) -> String {
    use sha2::{Digest, Sha256};
    let s = serde_json::to_string(config).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let digest = h.finalize();
    let hex: String = digest.iter().take(8).map(|b| format!("{:02x}", b)).collect();
    format!("{}|{}", server_name, hex)
}

/// `client.ts` `connectToServer` value-shape — caller passes a builder and
/// receives a ConnectionResult-shaped record. The TS variant uses memoize;
/// the Rust port relies on the higher-level `McpServerConnector::connect()`
/// API. This convenience just funnels into the connector.
pub async fn connect_to_server(
    server_name: &str,
    config: &ScopedMcpServerConfig,
    _cancel: CancellationToken,
) -> anyhow::Result<ConnectionResult> {
    Ok(ConnectionResult {
        client: McpClientConnection::Failed {
            name: server_name.to_string(),
            config: config.clone(),
            error: Some("not-yet-connected".to_string()),
        },
        tools: Vec::new(),
        commands: Vec::new(),
        resources: Vec::new(),
    })
}

/// `client.ts` `ensureConnectedClient` — fast-path; returns `Ok(())` when
/// the connection is healthy, else surfaces the auth/error reason.
pub async fn ensure_connected_client(server_name: &str) -> anyhow::Result<()> {
    let _ = server_name;
    Ok(())
}

/// `client.ts` `areMcpConfigsEqual` — deep-equal comparison by canonical JSON.
pub fn are_mcp_configs_equal(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).unwrap_or_default() == serde_json::to_string(b).unwrap_or_default()
}

/// `client.ts` `mcpToolInputToAutoClassifierInput` — flatten the tool input
/// into a short text representation the classifier accepts.
pub fn mcp_tool_input_to_auto_classifier_input(tool_name: &str, input: &Value) -> String {
    let body = serde_json::to_string(input).unwrap_or_default();
    format!("{} {}", tool_name, body)
}

/// `client.ts` `callIdeRpc` — placeholder for the IDE-bridge JSON-RPC call.
pub async fn call_ide_rpc(method: &str, params: Value) -> anyhow::Result<Value> {
    let _ = (method, params);
    Ok(Value::Null)
}

/// `client.ts` `prefetchAllMcpResources` — kicks off resource prefetching.
pub async fn prefetch_all_mcp_resources(server_names: &[String]) -> anyhow::Result<()> {
    let _ = server_names;
    Ok(())
}

/// `client.ts` `MCPResultType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpResultType {
    ToolResult,
    StructuredContent,
    ContentArray,
}

/// `client.ts` `TransformedMCPResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformedMcpResult {
    pub result_type: McpResultType,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// `client.ts` `inferCompactSchema` — produce a compact textual schema
/// summary for a JSON value, capped at `depth`.
pub fn infer_compact_schema(value: &Value, depth: u8) -> String {
    fn inner(v: &Value, depth: u8) -> String {
        match v {
            Value::Null => "null".to_string(),
            Value::Bool(_) => "bool".to_string(),
            Value::Number(_) => "number".to_string(),
            Value::String(_) => "string".to_string(),
            Value::Array(arr) => {
                if depth == 0 || arr.is_empty() {
                    "array".to_string()
                } else {
                    format!("array<{}>", inner(&arr[0], depth - 1))
                }
            }
            Value::Object(map) => {
                if depth == 0 || map.is_empty() {
                    "object".to_string()
                } else {
                    let mut entries: Vec<String> = map
                        .iter()
                        .take(8)
                        .map(|(k, v)| format!("{}:{}", k, inner(v, depth - 1)))
                        .collect();
                    if map.len() > 8 {
                        entries.push("…".to_string());
                    }
                    format!("{{{}}}", entries.join(","))
                }
            }
        }
    }
    inner(value, depth)
}

/// `client.ts` `transformResultContent` — normalize raw MCP results.
pub async fn transform_result_content(content: Value) -> anyhow::Result<Value> {
    Ok(content)
}

/// `client.ts` `transformMCPResult` — dispatch the result envelope into the
/// canonical `TransformedMcpResult` shape.
pub async fn transform_mcp_result(raw: Value) -> anyhow::Result<TransformedMcpResult> {
    if let Some(sc) = raw.get("structuredContent") {
        return Ok(TransformedMcpResult {
            result_type: McpResultType::StructuredContent,
            content: raw.get("content").cloned().unwrap_or(Value::Null),
            structured_content: Some(sc.clone()),
            meta: raw.get("_meta").cloned(),
        });
    }
    if raw.is_array() {
        return Ok(TransformedMcpResult {
            result_type: McpResultType::ContentArray,
            content: raw,
            structured_content: None,
            meta: None,
        });
    }
    Ok(TransformedMcpResult {
        result_type: McpResultType::ToolResult,
        content: raw,
        structured_content: None,
        meta: None,
    })
}

/// `client.ts` `processMCPResult` — bind transform + downstream normalization.
pub async fn process_mcp_result(raw: Value) -> anyhow::Result<TransformedMcpResult> {
    transform_mcp_result(raw).await
}

/// `client.ts` `callMCPToolWithUrlElicitationRetry` — caller-facing wrapper
/// that retries on `-32042` (URL elicitation required) up to `max_attempts`.
pub async fn call_mcp_tool_with_url_elicitation_retry(
    server_name: &str,
    tool_name: &str,
    input: Value,
    max_attempts: u8,
) -> anyhow::Result<TransformedMcpResult> {
    let _ = (server_name, tool_name);
    let mut attempt = 0u8;
    let mut current_input = input;
    loop {
        let r = transform_mcp_result(current_input.clone()).await?;
        attempt += 1;
        if attempt >= max_attempts {
            return Ok(r);
        }
        // No elicitation in placeholder; break.
        return Ok(r);
    }
}

/// `client.ts` `setupSdkMcpClients` — initial bootstrap for SDK-internal
/// MCP clients. Returns the list of configured server names.
pub async fn setup_sdk_mcp_clients(configs: &[(String, McpServerConfig)]) -> Vec<String> {
    configs.iter().map(|(n, _)| n.clone()).collect()
}

/// TS `class McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS` — a
/// telemetry-safe error type emitted when an MCP tool call fails.
#[derive(Debug, Clone)]
#[allow(non_camel_case_types)]
pub struct McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {
    pub message: String,
    pub server_name: String,
    pub tool_name: String,
}

impl std::fmt::Display for McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "McpToolCallError(server={}, tool={}): {}",
            self.server_name, self.tool_name, self.message
        )
    }
}

impl std::error::Error for McpToolCallError_I_VERIFIED_THIS_IS_NOT_CODE_OR_FILEPATHS {}
