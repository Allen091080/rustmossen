//! /mcp add slash command plan — staged install with confirmation token.
//!
//! Translates `services/mcp/slashAddPlan.ts`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;

use crate::mcp::types::{ConfigScope, McpServerConfig};

pub const MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;

pub type McpSlashAddWritableScope = ConfigScope; // local | user | project
pub type McpSlashAddTransport = &'static str; // "stdio" | "sse" | "http"

#[derive(Debug, Clone)]
pub struct McpSlashAddPlan {
    pub token: String,
    pub created_at: u64,
    pub server_name: String,
    pub scope: ConfigScope,
    pub transport: String,
    pub config: McpServerConfig,
}

#[derive(Debug, Clone)]
pub enum McpSlashAddPlanError {
    MissingServerName,
    MissingCommand,
    InvalidScope { scope: Option<String> },
    InvalidTransport { transport: Option<String> },
    InvalidEnv { message: String },
    InvalidHeader { message: String },
    InvalidConfig { reason: String },
    UnknownToken { token: String },
    ExpiredToken { token: String },
    InstallFailed { message: String },
}

pub enum McpSlashAddPlanResult {
    Ok { plan: McpSlashAddPlan },
    Err { error: McpSlashAddPlanError },
}

lazy_static::lazy_static! {
    static ref PLAN_STORE: Mutex<HashMap<String, McpSlashAddPlan>> = Mutex::new(HashMap::new());
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn prune_expired_plans(store: &mut HashMap<String, McpSlashAddPlan>) {
    let now = now_ms();
    store.retain(|_, plan| now - plan.created_at <= MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS);
}

fn create_token(store: &HashMap<String, McpSlashAddPlan>) -> String {
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

fn normalize_transport(transport: Option<&str>) -> Option<&'static str> {
    match transport {
        None | Some("stdio") => Some("stdio"),
        Some("sse") => Some("sse"),
        Some("http") => Some("http"),
        _ => None,
    }
}

/// Parse environment variables from "KEY=VALUE" format.
pub fn parse_env_vars(env: Option<&[String]>) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    if let Some(vars) = env {
        for var in vars {
            let eq_idx = var
                .find('=')
                .ok_or_else(|| format!("Invalid env format: \"{}\". Expected KEY=VALUE", var))?;
            let key = var[..eq_idx].trim().to_string();
            let value = var[eq_idx + 1..].trim().to_string();
            if key.is_empty() {
                return Err(format!("Invalid env: \"{}\". Key cannot be empty.", var));
            }
            map.insert(key, value);
        }
    }
    Ok(map)
}

/// Parse headers from "Key: Value" format.
pub fn parse_headers(headers: &[String]) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for header in headers {
        let colon_idx = header.find(':').ok_or_else(|| {
            format!(
                "Invalid header format: \"{}\". Expected format: \"Header-Name: value\"",
                header
            )
        })?;
        let key = header[..colon_idx].trim().to_string();
        let value = header[colon_idx + 1..].trim().to_string();
        if key.is_empty() {
            return Err(format!(
                "Invalid header: \"{}\". Header name cannot be empty.",
                header
            ));
        }
        map.insert(key, value);
    }
    Ok(map)
}

/// Create a /mcp add install plan.
pub fn get_mcp_slash_add_plan(
    server_name: Option<&str>,
    scope: Option<&str>,
    transport: Option<&str>,
    command_or_url: Option<&str>,
    args: Option<&[String]>,
    env: Option<&[String]>,
    headers: Option<&[String]>,
) -> McpSlashAddPlanResult {
    let mut store = PLAN_STORE.lock().unwrap();
    prune_expired_plans(&mut store);

    let server_name = match server_name.map(|s| s.trim()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => {
            return McpSlashAddPlanResult::Err {
                error: McpSlashAddPlanError::MissingServerName,
            };
        }
    };

    let resolved_scope = match normalize_scope(scope) {
        Some(s) => s,
        None => {
            return McpSlashAddPlanResult::Err {
                error: McpSlashAddPlanError::InvalidScope {
                    scope: scope.map(|s| s.to_string()),
                },
            };
        }
    };

    let resolved_transport = match normalize_transport(transport) {
        Some(t) => t,
        None => {
            return McpSlashAddPlanResult::Err {
                error: McpSlashAddPlanError::InvalidTransport {
                    transport: transport.map(|s| s.to_string()),
                },
            };
        }
    };

    let command_or_url = match command_or_url.map(|s| s.trim()) {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => {
            return McpSlashAddPlanResult::Err {
                error: McpSlashAddPlanError::MissingCommand,
            };
        }
    };

    let config = if resolved_transport == "stdio" {
        let parsed_env = match parse_env_vars(env) {
            Ok(e) => e,
            Err(msg) => {
                return McpSlashAddPlanResult::Err {
                    error: McpSlashAddPlanError::InvalidEnv { message: msg },
                };
            }
        };
        McpServerConfig::Stdio {
            command: command_or_url,
            args: args.map(|a| a.to_vec()).unwrap_or_default(),
            env: if parsed_env.is_empty() {
                None
            } else {
                Some(parsed_env)
            },
            cwd: None,
        }
    } else {
        let parsed_headers = if let Some(h) = headers {
            if h.is_empty() {
                None
            } else {
                match parse_headers(h) {
                    Ok(h) => Some(h),
                    Err(msg) => {
                        return McpSlashAddPlanResult::Err {
                            error: McpSlashAddPlanError::InvalidHeader { message: msg },
                        };
                    }
                }
            }
        } else {
            None
        };

        if resolved_transport == "sse" {
            McpServerConfig::Sse {
                url: command_or_url,
                headers: parsed_headers,
                headers_helper: None,
                oauth: None,
            }
        } else {
            McpServerConfig::Http {
                url: command_or_url,
                headers: parsed_headers,
                headers_helper: None,
                oauth: None,
            }
        }
    };

    let token = create_token(&store);
    let plan = McpSlashAddPlan {
        token: token.clone(),
        created_at: now_ms(),
        server_name,
        scope: resolved_scope,
        transport: resolved_transport.to_string(),
        config,
    };
    store.insert(token, plan.clone());
    McpSlashAddPlanResult::Ok { plan }
}

/// Execute a previously-created slash add plan.
pub async fn execute_mcp_slash_add_plan(
    token: &str,
    add_mcp_config: &dyn super::builtin_template_plan::AsyncAddMcpConfig,
) -> McpSlashAddPlanResult {
    let plan = {
        let mut store = PLAN_STORE.lock().unwrap();
        prune_expired_plans(&mut store);
        store.remove(token)
    };

    let plan = match plan {
        Some(p) => p,
        None => {
            return McpSlashAddPlanResult::Err {
                error: McpSlashAddPlanError::UnknownToken {
                    token: token.to_string(),
                },
            };
        }
    };

    if now_ms() - plan.created_at > MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS {
        return McpSlashAddPlanResult::Err {
            error: McpSlashAddPlanError::ExpiredToken {
                token: token.to_string(),
            },
        };
    }

    match add_mcp_config
        .add_config(&plan.server_name, &plan.config, plan.scope)
        .await
    {
        Ok(()) => McpSlashAddPlanResult::Ok { plan },
        Err(e) => McpSlashAddPlanResult::Err {
            error: McpSlashAddPlanError::InstallFailed {
                message: e.to_string(),
            },
        },
    }
}

/// Reset plan store for testing.
pub fn reset_mcp_slash_add_plan_store_for_testing() {
    PLAN_STORE.lock().unwrap().clear();
}
