//! Remote MCP config install plan — fetch JSON from URL and staged install.
//!
//! Translates `services/mcp/remoteInstallPlan.ts`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;

use crate::mcp::types::{ConfigScope, McpServerConfig};

pub const MCP_REMOTE_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct McpRemoteInstallPlan {
    pub token: String,
    pub created_at: u64,
    pub source: String,
    pub server_name: String,
    pub scope: ConfigScope,
    pub config: McpServerConfig,
    pub available_servers: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum McpRemotePlanError {
    MissingSource,
    InvalidScope { scope: Option<String> },
    InvalidSource { reason: String },
    MultipleServers { available_servers: Vec<String> },
    MissingServerName,
    ServerNotFound { server_name: String, available_servers: Vec<String> },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

pub enum McpRemoteInstallResult {
    Ok { plan: McpRemoteInstallPlan },
    Err { error: McpRemotePlanError },
}

lazy_static::lazy_static! {
    static ref PLAN_STORE: Mutex<HashMap<String, McpRemoteInstallPlan>> = Mutex::new(HashMap::new());
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn prune_expired_plans(store: &mut HashMap<String, McpRemoteInstallPlan>) {
    let now = now_ms();
    store.retain(|_, plan| now - plan.created_at <= MCP_REMOTE_PLAN_TOKEN_TTL_MS);
}

fn create_token(store: &HashMap<String, McpRemoteInstallPlan>) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let token = format!("{:08x}", rng.gen::<u32>());
        if !store.contains_key(&token) {
            return token;
        }
    }
}

fn normalize_scope(scope: Option<&str>) -> Option<ConfigScope> {
    match scope {
        None | Some("local") => Some(ConfigScope::Local),
        Some("user") => Some(ConfigScope::User),
        Some("project") => Some(ConfigScope::Project),
        _ => None,
    }
}

/// Convert GitHub blob URLs to raw content URLs.
fn to_fetchable_url(source: &str) -> String {
    match url::Url::parse(source) {
        Ok(u) => {
            let host = u.host_str().unwrap_or("");
            if host != "github.com" && host != "www.github.com" {
                return source.to_string();
            }
            let parts: Vec<&str> = u.path().split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 5 && parts[2] == "blob" {
                let owner = parts[0];
                let repo = parts[1];
                let ref_name = parts[3];
                let path_parts = &parts[4..];
                return format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}",
                    owner, repo, ref_name, path_parts.join("/")
                );
            }
            source.to_string()
        }
        Err(_) => source.to_string(),
    }
}

/// Load remote JSON config from URL.
async fn load_remote_json(source: &str) -> Result<serde_json::Value, String> {
    let fetchable = to_fetchable_url(source);
    let url = match url::Url::parse(&fetchable) {
        Ok(u) => u,
        Err(_) => {
            return Err(
                "Expected an http(s) URL or a GitHub blob URL to a JSON MCP config.".to_string(),
            );
        }
    };

    let scheme = url.scheme();
    if scheme != "https" && scheme != "http" {
        return Err("Only http(s) remote MCP config URLs are supported.".to_string());
    }

    let client = reqwest::Client::new();
    let response = client
        .get(url.as_str())
        .header("Accept", "application/json,text/plain;q=0.9,*/*;q=0.1")
        .header("User-Agent", "mossen-mcp-installer")
        .send()
        .await
        .map_err(|e| format!("Remote config request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Remote config request failed ({})",
            response.status().as_u16()
        ));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str(&text)
        .map_err(|e| format!("Remote config is not valid JSON: {}", e))
}

