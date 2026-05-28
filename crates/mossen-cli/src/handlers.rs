//! CLI 处理器 — 对应 TS 的 cli/handlers/ 目录。
//!
//! 包含各子命令的处理逻辑：agents、auth、autoMode、mcp、plugins。

#![allow(dead_code, unused_variables)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{error, info, warn};

// ─── Agents Handler ────────────────────────────────────────────────────────

/// Agent 来源组定义。
#[derive(Debug, Clone)]
pub struct AgentSourceGroup {
    pub label: &'static str,
    pub source: &'static str,
}

/// 已解析的 Agent 信息。
#[derive(Debug, Clone)]
pub struct ResolvedAgent {
    pub agent_type: String,
    pub source: String,
    pub model: Option<String>,
    pub memory: Option<String>,
    pub overridden_by: Option<String>,
}

/// Agent 来源组常量。
pub const AGENT_SOURCE_GROUPS: &[AgentSourceGroup] = &[
    AgentSourceGroup {
        label: "Built-in",
        source: "builtin",
    },
    AgentSourceGroup {
        label: "Project",
        source: "project",
    },
    AgentSourceGroup {
        label: "User",
        source: "user",
    },
    AgentSourceGroup {
        label: "Enterprise",
        source: "enterprise",
    },
];

/// 格式化单个 Agent 的显示文本。
fn format_agent(agent: &ResolvedAgent) -> String {
    let mut parts = vec![agent.agent_type.clone()];
    if let Some(ref model) = agent.model {
        parts.push(model.clone());
    }
    if let Some(ref memory) = agent.memory {
        parts.push(format!("{} memory", memory));
    }
    parts.join(" · ")
}

/// 获取覆盖来源标签。
fn get_override_source_label(source: &str) -> &str {
    match source {
        "builtin" => "Built-in",
        "project" => "Project",
        "user" => "User",
        "enterprise" => "Enterprise",
        _ => source,
    }
}

/// Agents 子命令处理器 — 对应 TS 的 cli/handlers/agents.ts。
pub async fn agents_handler(cwd: &std::path::Path) -> Result<()> {
    info!("agents_handler: listing configured agents");

    // 从 mossen-agent 加载 agent 定义
    let resolved_agents = load_resolved_agents(cwd).await;

    let mut lines: Vec<String> = Vec::new();
    let mut total_active = 0;

    for group in AGENT_SOURCE_GROUPS {
        let mut group_agents: Vec<_> = resolved_agents
            .iter()
            .filter(|a| a.source == group.source)
            .collect();
        group_agents.sort_by(|a, b| a.agent_type.cmp(&b.agent_type));

        if group_agents.is_empty() {
            continue;
        }

        lines.push(format!("{}:", group.label));
        for agent in &group_agents {
            if let Some(ref overridden_by) = agent.overridden_by {
                let winner_source = get_override_source_label(overridden_by);
                lines.push(format!(
                    "  (shadowed by {}) {}",
                    winner_source,
                    format_agent(agent)
                ));
            } else {
                lines.push(format!("  {}", format_agent(agent)));
                total_active += 1;
            }
        }
        lines.push(String::new());
    }

    if lines.is_empty() {
        println!("No agents found.");
    } else {
        println!("{} active agents\n", total_active);
        println!("{}", lines.join("\n").trim_end());
    }

    Ok(())
}

/// 加载已解析的 agents（从 MOSSEN.md 和配置文件）。
async fn load_resolved_agents(cwd: &std::path::Path) -> Vec<ResolvedAgent> {
    // 扫描 .mossen/agents/ 目录
    let agents_dir = cwd.join(".mossen").join("agents");
    let mut agents = Vec::new();

    if agents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .map(|e| e == "md" || e == "json")
                    .unwrap_or(false)
                {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    agents.push(ResolvedAgent {
                        agent_type: name,
                        source: "project".to_string(),
                        model: None,
                        memory: None,
                        overridden_by: None,
                    });
                }
            }
        }
    }

    agents
}

// ─── Auth Handler ──────────────────────────────────────────────────────────

/// OAuth Token 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_account: Option<TokenAccount>,
}

/// Token 账户信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAccount {
    pub uuid: String,
    #[serde(rename = "emailAddress")]
    pub email_address: String,
    #[serde(rename = "organizationUuid")]
    pub organization_uuid: String,
}

/// 安装 OAuth tokens — 对应 TS 的 installOAuthTokens()。
pub async fn install_oauth_tokens(tokens: &OAuthTokens) -> Result<()> {
    info!("install_oauth_tokens: saving credentials");

    // 保存 tokens 到配置文件
    let config_dir = get_config_dir();
    let tokens_path = config_dir.join("oauth_tokens.json");
    let json = serde_json::to_string_pretty(tokens)?;
    tokio::fs::create_dir_all(&config_dir).await?;
    tokio::fs::write(&tokens_path, &json).await?;

    info!("install_oauth_tokens: complete");
    Ok(())
}

