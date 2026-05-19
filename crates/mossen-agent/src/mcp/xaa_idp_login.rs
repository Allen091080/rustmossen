//! XAA IdP Login — acquires an OIDC id_token from an enterprise IdP.
//!
//! Translates `services/mcp/xaaIdpLogin.ts`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::mcp::oauth_port::{build_redirect_uri, find_available_port};

const IDP_LOGIN_TIMEOUT_MS: u64 = 5 * 60 * 1000;
const IDP_REQUEST_TIMEOUT_MS: u64 = 30000;
const ID_TOKEN_EXPIRY_BUFFER_S: u64 = 60;

/// Check if XAA is enabled via environment variable.
pub fn is_xaa_enabled() -> bool {
    std::env::var("MOSSEN_CODE_ENABLE_XAA")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// XAA IdP settings from configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaaIdpSettings {
    pub issuer: String,
    pub client_id: String,
    pub callback_port: Option<u16>,
}

/// Options for IdP login flow.
#[derive(Debug, Clone)]
pub struct IdpLoginOptions {
    pub idp_issuer: String,
    pub idp_client_id: String,
    pub idp_client_secret: Option<String>,
    pub callback_port: Option<u16>,
    pub skip_browser_open: bool,
}

/// Normalize an IdP issuer URL for use as a cache key.
pub fn issuer_key(issuer: &str) -> String {
    match url::Url::parse(issuer) {
        Ok(mut u) => {
            let path = u.path().trim_end_matches('/').to_string();
            u.set_path(&path);
            u.to_string()
        }
        Err(_) => issuer.trim_end_matches('/').to_string(),
    }
}

/// Cached IdP token entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpTokenEntry {
    pub id_token: String,
    pub expires_at: u64,
}

/// Storage trait for IdP tokens.
pub trait IdpTokenStorage: Send + Sync {
    fn read_token(&self, issuer_key: &str) -> Option<IdpTokenEntry>;
    fn save_token(&self, issuer_key: &str, entry: IdpTokenEntry);
    fn clear_token(&self, issuer_key: &str);
    fn read_client_secret(&self, issuer_key: &str) -> Option<String>;
    fn save_client_secret(&self, issuer_key: &str, secret: &str) -> Result<(), String>;
    fn clear_client_secret(&self, issuer_key: &str);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Read a cached id_token for the given IdP issuer from secure storage.
/// Returns None if missing or within expiry buffer.
pub fn get_cached_idp_id_token(storage: &dyn IdpTokenStorage, idp_issuer: &str) -> Option<String> {
    let key = issuer_key(idp_issuer);
    let entry = storage.read_token(&key)?;
    let remaining_ms = entry.expires_at.saturating_sub(now_ms());
    if remaining_ms <= ID_TOKEN_EXPIRY_BUFFER_S * 1000 {
        return None;
    }
    Some(entry.id_token)
}

/// Save an externally-obtained id_token into the XAA cache.
pub fn save_idp_id_token_from_jwt(
    storage: &dyn IdpTokenStorage,
    idp_issuer: &str,
    id_token: &str,
) -> u64 {
    let exp = jwt_exp(id_token);
    let expires_at = match exp {
        Some(e) => e * 1000,
        None => now_ms() + 3600 * 1000,
    };
    let key = issuer_key(idp_issuer);
    storage.save_token(
        &key,
        IdpTokenEntry {
            id_token: id_token.to_string(),
            expires_at,
        },
    );
    expires_at
}

/// Clear cached id_token for the given issuer.
pub fn clear_idp_id_token(storage: &dyn IdpTokenStorage, idp_issuer: &str) {
    let key = issuer_key(idp_issuer);
    storage.clear_token(&key);
}

/// Save an IdP client secret to secure storage.
pub fn save_idp_client_secret(
    storage: &dyn IdpTokenStorage,
    idp_issuer: &str,
    client_secret: &str,
) -> Result<(), String> {
    let key = issuer_key(idp_issuer);
    storage.save_client_secret(&key, client_secret)
}

/// Read the IdP client secret.
pub fn get_idp_client_secret(storage: &dyn IdpTokenStorage, idp_issuer: &str) -> Option<String> {
    let key = issuer_key(idp_issuer);
    storage.read_client_secret(&key)
}

/// Remove the IdP client secret.
pub fn clear_idp_client_secret(storage: &dyn IdpTokenStorage, idp_issuer: &str) {
    let key = issuer_key(idp_issuer);
    storage.clear_client_secret(&key);
}

/// OIDC Discovery metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

/// Discover OIDC metadata from issuer.
pub async fn discover_oidc(
    idp_issuer: &str,
) -> Result<OidcMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let base = if idp_issuer.ends_with('/') {
        idp_issuer.to_string()
    } else {
        format!("{}/", idp_issuer)
    };
    let url = format!("{}.well-known/openid-configuration", base);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .timeout(Duration::from_millis(IDP_REQUEST_TIMEOUT_MS))
        .send()
        .await
        .map_err(|e| format!("XAA IdP: OIDC discovery failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "XAA IdP: OIDC discovery failed: HTTP {} at {}",
            resp.status(),
            url
        )
        .into());
    }

    let metadata: OidcMetadata = resp
        .json()
        .await
        .map_err(|_| format!("XAA IdP: OIDC discovery returned non-JSON at {}", url))?;

    let te_url =
        url::Url::parse(&metadata.token_endpoint).map_err(|_| "Invalid token_endpoint URL")?;
    if te_url.scheme() != "https" {
        return Err(format!(
            "XAA IdP: refusing non-HTTPS token endpoint: {}",
            metadata.token_endpoint
        )
        .into());
    }

    Ok(metadata)
}