/// Select a server config from the parsed JSON.
fn select_server_config(
    json: &serde_json::Value,
    requested_name: Option<&str>,
) -> Result<(String, McpServerConfig, Vec<String>), McpRemotePlanError> {
    // Try parsing as McpJsonConfig { mcpServers: { ... } }
    if let Some(obj) = json.as_object() {
        if let Some(servers_val) = obj.get("mcpServers") {
            if let Some(servers) = servers_val.as_object() {
                let available: Vec<String> = servers.keys().cloned().collect();
                let server_name = match requested_name {
                    Some(n) => n.to_string(),
                    None => {
                        if available.len() == 1 {
                            available[0].clone()
                        } else {
                            return Err(McpRemotePlanError::MultipleServers {
                                available_servers: available,
                            });
                        }
                    }
                };

                let config_val = match servers.get(&server_name) {
                    Some(v) => v,
                    None => {
                        return Err(McpRemotePlanError::ServerNotFound {
                            server_name,
                            available_servers: available,
                        });
                    }
                };

                let config: McpServerConfig = serde_json::from_value(config_val.clone())
                    .map_err(|e| McpRemotePlanError::InvalidSource {
                        reason: e.to_string(),
                    })?;

                return Ok((server_name, config, available));
            }
        }
    }

    // Try parsing as a bare McpServerConfig
    let name = match requested_name {
        Some(n) => n.to_string(),
        None => return Err(McpRemotePlanError::MissingServerName),
    };

    let config: McpServerConfig =
        serde_json::from_value(json.clone()).map_err(|e| McpRemotePlanError::InvalidSource {
            reason: e.to_string(),
        })?;

    Ok((name.clone(), config, vec![name]))
}

/// Create a remote install plan.
pub async fn get_mcp_remote_install_plan(
    source: Option<&str>,
    server_name: Option<&str>,
    scope: Option<&str>,
) -> McpRemoteInstallResult {
    {
        let mut store = PLAN_STORE.lock().unwrap();
        prune_expired_plans(&mut store);
    }

    let source = match source.map(|s| s.trim()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return McpRemoteInstallResult::Err {
                error: McpRemotePlanError::MissingSource,
            };
        }
    };

    let resolved_scope = match normalize_scope(scope) {
        Some(s) => s,
        None => {
            return McpRemoteInstallResult::Err {
                error: McpRemotePlanError::InvalidScope {
                    scope: scope.map(|s| s.to_string()),
                },
            };
        }
    };

    let json = match load_remote_json(&source).await {
        Ok(j) => j,
        Err(reason) => {
            return McpRemoteInstallResult::Err {
                error: McpRemotePlanError::InvalidSource { reason },
            };
        }
    };

    let (name, config, available) = match select_server_config(&json, server_name) {
        Ok(v) => v,
        Err(e) => return McpRemoteInstallResult::Err { error: e },
    };

    let mut store = PLAN_STORE.lock().unwrap();
    let token = create_token(&store);
    let plan = McpRemoteInstallPlan {
        token: token.clone(),
        created_at: now_ms(),
        source,
        server_name: name,
        scope: resolved_scope,
        config,
        available_servers: available,
    };
    store.insert(token, plan.clone());
    McpRemoteInstallResult::Ok { plan }
}

/// Execute a previously-created remote install plan.
pub async fn execute_mcp_remote_install_plan(
    token: &str,
    add_mcp_config: &dyn super::builtin_template_plan::AsyncAddMcpConfig,
) -> McpRemoteInstallResult {
    let plan = {
        let mut store = PLAN_STORE.lock().unwrap();
        prune_expired_plans(&mut store);
        store.remove(token)
    };

    let plan = match plan {
        Some(p) => p,
        None => {
            return McpRemoteInstallResult::Err {
                error: McpRemotePlanError::UnknownToken { token: token.to_string() },
            };
        }
    };

    if now_ms() - plan.created_at > MCP_REMOTE_PLAN_TOKEN_TTL_MS {
        return McpRemoteInstallResult::Err {
            error: McpRemotePlanError::ExpiredToken { token: token.to_string() },
        };
    }

    match add_mcp_config.add_config(&plan.server_name, &plan.config, plan.scope).await {
        Ok(()) => McpRemoteInstallResult::Ok { plan },
        Err(e) => McpRemoteInstallResult::Err {
            error: McpRemotePlanError::InstallFailed { message: e.to_string() },
        },
    }
}

/// Reset plan store for testing.
pub fn reset_mcp_remote_plan_store_for_testing() {
    PLAN_STORE.lock().unwrap().clear();
}

/// TS `type McpRemoteWritableScope` — the scopes an MCP remote-install plan
/// is permitted to write into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpRemoteWritableScope {
    Project,
    User,
}