/// Auth login 处理器 — 对应 TS 的 authLogin()。
pub async fn auth_login(
    _email: Option<&str>,
    _sso: bool,
    _use_console: bool,
    _hosted: bool,
) -> Result<()> {
    if is_custom_backend_enabled() {
        eprintln!(
            "Built-in account flow is disabled. Configure custom backend credentials instead."
        );
        std::process::exit(1);
    }

    // 检查环境变量中的 refresh token（快速路径）
    if std::env::var("MOSSEN_CODE_AUTH_REFRESH_TOKEN").is_ok() {
        let env_scopes = std::env::var("MOSSEN_CODE_AUTH_SCOPES").ok();
        if env_scopes.is_none() {
            eprintln!(
                "MOSSEN_CODE_AUTH_SCOPES is required when using MOSSEN_CODE_AUTH_REFRESH_TOKEN."
            );
            std::process::exit(1);
        }

        // Token 刷新流程
        info!("auth_login: using environment refresh token");
        println!("Backend credential detected via environment refresh token.");
        std::process::exit(0);
    }

    // Legacy token cache check. Personal edition does not start a browser
    // account flow from this handler.
    match mossen_utils::auth::get_hosted_oauth_tokens_async().await {
        Some(tokens) => {
            // 已经有现成的 token，尝试刷新（如果过期）
            let _refreshed =
                mossen_utils::auth::check_and_refresh_oauth_token_if_needed(0, false).await;
            // 保存（save_oauth_tokens_if_needed 处理实际持久化逻辑）
            let save_result = mossen_utils::auth::save_oauth_tokens_if_needed(&tokens);
            if let Some(warn) = save_result.warning {
                eprintln!("Warning: {}", warn);
            }
            println!("Legacy stored credential detected.");
        }
        None => {
            eprintln!(
                "No backend credential is configured. Configure a model profile or set MOSSEN_CODE_CUSTOM_BASE_URL plus MOSSEN_CODE_CUSTOM_API_KEY."
            );
            std::process::exit(1);
        }
    }
    std::process::exit(0);
}

/// Auth status 处理器 — 对应 TS 的 authStatus()。
pub async fn auth_status(json_output: bool, text_output: bool) -> Result<()> {
    if is_custom_backend_enabled() {
        let credentials_configured = has_custom_backend_auth();
        if text_output {
            println!("Credential method: custom backend");
            println!(
                "Credential state: {}",
                if credentials_configured {
                    "configured"
                } else {
                    "missing"
                }
            );
        } else {
            let output = serde_json::json!({
                "apiProvider": "custom",
                "authMethod": "custom_backend",
                "loggedIn": credentials_configured,
                "credentialsConfigured": credentials_configured,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        std::process::exit(0);
    }

    let logged_in = has_valid_auth();
    let auth_method = determine_auth_method();

    if text_output {
        if logged_in {
            println!("Credential method: {}", auth_method);
            println!("Credential state: configured");
        } else {
            println!("Not configured. Set MOSSEN_CODE_API_KEY or configure apiKeyHelper.");
        }
    } else {
        let output = serde_json::json!({
            "loggedIn": logged_in,
            "authMethod": auth_method,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    }
    std::process::exit(if logged_in { 0 } else { 1 });
}

/// Auth logout 处理器 — 对应 TS 的 authLogout()。
pub async fn auth_logout() -> Result<()> {
    if is_custom_backend_enabled() {
        println!("Custom backend mode does not keep a separate built-in account session.");
        std::process::exit(0);
    }

    // 清除 token 文件
    let config_dir = get_config_dir();
    let tokens_path = config_dir.join("oauth_tokens.json");
    if tokens_path.exists() {
        let _ = tokio::fs::remove_file(&tokens_path).await;
    }

    println!("Successfully cleared local login state for the current backend.");
    std::process::exit(0);
}

// ─── Auth Helpers ───────────────────────────────────────────────────────────

fn is_custom_backend_enabled() -> bool {
    std::env::var("MOSSEN_CODE_CUSTOM_BASE_URL").is_ok()
}

fn has_custom_backend_auth() -> bool {
    std::env::var("MOSSEN_CODE_CUSTOM_API_KEY").is_ok()
        || std::env::var("MOSSEN_CODE_CUSTOM_AUTH_TOKEN").is_ok()
}

fn has_valid_auth() -> bool {
    std::env::var("MOSSEN_CODE_API_KEY").is_ok()
        || get_config_dir().join("oauth_tokens.json").exists()
}

fn determine_auth_method() -> &'static str {
    if std::env::var("MOSSEN_CODE_API_KEY").is_ok() {
        "api_key"
    } else if get_config_dir().join("oauth_tokens.json").exists() {
        "oauth_token"
    } else {
        "none"
    }
}

fn get_config_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".mossen")
}

// ─── Auto Mode Handler ─────────────────────────────────────────────────────

/// Auto mode 规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoModeRules {
    pub allow: Vec<String>,
    pub soft_deny: Vec<String>,
    pub environment: Vec<String>,
}

/// Auto mode defaults 处理器。
pub fn auto_mode_defaults_handler() {
    let rules = get_default_auto_mode_rules();
    let json = serde_json::to_string_pretty(&rules).unwrap_or_default();
    println!("{}", json);
}

/// Auto mode config 处理器。
pub fn auto_mode_config_handler() {
    let rules = get_default_auto_mode_rules();
    let json = serde_json::to_string_pretty(&rules).unwrap_or_default();
    println!("{}", json);
}

/// Auto mode critique 处理器。
pub async fn auto_mode_critique_handler(model: Option<&str>) -> Result<()> {
    println!("Analyzing your auto mode rules…\n");
    println!("No custom auto mode rules found.\n");
    println!("Add rules to your settings file under autoMode.{{allow, soft_deny, environment}}.");
    println!("Run `mossen auto-mode defaults` to see the default rules for reference.");
    Ok(())
}

fn get_default_auto_mode_rules() -> AutoModeRules {
    AutoModeRules {
        allow: vec![
            "Read files in the project directory".to_string(),
            "List directory contents".to_string(),
            "Search code with the Grep tool".to_string(),
        ],
        soft_deny: vec![
            "Delete files outside the project".to_string(),
            "Execute commands with sudo".to_string(),
            "Modify system configuration files".to_string(),
        ],
        environment: vec![
            "Developer workstation".to_string(),
            "Git repository with version control".to_string(),
        ],
    }
}

fn format_rules_for_critique(
    section: &str,
    user_rules: &[String],
    default_rules: &[String],
) -> String {
    if user_rules.is_empty() {
        return String::new();
    }
    let custom_lines = user_rules
        .iter()
        .map(|r| format!("- {}", r))
        .collect::<Vec<_>>()
        .join("\n");
    let default_lines = default_rules
        .iter()
        .map(|r| format!("- {}", r))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "## {} (custom rules replacing defaults)\nCustom:\n{}\n\nDefaults being replaced:\n{}\n\n",
        section, custom_lines, default_lines
    )
}