/// Decode the exp claim from a JWT without verifying.
fn jwt_exp(jwt: &str) -> Option<u64> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    use base64::Engine;
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    payload["exp"].as_u64()
}

/// PKCE code verifier and challenge generation.
fn generate_pkce() -> (String, String) {
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let mut rng = rand::thread_rng();
    let verifier_bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&verifier_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

/// Generate a random state parameter.
fn generate_state() -> String {
    use base64::Engine;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
}

/// Acquire an id_token from the IdP: return cached if valid, otherwise run OIDC flow.
pub async fn acquire_idp_id_token(
    opts: &IdpLoginOptions,
    storage: &dyn IdpTokenStorage,
    on_authorization_url: impl Fn(&str),
    cancel: CancellationToken,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Check cache
    if let Some(cached) = get_cached_idp_id_token(storage, &opts.idp_issuer) {
        tracing::debug!("xaa: Using cached id_token for {}", opts.idp_issuer);
        return Ok(cached);
    }

    tracing::debug!(
        "xaa: No cached id_token for {}; starting OIDC login",
        opts.idp_issuer
    );

    let metadata = discover_oidc(&opts.idp_issuer).await?;
    let port = match opts.callback_port {
        Some(p) => p,
        None => find_available_port().await?,
    };
    let redirect_uri = build_redirect_uri(port);
    let state = generate_state();
    let (code_verifier, code_challenge) = generate_pkce();

    // Build authorization URL
    let mut auth_url = url::Url::parse(&metadata.authorization_endpoint)?;
    auth_url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &opts.idp_client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", "openid")
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");

    // Start callback server
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    // Notify caller of URL
    on_authorization_url(auth_url.as_str());

    if !opts.skip_browser_open {
        tracing::debug!("xaa: Opening browser to IdP authorization endpoint");
        let _ = open::that(auth_url.as_str());
    }

    // Wait for callback
    let code = wait_for_callback(listener, &state, cancel.clone()).await?;

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &opts.idp_client_id),
        ("code_verifier", &code_verifier),
    ];
    let secret_str;
    if let Some(ref secret) = opts.idp_client_secret {
        secret_str = secret.clone();
        params.push(("client_secret", &secret_str));
    }

    let resp = client
        .post(&metadata.token_endpoint)
        .form(&params)
        .timeout(Duration::from_millis(IDP_REQUEST_TIMEOUT_MS))
        .send()
        .await
        .map_err(|e| format!("XAA IdP: token exchange failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("XAA IdP: token exchange HTTP {}", resp.status()).into());
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("XAA IdP: token response parse failed: {}", e))?;

    let id_token = body["id_token"]
        .as_str()
        .ok_or("XAA IdP: token response missing id_token (check scope=openid)")?
        .to_string();

    // Cache the token
    let exp = jwt_exp(&id_token);
    let expires_at = match exp {
        Some(e) => e * 1000,
        None => {
            let expires_in = body["expires_in"].as_u64().unwrap_or(3600);
            now_ms() + expires_in * 1000
        }
    };

    let key = issuer_key(&opts.idp_issuer);
    storage.save_token(
        &key,
        IdpTokenEntry {
            id_token: id_token.clone(),
            expires_at,
        },
    );
    tracing::debug!("xaa: Cached id_token for {}", opts.idp_issuer);

    Ok(id_token)
}

