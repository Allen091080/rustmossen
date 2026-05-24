//! Hosted MCP server configuration — fetch org-managed connectors.
//!
//! Translates `services/mcp/hosted.ts`.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::RwLock;

use crate::mcp::normalization::normalize_name_for_mcp;
use crate::mcp::types::{ConfigScope, ScopedMcpServerConfig};

#[derive(Debug, Deserialize)]
struct HostedMcpServer {
    id: String,
    display_name: String,
    url: String,
    #[allow(dead_code)]
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct HostedMcpServersResponse {
    data: Vec<HostedMcpServer>,
    #[allow(dead_code)]
    has_more: bool,
    #[allow(dead_code)]
    next_page: Option<String>,
}

const FETCH_TIMEOUT_MS: u64 = 5000;
const MCP_SERVERS_BETA_HEADER: &str = "mcp-servers-2025-12-04";

static HOSTED_CACHE: OnceLock<RwLock<Option<HashMap<String, ScopedMcpServerConfig>>>> =
    OnceLock::new();

fn cache() -> &'static RwLock<Option<HashMap<String, ScopedMcpServerConfig>>> {
    HOSTED_CACHE.get_or_init(|| RwLock::new(None))
}

/// Fetch MCP server configurations from hosted org configs.
/// Results are memoized for the session lifetime (fetch once per CLI session).
pub async fn fetch_hosted_mcp_configs_if_eligible(
    is_custom_backend: bool,
    disable_hosted_env: bool,
    access_token: Option<&str>,
    scopes: Option<&[String]>,
    base_api_url: &str,
) -> HashMap<String, ScopedMcpServerConfig> {
    // Check if already cached
    {
        let guard = cache().read().await;
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }

    let result = fetch_hosted_mcp_configs_impl(
        is_custom_backend,
        disable_hosted_env,
        access_token,
        scopes,
        base_api_url,
    )
    .await;

    // Store in cache
    let mut guard = cache().write().await;
    *guard = Some(result.clone());
    result
}

async fn fetch_hosted_mcp_configs_impl(
    is_custom_backend: bool,
    disable_hosted_env: bool,
    access_token: Option<&str>,
    scopes: Option<&[String]>,
    base_api_url: &str,
) -> HashMap<String, ScopedMcpServerConfig> {
    if is_custom_backend {
        tracing::debug!("[hosted-mcp] Disabled: custom backend mode");
        return HashMap::new();
    }

    if disable_hosted_env {
        tracing::debug!("[hosted-mcp] Disabled via env var");
        return HashMap::new();
    }

    let token = match access_token {
        Some(t) if !t.is_empty() => t,
        _ => {
            tracing::debug!("[hosted-mcp] No access token");
            return HashMap::new();
        }
    };

    // Check for user:mcp_servers scope
    let has_scope = scopes
        .map(|s| s.iter().any(|scope| scope == "user:mcp_servers"))
        .unwrap_or(false);

    if !has_scope {
        tracing::debug!("[hosted-mcp] Missing user:mcp_servers scope");
        return HashMap::new();
    }

    let url = format!("{}/v1/mcp_servers?limit=1000", base_api_url);
    tracing::debug!("[hosted-mcp] Fetching from {}", url);

    let client = reqwest::Client::new();
    let result = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .header("mossen-beta", MCP_SERVERS_BETA_HEADER)
        .header("mossen-version", "2023-06-01")
        .timeout(Duration::from_millis(FETCH_TIMEOUT_MS))
        .send()
        .await;

    match result {
        Ok(resp) => match resp.json::<HostedMcpServersResponse>().await {
            Ok(data) => {
                let mut configs: HashMap<String, ScopedMcpServerConfig> = HashMap::new();
                let mut used_normalized_names: HashSet<String> = HashSet::new();

                for server in &data.data {
                    let base_name = format!("hosted {}", server.display_name);

                    let mut final_name = base_name.clone();
                    let mut final_normalized = normalize_name_for_mcp(&final_name);
                    let mut count = 1u32;
                    while used_normalized_names.contains(&final_normalized) {
                        count += 1;
                        final_name = format!("{} ({})", base_name, count);
                        final_normalized = normalize_name_for_mcp(&final_name);
                    }
                    used_normalized_names.insert(final_normalized);

                    configs.insert(
                        final_name,
                        ScopedMcpServerConfig {
                            config: crate::mcp::types::McpServerConfig::HostedProxy {
                                url: server.url.clone(),
                                id: server.id.clone(),
                            },
                            scope: ConfigScope::Hosted,
                            plugin_source: None,
                        },
                    );
                }

                tracing::debug!("[hosted-mcp] Fetched {} servers", configs.len());
                configs
            }
            Err(_) => {
                tracing::debug!("[hosted-mcp] Fetch failed (parse error)");
                HashMap::new()
            }
        },
        Err(_) => {
            tracing::debug!("[hosted-mcp] Fetch failed");
            HashMap::new()
        }
    }
}

/// Clear the memoized cache for fetchHostedMcpConfigsIfEligible.
pub async fn clear_hosted_mcp_configs_cache() {
    let mut guard = cache().write().await;
    *guard = None;
}

/// Record that a hosted connector successfully connected.
pub fn mark_hosted_mcp_connected(
    name: &str,
    get_config: impl Fn() -> Vec<String>,
    save_config: impl Fn(Vec<String>),
) {
    let seen = get_config();
    if seen.contains(&name.to_string()) {
        return;
    }
    let mut updated = seen;
    updated.push(name.to_string());
    save_config(updated);
}

/// Check if a hosted connector has ever connected.
pub fn has_hosted_mcp_ever_connected(name: &str, ever_connected: &[String]) -> bool {
    ever_connected.contains(&name.to_string())
}
