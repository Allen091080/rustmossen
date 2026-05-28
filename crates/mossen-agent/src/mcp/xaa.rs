//! Cross-App Access (XAA) / Enterprise Managed Authorization (SEP-990).
//!
//! Translates `services/mcp/xaa.ts`.

use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

use mossen_utils::string_utils::truncate_chars;

const XAA_REQUEST_TIMEOUT_MS: u64 = 30000;
const TOKEN_EXCHANGE_GRANT: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const ID_JAG_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id-jag";
const ID_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:id_token";

/// Error from the IdP token-exchange leg.
#[derive(Debug, thiserror::Error)]
#[error("XaaTokenExchangeError: {message}")]
pub struct XaaTokenExchangeError {
    pub message: String,
    pub should_clear_id_token: bool,
}

/// Redact sensitive tokens from strings for logging.
fn redact_tokens(raw: &str) -> String {
    lazy_static::lazy_static! {
        static ref RE: Regex = Regex::new(
            r#""(access_token|refresh_token|id_token|assertion|subject_token|client_secret)"\s*:\s*"[^"]*""#
        ).unwrap();
    }
    RE.replace_all(raw, |caps: &regex::Captures| {
        format!("\"{}\":\"[REDACTED]\"", &caps[1])
    })
    .to_string()
}

/// Normalize URL for comparison (RFC 8414 §3.3).
fn normalize_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => u.to_string().trim_end_matches('/').to_string(),
        Err(_) => url.trim_end_matches('/').to_string(),
    }
}

// ─── Schemas ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: Option<String>,
    issued_token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JwtBearerResponse {
    access_token: String,
    #[serde(default = "default_token_type")]
    token_type: String,
    expires_in: Option<u64>,
    scope: Option<String>,
    refresh_token: Option<String>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

// ─── Discovery ──────────────────────────────────────────────────────────

/// Protected Resource Metadata (RFC 9728).
#[derive(Debug, Clone)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
}