/// Wait for OAuth callback with authorization code.
async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
    cancel: CancellationToken,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let timeout = tokio::time::sleep(Duration::from_millis(IDP_LOGIN_TIMEOUT_MS));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => {
                return Err("XAA IdP: login timed out".into());
            }
            _ = cancel.cancelled() => {
                return Err("XAA IdP: login cancelled".into());
            }
            accept_result = listener.accept() => {
                let (mut stream, _) = accept_result?;
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await?;
                let request = String::from_utf8_lossy(&buf[..n]);

                // Parse the request line
                let first_line = request.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("");

                if !path.starts_with("/callback") {
                    let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes()).await;
                    continue;
                }

                // Parse query parameters
                let query = path.split('?').nth(1).unwrap_or("");
                let params: std::collections::HashMap<&str, &str> = query
                    .split('&')
                    .filter_map(|p| {
                        let mut parts = p.splitn(2, '=');
                        Some((parts.next()?, parts.next().unwrap_or("")))
                    })
                    .collect();

                if let Some(err) = params.get("error") {
                    let desc = params.get("error_description").unwrap_or(&"");
                    let body = format!(
                        "<html><body><h3>IdP login failed</h3><p>{}</p><p>{}</p></body></html>",
                        err, desc
                    );
                    let response = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(), body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Err(format!("XAA IdP: {} — {}", err, desc).into());
                }

                let state = params.get("state").copied().unwrap_or("");
                if state != expected_state {
                    let body = "<html><body><h3>State mismatch</h3></body></html>";
                    let response = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(), body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    return Err("XAA IdP: state mismatch (possible CSRF)".into());
                }

                let code = match params.get("code") {
                    Some(c) if !c.is_empty() => c.to_string(),
                    _ => {
                        let body = "<html><body><h3>Missing code</h3></body></html>";
                        let response = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(), body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        return Err("XAA IdP: callback missing code".into());
                    }
                };

                let body = "<html><body><h3>IdP login complete — you can close this window.</h3></body></html>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                return Ok(code);
            }
        }
    }
}

/// TS `getXaaIdpSettings` — load the configured XAA IDP settings from the
/// environment. Returns `None` when the feature is disabled (no `MOSSEN_XAA_*`
/// env vars present).
pub fn get_xaa_idp_settings() -> Option<XaaIdpSettings> {
    let issuer = std::env::var("MOSSEN_XAA_ISSUER").ok()?;
    let client_id = std::env::var("MOSSEN_XAA_CLIENT_ID").ok().unwrap_or_default();
    let callback_port = std::env::var("MOSSEN_XAA_CALLBACK_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok());
    Some(XaaIdpSettings {
        issuer,
        client_id,
        callback_port,
    })
}