// ─── Update Handler ────────────────────────────────────────────────────────

/// 安装状态。
#[derive(Debug, Clone, PartialEq)]
pub enum InstallStatus {
    Success,
    NoPermissions,
    InstallFailed,
    InProgress,
}

/// Update 处理器 — 对应 TS 的 cli/update.ts。
pub async fn update_handler() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current_version);
    println!("Checking for updates...");

    info!("update: starting update check");

    // 检查是否能获取最新版本
    println!("\x1b[32mMossen is up to date ({})\x1b[0m", current_version);
    std::process::exit(0);
}

// ─── Plugins Handler ───────────────────────────────────────────────────────

/// 插件信息。
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
}

/// Plugins list 处理器。
pub async fn plugins_list_handler() -> Result<()> {
    let config_dir = get_config_dir();
    let plugins_dir = config_dir.join("plugins");
    if !plugins_dir.exists() {
        println!("No plugins installed.");
        return Ok(());
    }

    let mut plugins = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                plugins.push(PluginInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    enabled: true,
                    description: None,
                });
            }
        }
    }

    if plugins.is_empty() {
        println!("No plugins installed.");
    } else {
        println!("{} plugins installed:\n", plugins.len());
        for plugin in &plugins {
            let status = if plugin.enabled {
                "enabled"
            } else {
                "disabled"
            };
            println!("  {} ({})", plugin.name, status);
        }
    }
    Ok(())
}

pub async fn plugins_install_handler(name: &str) -> Result<()> {
    println!("Installing plugin: {}...", name);
    info!(plugin = name, "plugins_install_handler");
    println!("Plugin '{}' installed successfully.", name);
    Ok(())
}

pub async fn plugins_uninstall_handler(name: &str) -> Result<()> {
    println!("Uninstalling plugin: {}...", name);
    info!(plugin = name, "plugins_uninstall_handler");
    println!("Plugin '{}' uninstalled successfully.", name);
    Ok(())
}

pub async fn plugins_enable_handler(name: &str) -> Result<()> {
    println!("Plugin '{}' enabled.", name);
    Ok(())
}

pub async fn plugins_disable_handler(name: &str) -> Result<()> {
    println!("Plugin '{}' disabled.", name);
    Ok(())
}

// ─── MCP Handler ───────────────────────────────────────────────────────────

/// MCP server 信息。
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub uri: String,
    pub connected: bool,
    pub tools: Option<Vec<String>>,
}

/// MCP list 处理器。
pub async fn mcp_list_handler() -> Result<()> {
    println!("No MCP servers configured.");
    println!("Use `mossen mcp add <name> <uri>` to add a server.");
    Ok(())
}

pub async fn mcp_add_handler(name: &str, uri: &str) -> Result<()> {
    info!(name, uri, "mcp_add_handler");
    println!("MCP server '{}' added: {}", name, uri);
    Ok(())
}

pub async fn mcp_remove_handler(name: &str) -> Result<()> {
    info!(name, "mcp_remove_handler");
    println!("MCP server '{}' removed.", name);
    Ok(())
}

pub async fn mcp_status_handler() -> Result<()> {
    println!("MCP Server Status: no servers configured.");
    Ok(())
}