/// Discover PRM for a server URL.
pub async fn discover_protected_resource(
    server_url: &str,
    client: &reqwest::Client,
) -> Result<ProtectedResourceMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!(
        "{}/.well-known/oauth-protected-resource",
        server_url.trim_end_matches('/')
    );

    let resp = client
        .get(&url)
        .timeout(Duration::from_millis(XAA_REQUEST_TIMEOUT_MS))
        .send()
        .await
        .map_err(|e| format!("XAA: PRM discovery failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("XAA: PRM discovery failed: HTTP {}", resp.status()).into());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("XAA: PRM discovery failed: {}", e))?;

    let resource = body["resource"]
        .as_str()
        .ok_or("XAA: PRM missing resource")?
        .to_string();
    let auth_servers: Vec<String> = body["authorization_servers"]
        .as_array()
        .ok_or("XAA: PRM missing authorization_servers")?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();

    if auth_servers.is_empty() {
        return Err("XAA: PRM missing authorization_servers".into());
    }

    if normalize_url(&resource) != normalize_url(server_url) {
        return Err(format!(
            "XAA: PRM resource mismatch: expected {}, got {}",
            server_url, resource
        )
        .into());
    }

    Ok(ProtectedResourceMetadata {
        resource,
        authorization_servers: auth_servers,
    })
}

/// Authorization Server Metadata.
#[derive(Debug, Clone)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub token_endpoint: String,
    pub grant_types_supported: Option<Vec<String>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

/// Discover AS metadata (RFC 8414).
pub async fn discover_authorization_server(
    as_url: &str,
    client: &reqwest::Client,
) -> Result<AuthorizationServerMetadata, Box<dyn std::error::Error + Send + Sync>> {
    let base = if as_url.ends_with('/') {
        as_url.to_string()
    } else {
        format!("{}/", as_url)
    };
    let url = format!("{}.well-known/oauth-authorization-server", base);

    let resp = client
        .get(&url)
        .timeout(Duration::from_millis(XAA_REQUEST_TIMEOUT_MS))
        .send()
        .await
        .map_err(|e| format!("XAA: AS metadata discovery failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("XAA: AS metadata discovery failed: HTTP {}", resp.status()).into());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("XAA: AS metadata discovery failed: {}", e))?;

    let issuer = body["issuer"]
        .as_str()
        .ok_or("XAA: AS metadata missing issuer")?
        .to_string();
    let token_endpoint = body["token_endpoint"]
        .as_str()
        .ok_or("XAA: AS metadata missing token_endpoint")?
        .to_string();

    if normalize_url(&issuer) != normalize_url(as_url) {
        return Err(format!(
            "XAA: AS metadata issuer mismatch: expected {}, got {}",
            as_url, issuer
        )
        .into());
    }

    let te_url = url::Url::parse(&token_endpoint).map_err(|_| "XAA: invalid token_endpoint URL")?;
    if te_url.scheme() != "https" {
        return Err(format!("XAA: refusing non-HTTPS token endpoint: {}", token_endpoint).into());
    }

    let grant_types = body["grant_types_supported"].as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });
    let auth_methods = body["token_endpoint_auth_methods_supported"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    Ok(AuthorizationServerMetadata {
        issuer,
        token_endpoint,
        grant_types_supported: grant_types,
        token_endpoint_auth_methods_supported: auth_methods,
    })
}

// ─── Exchange ───────────────────────────────────────────────────────────

/// Result of RFC 8693 token exchange.
#[derive(Debug, Clone)]
pub struct JwtAuthGrantResult {
    pub jwt_auth_grant: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

/// RFC 8693 Token Exchange at the IdP: id_token -> ID-JAG.
pub async fn request_jwt_authorization_grant(
    token_endpoint: &str,
    audience: &str,
    resource: &str,
    id_token: &str,
    client_id: &str,
    client_secret: Option<&str>,
    scope: Option<&str>,
    client: &reqwest::Client,
) -> Result<JwtAuthGrantResult, XaaTokenExchangeError> {
    let mut params = vec![
        ("grant_type", TOKEN_EXCHANGE_GRANT),
        ("requested_token_type", ID_JAG_TOKEN_TYPE),
        ("audience", audience),
        ("resource", resource),
        ("subject_token", id_token),
        ("subject_token_type", ID_TOKEN_TYPE),
        ("client_id", client_id),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret));
    }
    if let Some(s) = scope {
        params.push(("scope", s));
    }

    let resp = client
        .post(token_endpoint)
        .form(&params)
        .timeout(Duration::from_millis(XAA_REQUEST_TIMEOUT_MS))
        .send()
        .await
        .map_err(|e| XaaTokenExchangeError {
            message: format!("XAA: token exchange request failed: {}", e),
            should_clear_id_token: false,
        })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let redacted = redact_tokens(&body);
        let should_clear = status < 500;
        return Err(XaaTokenExchangeError {
            message: format!(
                "XAA: token exchange failed: HTTP {}: {}",
                status,
                truncate_chars(&redacted, 200)
            ),
            should_clear_id_token: should_clear,
        });
    }

    let result: TokenExchangeResponse = resp.json().await.map_err(|_| XaaTokenExchangeError {
        message: format!(
            "XAA: token exchange returned non-JSON at {}",
            token_endpoint
        ),
        should_clear_id_token: false,
    })?;

    let access_token = result.access_token.ok_or_else(|| XaaTokenExchangeError {
        message: "XAA: token exchange response missing access_token".to_string(),
        should_clear_id_token: true,
    })?;

    if result.issued_token_type.as_deref() != Some(ID_JAG_TOKEN_TYPE) {
        return Err(XaaTokenExchangeError {
            message: format!(
                "XAA: token exchange returned unexpected issued_token_type: {:?}",
                result.issued_token_type
            ),
            should_clear_id_token: true,
        });
    }

    Ok(JwtAuthGrantResult {
        jwt_auth_grant: access_token,
        expires_in: result.expires_in,
        scope: result.scope,
    })
}

/// XAA token result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaaTokenResult {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub refresh_token: Option<String>,
}

/// Full XAA result including AS URL.
#[derive(Debug, Clone)]
pub struct XaaResult {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub refresh_token: Option<String>,
    pub authorization_server_url: String,
}

/// Authentication method for JWT bearer grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    ClientSecretBasic,
    ClientSecretPost,
}

/// RFC 7523 JWT Bearer Grant at the AS: ID-JAG -> access_token.
pub async fn exchange_jwt_auth_grant(
    token_endpoint: &str,
    assertion: &str,
    client_id: &str,
    client_secret: &str,
    auth_method: AuthMethod,
    scope: Option<&str>,
    client: &reqwest::Client,
) -> Result<XaaTokenResult, Box<dyn std::error::Error + Send + Sync>> {
    let mut params = vec![
        ("grant_type".to_string(), JWT_BEARER_GRANT.to_string()),
        ("assertion".to_string(), assertion.to_string()),
    ];
    if let Some(s) = scope {
        params.push(("scope".to_string(), s.to_string()));
    }

    let mut builder = client
        .post(token_endpoint)
        .timeout(Duration::from_millis(XAA_REQUEST_TIMEOUT_MS));

    match auth_method {
        AuthMethod::ClientSecretBasic => {
            use base64::Engine;
            let credentials = format!(
                "{}:{}",
                urlencoding::encode(client_id),
                urlencoding::encode(client_secret)
            );
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
            builder = builder.header("Authorization", format!("Basic {}", encoded));
        }
        AuthMethod::ClientSecretPost => {
            params.push(("client_id".to_string(), client_id.to_string()));
            params.push(("client_secret".to_string(), client_secret.to_string()));
        }
    }

    let resp = builder
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("XAA: jwt-bearer grant request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        let redacted = redact_tokens(&body);
        return Err(format!(
            "XAA: jwt-bearer grant failed: HTTP {}: {}",
            status,
            truncate_chars(&redacted, 200)
        )
        .into());
    }

    let result: JwtBearerResponse = resp
        .json()
        .await
        .map_err(|e| format!("XAA: jwt-bearer response parse failed: {}", e))?;

    Ok(XaaTokenResult {
        access_token: result.access_token,
        token_type: result.token_type,
        expires_in: result.expires_in,
        scope: result.scope,
        refresh_token: result.refresh_token,
    })
}

