//! Official MCP registry — prefetch and lookup for known MCP server URLs.
//!
//! Translates `services/mcp/officialRegistry.ts`.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::RwLock;

static OFFICIAL_URLS: OnceLock<RwLock<Option<HashSet<String>>>> = OnceLock::new();

fn urls_lock() -> &'static RwLock<Option<HashSet<String>>> {
    OFFICIAL_URLS.get_or_init(|| RwLock::new(None))
}

#[derive(serde::Deserialize)]
struct RegistryServer {
    server: RegistryServerInner,
}

#[derive(serde::Deserialize)]
struct RegistryServerInner {
    remotes: Option<Vec<RegistryRemote>>,
}

#[derive(serde::Deserialize)]
struct RegistryRemote {
    url: String,
}

#[derive(serde::Deserialize)]
struct RegistryResponse {
    servers: Vec<RegistryServer>,
}

/// Normalize URL: strip query string and trailing slash.
fn normalize_url(url: &str) -> Option<String> {
    match url::Url::parse(url) {
        Ok(mut u) => {
            u.set_query(None);
            let s = u.to_string();
            Some(s.trim_end_matches('/').to_string())
        }
        Err(_) => None,
    }
}

/// Fire-and-forget fetch of the official MCP registry.
/// Populates official URLs for `is_official_mcp_url` lookups.
pub async fn prefetch_official_mcp_urls(
    remote_base_url: &str,
    is_custom_backend: bool,
    disable_nonessential_traffic: bool,
) {
    if is_custom_backend {
        return;
    }
    if disable_nonessential_traffic {
        return;
    }

    let url = format!(
        "{}/mcp-registry/v0/servers?version=latest&visibility=commercial",
        remote_base_url
    );

    let client = reqwest::Client::new();
    let result = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await;

    match result {
        Ok(resp) => match resp.json::<RegistryResponse>().await {
            Ok(data) => {
                let mut urls = HashSet::new();
                for entry in &data.servers {
                    if let Some(remotes) = &entry.server.remotes {
                        for remote in remotes {
                            if let Some(normalized) = normalize_url(&remote.url) {
                                urls.insert(normalized);
                            }
                        }
                    }
                }
                tracing::debug!("[mcp-registry] Loaded {} official MCP URLs", urls.len());
                let mut guard = urls_lock().write().await;
                *guard = Some(urls);
            }
            Err(e) => {
                tracing::error!("Failed to parse MCP registry response: {}", e);
            }
        },
        Err(e) => {
            tracing::error!("Failed to fetch MCP registry: {}", e);
        }
    }
}

/// Returns true if the given (already-normalized) URL is in the official MCP registry.
/// Undefined registry returns false (fail-closed).
pub async fn is_official_mcp_url(normalized_url: &str) -> bool {
    let guard = urls_lock().read().await;
    match guard.as_ref() {
        Some(set) => set.contains(normalized_url),
        None => false,
    }
}

/// Reset official URLs for testing.
pub async fn reset_official_mcp_urls_for_testing() {
    let mut guard = urls_lock().write().await;
    *guard = None;
}
