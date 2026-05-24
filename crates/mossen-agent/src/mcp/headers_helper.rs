//! Dynamic headers helper for MCP servers.
//!
//! Translates `services/mcp/headersHelper.ts`.

use std::collections::HashMap;
use std::time::Duration;

use crate::mcp::types::{
    ConfigScope, McpHttpServerConfig, McpSseServerConfig, McpWebSocketServerConfig,
    ScopedMcpServerConfig,
};

/// Check if the MCP server config comes from project settings (projectSettings or localSettings).
fn is_mcp_server_from_project_or_local_settings(config: &ScopedMcpServerConfig) -> bool {
    config.scope == ConfigScope::Project || config.scope == ConfigScope::Local
}

/// Trait to unify SSE/HTTP/WS configs that have headers_helper and url.
pub trait HasHeadersHelper {
    fn headers_helper(&self) -> Option<&str>;
    fn url(&self) -> &str;
    fn headers(&self) -> Option<&HashMap<String, String>>;
    fn scope(&self) -> Option<ConfigScope>;
}

/// Configuration for getMcpHeadersFromHelper
pub struct HeadersHelperConfig {
    pub headers_helper: Option<String>,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub scope: Option<ConfigScope>,
}

/// Get dynamic headers for an MCP server using the headersHelper script.
///
/// Returns headers object or None if not configured or failed.
pub async fn get_mcp_headers_from_helper(
    server_name: &str,
    config: &HeadersHelperConfig,
    is_non_interactive: bool,
    check_trust_accepted: impl Fn() -> bool,
) -> Option<HashMap<String, String>> {
    let helper = config.headers_helper.as_deref()?;

    // Security check for project/local settings
    if let Some(scope) = config.scope {
        if (scope == ConfigScope::Project || scope == ConfigScope::Local) && !is_non_interactive {
            let has_trust = check_trust_accepted();
            if !has_trust {
                tracing::error!(
                    server_name,
                    "Security: headersHelper for MCP server executed before workspace trust is confirmed"
                );
                return None;
            }
        }
    }

    tracing::debug!(
        server_name,
        "Executing headersHelper to get dynamic headers"
    );

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(helper);
    cmd.env("MOSSEN_CODE_MCP_SERVER_NAME", server_name);
    cmd.env("MOSSEN_CODE_MCP_SERVER_URL", &config.url);

    let result = tokio::time::timeout(Duration::from_secs(10), cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            if !output.status.success() || output.stdout.is_empty() {
                tracing::error!(server_name, "headersHelper did not return a valid value");
                return None;
            }

            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

            let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(
                        server_name,
                        error = %e,
                        "headersHelper returned invalid JSON"
                    );
                    return None;
                }
            };

            let obj = match parsed.as_object() {
                Some(o) => o,
                None => {
                    tracing::error!(
                        server_name,
                        "headersHelper must return a JSON object with string key-value pairs"
                    );
                    return None;
                }
            };

            let mut headers = HashMap::new();
            for (key, value) in obj {
                match value.as_str() {
                    Some(s) => {
                        headers.insert(key.clone(), s.to_string());
                    }
                    None => {
                        tracing::error!(
                            server_name,
                            key,
                            "headersHelper returned non-string value for key"
                        );
                        return None;
                    }
                }
            }

            tracing::debug!(
                server_name,
                count = headers.len(),
                "Successfully retrieved headers from headersHelper"
            );
            Some(headers)
        }
        Ok(Err(e)) => {
            tracing::error!(
                server_name,
                error = %e,
                "Error executing headersHelper"
            );
            None
        }
        Err(_) => {
            tracing::error!(server_name, "headersHelper timed out");
            None
        }
    }
}

/// Get combined headers for an MCP server (static + dynamic).
///
/// Dynamic headers override static headers if both are present.
pub async fn get_mcp_server_headers(
    server_name: &str,
    config: &HeadersHelperConfig,
    is_non_interactive: bool,
    check_trust_accepted: impl Fn() -> bool,
) -> HashMap<String, String> {
    let static_headers = config.headers.clone().unwrap_or_default();
    let dynamic_headers = get_mcp_headers_from_helper(
        server_name,
        config,
        is_non_interactive,
        check_trust_accepted,
    )
    .await
    .unwrap_or_default();

    let mut combined = static_headers;
    combined.extend(dynamic_headers);
    combined
}