// ─── Orchestrator ───────────────────────────────────────────────────────

/// Config needed to run the full XAA orchestrator.
#[derive(Debug, Clone)]
pub struct XaaConfig {
    pub client_id: String,
    pub client_secret: String,
    pub idp_client_id: String,
    pub idp_client_secret: Option<String>,
    pub idp_id_token: String,
    pub idp_token_endpoint: String,
}

/// Full XAA flow: PRM -> AS metadata -> token-exchange -> jwt-bearer -> access_token.
pub async fn perform_cross_app_access(
    server_url: &str,
    config: &XaaConfig,
    server_name: &str,
) -> Result<XaaResult, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::new();

    tracing::debug!(server_name, "XAA: discovering PRM for {}", server_url);
    let prm = discover_protected_resource(server_url, &client).await?;
    tracing::debug!(
        server_name,
        "XAA: discovered resource={} ASes=[{}]",
        prm.resource,
        prm.authorization_servers.join(", ")
    );

    // Try each advertised AS in order
    let mut as_meta: Option<AuthorizationServerMetadata> = None;
    let mut as_errors: Vec<String> = Vec::new();
    for as_url in &prm.authorization_servers {
        match discover_authorization_server(as_url, &client).await {
            Ok(candidate) => {
                if let Some(ref grants) = candidate.grant_types_supported {
                    if !grants.iter().any(|g| g == JWT_BEARER_GRANT) {
                        as_errors.push(format!(
                            "{}: does not advertise jwt-bearer grant (supported: {})",
                            as_url,
                            grants.join(", ")
                        ));
                        continue;
                    }
                }
                as_meta = Some(candidate);
                break;
            }
            Err(e) => {
                as_errors.push(format!("{}: {}", as_url, e));
                continue;
            }
        }
    }

    let as_meta = as_meta.ok_or_else(|| {
        format!(
            "XAA: no authorization server supports jwt-bearer. Tried: {}",
            as_errors.join("; ")
        )
    })?;

    // Pick auth method
    let auth_method = match &as_meta.token_endpoint_auth_methods_supported {
        Some(methods)
            if !methods.contains(&"client_secret_basic".to_string())
                && methods.contains(&"client_secret_post".to_string()) =>
        {
            AuthMethod::ClientSecretPost
        }
        _ => AuthMethod::ClientSecretBasic,
    };

    tracing::debug!(
        server_name,
        "XAA: AS issuer={} token_endpoint={} auth_method={:?}",
        as_meta.issuer,
        as_meta.token_endpoint,
        auth_method
    );

    tracing::debug!(server_name, "XAA: exchanging id_token for ID-JAG at IdP");
    let jag = request_jwt_authorization_grant(
        &config.idp_token_endpoint,
        &as_meta.issuer,
        &prm.resource,
        &config.idp_id_token,
        &config.idp_client_id,
        config.idp_client_secret.as_deref(),
        None,
        &client,
    )
    .await
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
    tracing::debug!(server_name, "XAA: ID-JAG obtained");

    tracing::debug!(server_name, "XAA: exchanging ID-JAG for access_token at AS");
    let tokens = exchange_jwt_auth_grant(
        &as_meta.token_endpoint,
        &jag.jwt_auth_grant,
        &config.client_id,
        &config.client_secret,
        auth_method,
        None,
        &client,
    )
    .await?;
    tracing::debug!(server_name, "XAA: access_token obtained");

    Ok(XaaResult {
        access_token: tokens.access_token,
        token_type: tokens.token_type,
        expires_in: tokens.expires_in,
        scope: tokens.scope,
        refresh_token: tokens.refresh_token,
        authorization_server_url: as_meta.issuer,
    })
}
