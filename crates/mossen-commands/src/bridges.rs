//! `/mcp` — Manage Model Context Protocol server connections.
//!
//! Translated from commands/mcp/ (12 TS/TSX files, ~1034 lines).
//! Provides the interactive `/mcp` slash command with subcommands:
//!   status, templates, add, add-template, install, enable, disable,
//!   reconnect, no-redirect, help.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};
use mossen_agent::mcp::builtin_template_plan::{
    AsyncAddMcpConfig, McpTemplateInstallResult, McpTemplatePlanError,
};
use mossen_agent::mcp::config::McpConfigWriter;
use mossen_agent::mcp::remote_install_plan::{McpRemoteInstallResult, McpRemotePlanError};
use mossen_agent::mcp::runtime_status::{self, RuntimeMcpConnectionState};
use mossen_agent::mcp::slash_add_plan::{McpSlashAddPlanError, McpSlashAddPlanResult};
use mossen_agent::mcp::types::{ConfigScope, McpServerConfig};

// ── Types ─────────────────────────────────────────────────────────────

/// Parsed arguments for `/mcp add`.
#[derive(Debug, Default)]
struct ParsedMcpAddArgs {
    server_name: Option<String>,
    scope: Option<String>,
    transport: Option<String>,
    command_or_url: Option<String>,
    args: Vec<String>,
    env: Vec<String>,
    headers: Vec<String>,
    confirm_token: Option<String>,
    unsupported_flag: Option<String>,
}

/// Parsed arguments for `/mcp install`.
#[derive(Debug, Default)]
struct ParsedMcpInstallArgs {
    source: Option<String>,
    server_name: Option<String>,
    scope: Option<String>,
    confirm_token: Option<String>,
    unsupported_flag: Option<String>,
}

/// Parsed arguments for `/mcp add-template`.
#[derive(Debug, Default)]
struct ParsedMcpAddTemplateArgs {
    template_name: Option<String>,
    server_name: Option<String>,
    scope: Option<String>,
    root: Option<String>,
    db: Option<String>,
    confirm_token: Option<String>,
    unsupported_flag: Option<String>,
}

/// MCP client connection state.
#[derive(Debug, Clone)]
struct McpClientInfo {
    name: String,
    status: String,
    scope: String,
    transport: String,
    tool_count: usize,
    prompt_count: usize,
    resource_count: usize,
    error: Option<String>,
    reconnect_attempt: Option<u32>,
    max_reconnect_attempts: Option<u32>,
}

#[derive(Debug, Clone)]
struct FileMcpConfigWriter {
    cwd: PathBuf,
}

impl FileMcpConfigWriter {
    fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    fn config_path(&self, scope: ConfigScope) -> std::result::Result<PathBuf, String> {
        match scope {
            ConfigScope::User => {
                Ok(mossen_utils::env::get_mossen_config_home_dir().join("mcp.json"))
            }
            ConfigScope::Local | ConfigScope::Project => Ok(self.cwd.join(".mcp.json")),
            ConfigScope::Dynamic
            | ConfigScope::Enterprise
            | ConfigScope::Hosted
            | ConfigScope::Managed => Err(format!("Cannot write MCP config to scope: {:?}", scope)),
        }
    }
}

async fn read_mcp_json(path: &Path) -> std::result::Result<Value, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => serde_json::from_str::<Value>(&text)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_json::json!({ "mcpServers": {} }))
        }
        Err(error) => Err(format!("Failed to read {}: {error}", path.display())),
    }
}

async fn write_mcp_json(path: &Path, value: &Value) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Failed to serialize MCP config: {error}"))?;
    tokio::fs::write(path, format!("{text}\n"))
        .await
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn mcp_servers_object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    let object = value.as_object_mut().expect("value converted to object");
    object
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if object
        .get("mcpServers")
        .and_then(Value::as_object)
        .is_none()
    {
        object.insert("mcpServers".to_string(), serde_json::json!({}));
    }
    object
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .expect("mcpServers converted to object")
}

#[async_trait]
impl McpConfigWriter for FileMcpConfigWriter {
    async fn write(
        &self,
        name: &str,
        config: &McpServerConfig,
        scope: ConfigScope,
    ) -> std::result::Result<(), String> {
        let path = self.config_path(scope)?;
        let mut value = read_mcp_json(&path).await?;
        let server_value = serde_json::to_value(config)
            .map_err(|error| format!("Failed to serialize MCP server config: {error}"))?;
        mcp_servers_object_mut(&mut value).insert(name.to_string(), server_value);
        write_mcp_json(&path, &value).await
    }

    async fn remove(&self, name: &str, scope: ConfigScope) -> std::result::Result<(), String> {
        let path = self.config_path(scope)?;
        let mut value = read_mcp_json(&path).await?;
        mcp_servers_object_mut(&mut value).remove(name);
        write_mcp_json(&path, &value).await
    }
}

#[async_trait]
impl AsyncAddMcpConfig for FileMcpConfigWriter {
    async fn add_config(
        &self,
        name: &str,
        config: &McpServerConfig,
        scope: ConfigScope,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        mossen_agent::mcp::config::add_mcp_config(name, config, scope, self)
            .await
            .map_err(|error| error.into())
    }
}

// ── Argument Parsers ──────────────────────────────────────────────────

const VALUE_FLAGS: &[&str] = &[
    "--scope",
    "-s",
    "--transport",
    "-t",
    "--env",
    "-e",
    "--header",
    "-H",
    "--confirm",
];

const SUPPORTED_FLAGS: &[&str] = &[
    "--scope",
    "-s",
    "--transport",
    "-t",
    "--env",
    "-e",
    "--header",
    "-H",
    "--confirm",
    "--dry-run",
];

fn read_flag_value(parts: &[&str], long_flag: &str, short_flag: Option<&str>) -> Option<String> {
    for (i, part) in parts.iter().enumerate() {
        if *part == long_flag || (short_flag == Some(*part)) {
            return parts.get(i + 1).map(|s| s.to_string());
        }
    }
    None
}

fn read_repeated_flag_values(
    parts: &[&str],
    long_flag: &str,
    short_flag: Option<&str>,
) -> Vec<String> {
    let mut values = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if *part == long_flag || (short_flag == Some(*part)) {
            if let Some(v) = parts.get(i + 1) {
                values.push(v.to_string());
            }
        }
    }
    values
}

/// Parse `/mcp add` arguments (translated from parseAddArgs.ts).
fn parse_mcp_add_args(parts: &[&str]) -> ParsedMcpAddArgs {
    // --confirm shortcut
    if let Some(token) = read_flag_value(parts, "--confirm", None) {
        return ParsedMcpAddArgs {
            confirm_token: Some(token),
            ..Default::default()
        };
    }

    let delimiter_index = parts.iter().position(|p| *p == "--");
    let before_command = match delimiter_index {
        Some(idx) => &parts[..idx],
        None => parts,
    };
    let command_parts = match delimiter_index {
        Some(idx) => &parts[idx + 1..],
        None => &[],
    };

    // Check for unsupported flags
    let unsupported = before_command
        .iter()
        .find(|part| part.starts_with('-') && !SUPPORTED_FLAGS.contains(part));
    if let Some(flag) = unsupported {
        return ParsedMcpAddArgs {
            unsupported_flag: Some(flag.to_string()),
            ..Default::default()
        };
    }

    // Collect positional args
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < before_command.len() {
        let part = before_command[i];
        if part == "--dry-run" {
            i += 1;
            continue;
        }
        if VALUE_FLAGS.contains(&part) {
            i += 2;
            continue;
        }
        positional.push(part.to_string());
        i += 1;
    }

    let transport = read_flag_value(before_command, "--transport", Some("-t"));
    let server_name = positional.first().cloned();
    let command_or_url = if !command_parts.is_empty() {
        Some(command_parts[0].to_string())
    } else if matches!(transport.as_deref(), Some("http") | Some("sse")) {
        positional.get(1).cloned()
    } else {
        None
    };

    let env = read_repeated_flag_values(before_command, "--env", Some("-e"));
    let headers = read_repeated_flag_values(before_command, "--header", Some("-H"));

    ParsedMcpAddArgs {
        server_name,
        scope: read_flag_value(before_command, "--scope", Some("-s")),
        transport,
        command_or_url,
        args: if command_parts.len() > 1 {
            command_parts[1..].iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        },
        env: if env.is_empty() { Vec::new() } else { env },
        headers: if headers.is_empty() {
            Vec::new()
        } else {
            headers
        },
        confirm_token: None,
        unsupported_flag: None,
    }
}

/// Parse `/mcp install` arguments (translated from parseInstallArgs.ts).
fn parse_mcp_install_args(parts: &[&str]) -> ParsedMcpInstallArgs {
    let unsupported = parts.iter().find(|part| {
        part.starts_with("--")
            && !matches!(**part, "--name" | "--scope" | "--confirm" | "--dry-run")
    });
    if let Some(flag) = unsupported {
        return ParsedMcpInstallArgs {
            unsupported_flag: Some(flag.to_string()),
            ..Default::default()
        };
    }

    let confirm_token = read_flag_value(parts, "--confirm", None);
    let positional: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|(i, part)| {
            if part.starts_with("--") {
                return false;
            }
            if *i > 0 {
                let prev = parts[i - 1];
                if matches!(prev, "--name" | "--scope" | "--confirm") {
                    return false;
                }
            }
            true
        })
        .map(|(_, p)| p.to_string())
        .collect();

    ParsedMcpInstallArgs {
        source: positional.first().cloned(),
        server_name: read_flag_value(parts, "--name", None),
        scope: read_flag_value(parts, "--scope", None),
        confirm_token,
        unsupported_flag: None,
    }
}

/// Parse `/mcp add-template` arguments (translated from parseTemplateArgs.ts).
fn parse_mcp_add_template_args(parts: &[&str]) -> ParsedMcpAddTemplateArgs {
    let unsupported = parts.iter().find(|part| {
        part.starts_with("--")
            && !matches!(
                **part,
                "--name" | "--scope" | "--root" | "--db" | "--confirm"
            )
    });
    if let Some(flag) = unsupported {
        return ParsedMcpAddTemplateArgs {
            unsupported_flag: Some(flag.to_string()),
            ..Default::default()
        };
    }

    let confirm_token = read_flag_value(parts, "--confirm", None);
    let positional: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|(i, part)| {
            if part.starts_with("--") {
                return false;
            }
            if *i > 0 {
                let prev = parts[i - 1];
                if matches!(prev, "--name" | "--scope" | "--root" | "--db" | "--confirm") {
                    return false;
                }
            }
            true
        })
        .map(|(_, p)| p.to_string())
        .collect();

    ParsedMcpAddTemplateArgs {
        template_name: positional.first().cloned(),
        server_name: read_flag_value(parts, "--name", None),
        scope: read_flag_value(parts, "--scope", None),
        root: read_flag_value(parts, "--root", None),
        db: read_flag_value(parts, "--db", None),
        confirm_token,
        unsupported_flag: None,
    }
}

// ── Status Formatting ─────────────────────────────────────────────────

fn status_label(status: &str) -> &str {
    match status {
        "connected" => "connected",
        "disabled" => "disabled",
        "pending" => "connecting",
        "needs-auth" => "needs auth",
        "failed" => "failed",
        _ => status,
    }
}

async fn current_mcp_clients() -> Vec<McpClientInfo> {
    runtime_status::snapshot()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|status| {
            let (status_label, reconnect_attempt, max_reconnect_attempts) = match status.state {
                RuntimeMcpConnectionState::Connected => ("connected".to_string(), None, None),
                RuntimeMcpConnectionState::Pending => ("pending".to_string(), None, None),
                RuntimeMcpConnectionState::Failed => ("failed".to_string(), None, None),
                RuntimeMcpConnectionState::NeedsAuth => ("needs-auth".to_string(), None, None),
                RuntimeMcpConnectionState::Disabled => ("disabled".to_string(), None, None),
            };
            McpClientInfo {
                name: status.name,
                status: status_label,
                scope: status.scope,
                transport: status.transport,
                tool_count: status.tools_count,
                prompt_count: status.prompts_count,
                resource_count: status.resources_count,
                error: status.last_error,
                reconnect_attempt,
                max_reconnect_attempts,
            }
        })
        .collect()
}

fn format_mcp_status(clients: &[McpClientInfo]) -> String {
    let mut counts = (0usize, 0usize, 0usize, 0usize, 0usize); // connected, disabled, pending, needs_auth, failed
    let total_tools: usize = clients.iter().map(|c| c.tool_count).sum();
    let total_prompts: usize = clients.iter().map(|c| c.prompt_count).sum();
    let total_resources: usize = clients.iter().map(|c| c.resource_count).sum();

    for client in clients {
        match client.status.as_str() {
            "connected" => counts.0 += 1,
            "disabled" => counts.1 += 1,
            "pending" => counts.2 += 1,
            "needs-auth" => counts.3 += 1,
            "failed" => counts.4 += 1,
            _ => {}
        }
    }

    let mut lines = Vec::new();
    lines.push("ℹ MCP status (read-only)".to_string());
    lines.push(String::new());
    lines.push(format!(
        "Servers: {} total, {} connected, {} disabled, {} connecting, {} needs auth, {} failed",
        clients.len(),
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        counts.4
    ));
    lines.push(format!(
        "Capabilities: {} tools, {} prompts/skills, {} resources",
        total_tools, total_prompts, total_resources
    ));
    lines.push(String::new());

    if clients.is_empty() {
        lines.push("No MCP servers are configured in the current session.".to_string());
    } else {
        let mut sorted = clients.to_vec();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        for client in &sorted {
            lines.push(format!("❯ {}", client.name));
            lines.push(format!("  status:    {}", status_label(&client.status)));
            lines.push(format!(
                "  scope:     {}  transport: {}",
                client.scope, client.transport
            ));
            lines.push(format!(
                "  exposes:   {} tools, {} prompts, {} resources",
                client.tool_count, client.prompt_count, client.resource_count
            ));
            if client.status == "failed" {
                if let Some(ref err) = client.error {
                    lines.push(format!("  error:     {}", err));
                }
            }
            if client.status == "pending" {
                if let (Some(attempt), Some(max)) =
                    (client.reconnect_attempt, client.max_reconnect_attempts)
                {
                    lines.push(format!("  reconnect: {}/{}", attempt, max));
                }
            }
        }
    }

    lines.push(String::new());
    lines.push(
        "This command does not reconnect, enable, disable, authenticate, or modify MCP config."
            .to_string(),
    );
    lines.join("\n")
}

// ── MCP Add Formatting ────────────────────────────────────────────────

fn format_mcp_add_plan(plan: &mossen_agent::mcp::slash_add_plan::McpSlashAddPlan) -> String {
    let ttl_min = mossen_agent::mcp::slash_add_plan::MCP_SLASH_ADD_PLAN_TOKEN_TTL_MS / 60_000;
    let config = mcp_config_summary(&plan.config);

    let mut lines = Vec::new();
    lines.push("ℹ MCP add dry-run".to_string());
    lines.push(String::new());
    lines.push(format!("Server name: {}", plan.server_name));
    lines.push(format!("Scope: {}", scope_label(plan.scope)));
    lines.push(format!("Transport: {}", plan.transport));
    lines.push(format!("Config: {}", config));
    lines.push(String::new());
    lines.push(
        "No files were modified. Confirming will write this MCP server through the existing addMcpConfig() path; it will not auto-connect the server."
            .to_string(),
    );
    lines.push(String::new());
    lines.push(format!(
        "To install within {} min: /mcp add --confirm {}",
        ttl_min, plan.token
    ));
    lines.join("\n")
}

// ── MCP Install Formatting ────────────────────────────────────────────

fn format_mcp_install_plan(
    plan: &mossen_agent::mcp::remote_install_plan::McpRemoteInstallPlan,
) -> String {
    let ttl_min = mossen_agent::mcp::remote_install_plan::MCP_REMOTE_PLAN_TOKEN_TTL_MS / 60_000;

    let mut lines = Vec::new();
    lines.push("ℹ MCP remote install dry-run".to_string());
    lines.push(String::new());
    lines.push(format!("Source: {}", plan.source));
    lines.push(format!("Server name: {}", plan.server_name));
    lines.push(format!("Scope: {}", scope_label(plan.scope)));
    if plan.available_servers.len() > 1 {
        lines.push(format!(
            "Available servers: {}",
            plan.available_servers.join(", ")
        ));
    }
    lines.push(String::new());
    lines.push(
        "No files were modified. Confirming will write this MCP server through the existing addMcpConfig() path; it will not auto-connect the server."
            .to_string(),
    );
    lines.push(String::new());
    lines.push(format!(
        "To install within {} min: /mcp install --confirm {}",
        ttl_min, plan.token
    ));
    lines.join("\n")
}

// ── MCP Template Formatting ───────────────────────────────────────────

fn format_templates_list() -> String {
    let mut lines = Vec::new();
    lines.push("ℹ Built-in MCP templates (read-only inventory)".to_string());
    lines.push(String::new());
    lines.push(
        "These templates are not enabled automatically. Copy a template into settings only after reviewing scope, credentials, and side effects."
            .to_string(),
    );
    lines.push(String::new());

    for template in mossen_agent::mcp::builtin_templates::get_builtin_mcp_templates() {
        let localized =
            mossen_agent::mcp::builtin_templates::get_localized_builtin_mcp_template_text(
                template.name,
            );
        lines.push(format!("❯ {}", template.name));
        lines.push(format!(
            "  title:       {}",
            localized.title.unwrap_or(template.title)
        ));
        lines.push(format!("  risk:        {:?}", template.risk).to_lowercase());
        lines.push(format!(
            "  readonly:    {}",
            if template.read_only { "yes" } else { "no" }
        ));
        lines.push(format!(
            "  credentials: {}",
            if template.requires_credentials {
                "required"
            } else {
                "not required"
            }
        ));
        lines.push(format!(
            "  network:     {}",
            if template.requires_network {
                "required"
            } else {
                "not required"
            }
        ));
        lines.push(format!(
            "  config:      {}",
            mcp_config_summary(&template.config)
        ));
        lines.push(format!(
            "  {}",
            localized.description.unwrap_or(template.description)
        ));
        let notes = localized.notes.unwrap_or(template.notes);
        for note in notes {
            lines.push(format!("  - {}", note));
        }
        lines.push(String::new());
    }

    lines.push(
        "Next step: install one with /mcp add-template <template> and confirm the dry-run token."
            .to_string(),
    );
    lines.join("\n")
}

fn format_mcp_add_template_plan(
    plan: &mossen_agent::mcp::builtin_template_plan::McpTemplateInstallPlan,
) -> String {
    let ttl_min = mossen_agent::mcp::builtin_template_plan::MCP_TEMPLATE_PLAN_TOKEN_TTL_MS / 60_000;
    let mut lines = Vec::new();
    lines.push("ℹ MCP add-template dry-run".to_string());
    lines.push(String::new());
    lines.push(format!("Template: {} ({})", plan.template_name, plan.title));
    lines.push(format!("Server name: {}", plan.server_name));
    lines.push(format!("Scope: {}", scope_label(plan.scope)));
    lines.push(format!(
        "Readonly: {}  Risk: {}",
        if plan.read_only { "yes" } else { "no" },
        plan.risk
    ));
    lines.push(format!("Config: {}", mcp_config_summary(&plan.config)));
    for note in &plan.notes {
        lines.push(format!("Note: {}", note));
    }
    lines.push(String::new());
    lines.push(
        "No files were modified. Confirming will write this MCP server through the existing addMcpConfig() path; it will not auto-connect the server."
            .to_string(),
    );
    lines.push(String::new());
    lines.push(format!(
        "To install within {} min: /mcp add-template --confirm {}",
        ttl_min, plan.token
    ));
    lines.join("\n")
}

// ── MCP Toggle ────────────────────────────────────────────────────────

fn format_mcp_toggle(action: &str, target: &str) -> String {
    let is_enabling = action == "enable";
    let verb = if is_enabling { "Enabled" } else { "Disabled" };
    if target == "all" {
        format!("{} all MCP servers", verb)
    } else {
        format!("MCP server \"{}\" {}", target, action.to_lowercase() + "d")
    }
}

fn scope_label(scope: ConfigScope) -> &'static str {
    match scope {
        ConfigScope::Local => "local",
        ConfigScope::User => "user",
        ConfigScope::Project => "project",
        ConfigScope::Dynamic => "dynamic",
        ConfigScope::Enterprise => "enterprise",
        ConfigScope::Hosted => "hosted",
        ConfigScope::Managed => "managed",
    }
}

fn mcp_config_summary(config: &McpServerConfig) -> String {
    match config {
        McpServerConfig::Stdio { command, args, .. } => {
            let mut parts = vec![command.clone()];
            parts.extend(args.iter().cloned());
            parts.join(" ")
        }
        McpServerConfig::Sse { url, .. } => format!("sse {url}"),
        McpServerConfig::SseIde { url, ide_name, .. } => format!("sse-ide {ide_name} {url}"),
        McpServerConfig::Http { url, .. } => format!("http {url}"),
        McpServerConfig::Ws { url, .. } => format!("ws {url}"),
        McpServerConfig::WsIde { url, ide_name, .. } => format!("ws-ide {ide_name} {url}"),
        McpServerConfig::Sdk { name } => format!("sdk {name}"),
        McpServerConfig::HostedProxy { url, id } => format!("hosted {id} {url}"),
    }
}

fn slash_add_error_text(error: McpSlashAddPlanError) -> String {
    match error {
        McpSlashAddPlanError::MissingServerName => {
            "✗ Missing MCP server name.\nUsage: /mcp add <name> [--scope local|user|project] -- <command> [args...]".to_string()
        }
        McpSlashAddPlanError::MissingCommand => {
            "✗ Missing MCP command or URL.\nExample: /mcp add playwright --scope local -- npx -y @playwright/mcp@latest".to_string()
        }
        McpSlashAddPlanError::InvalidScope { scope } => {
            format!("✗ Invalid MCP scope: {}. Use local, user, or project.", scope.unwrap_or_else(|| "(missing)".to_string()))
        }
        McpSlashAddPlanError::InvalidTransport { transport } => {
            format!("✗ Invalid MCP transport: {}. Use stdio, sse, or http.", transport.unwrap_or_else(|| "(missing)".to_string()))
        }
        McpSlashAddPlanError::InvalidEnv { message }
        | McpSlashAddPlanError::InvalidHeader { message }
        | McpSlashAddPlanError::InvalidConfig { reason: message }
        | McpSlashAddPlanError::InstallFailed { message } => format!("✗ {message}"),
        McpSlashAddPlanError::UnknownToken { token } => {
            format!("✗ Unknown or already used MCP add token: {token}")
        }
        McpSlashAddPlanError::ExpiredToken { token } => {
            format!("✗ Expired MCP add token: {token}. Run /mcp add again.")
        }
    }
}

fn template_error_text(error: McpTemplatePlanError) -> String {
    match error {
        McpTemplatePlanError::UnknownTemplate {
            template_name,
            available_templates,
        } => format!(
            "✗ Unknown MCP template: {}\nAvailable templates: {}",
            template_name.unwrap_or_else(|| "(missing)".to_string()),
            available_templates.join(", ")
        ),
        McpTemplatePlanError::MissingParameter {
            template_name,
            missing,
        } => format!(
            "✗ MCP template {template_name} missing required parameter(s): {}",
            missing
                .into_iter()
                .map(|parameter| format!("{:?}", parameter).to_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        McpTemplatePlanError::PathNotAbsolute { parameter, value } => format!(
            "✗ MCP template parameter {} must be an absolute path: {}",
            format!("{:?}", parameter).to_lowercase(),
            value
        ),
        McpTemplatePlanError::InvalidScope { scope } => format!(
            "✗ Invalid MCP scope: {}. Use local, user, or project.",
            scope.unwrap_or_else(|| "(missing)".to_string())
        ),
        McpTemplatePlanError::UnknownToken { token } => {
            format!("✗ Unknown or already used MCP template token: {token}")
        }
        McpTemplatePlanError::ExpiredToken { token } => {
            format!("✗ Expired MCP template token: {token}. Run /mcp add-template again.")
        }
        McpTemplatePlanError::InstallFailed { message } => format!("✗ {message}"),
    }
}

fn remote_error_text(error: McpRemotePlanError) -> String {
    match error {
        McpRemotePlanError::MissingSource => {
            "✗ Missing remote MCP config URL.\nUsage: /mcp install --dry-run <url> [--name server] [--scope local|user|project]".to_string()
        }
        McpRemotePlanError::InvalidScope { scope } => format!(
            "✗ Invalid MCP scope: {}. Use local, user, or project.",
            scope.unwrap_or_else(|| "(missing)".to_string())
        ),
        McpRemotePlanError::InvalidSource { reason } => format!("✗ {reason}"),
        McpRemotePlanError::MultipleServers { available_servers } => format!(
            "✗ Remote MCP config contains multiple servers. Re-run with --name <server>. Available: {}",
            available_servers.join(", ")
        ),
        McpRemotePlanError::MissingServerName => {
            "✗ Remote MCP config is a bare server config. Re-run with --name <server>.".to_string()
        }
        McpRemotePlanError::ServerNotFound {
            server_name,
            available_servers,
        } => format!(
            "✗ Remote MCP server {server_name} was not found. Available: {}",
            available_servers.join(", ")
        ),
        McpRemotePlanError::UnknownToken { token } => {
            format!("✗ Unknown or already used remote MCP install token: {token}")
        }
        McpRemotePlanError::ExpiredToken { token } => {
            format!("✗ Expired remote MCP install token: {token}. Run /mcp install again.")
        }
        McpRemotePlanError::InstallFailed { message } => format!("✗ {message}"),
    }
}

async fn execute_add_confirm(token: &str, ctx: &CommandContext) -> CommandResult {
    let writer = FileMcpConfigWriter::new(ctx.cwd.clone());
    match mossen_agent::mcp::slash_add_plan::execute_mcp_slash_add_plan(token, &writer).await {
        McpSlashAddPlanResult::Ok { plan } => CommandResult::System(format!(
            "✓ Installed MCP server {}\nscope: {}\nconfig: {}\nNo auto-connect was performed; restart or reconnect MCP to use it.",
            plan.server_name,
            scope_label(plan.scope),
            mcp_config_summary(&plan.config)
        )),
        McpSlashAddPlanResult::Err { error } => CommandResult::Error(slash_add_error_text(error)),
    }
}

async fn execute_template_confirm(token: &str, ctx: &CommandContext) -> CommandResult {
    let writer = FileMcpConfigWriter::new(ctx.cwd.clone());
    match mossen_agent::mcp::builtin_template_plan::execute_mcp_template_install_plan(token, writer)
        .await
    {
        McpTemplateInstallResult::Ok { plan } => CommandResult::System(format!(
            "✓ Installed MCP template {} as {}\nscope: {}\nNo auto-connect was performed; restart or reconnect MCP to use it.",
            plan.template_name,
            plan.server_name,
            scope_label(plan.scope)
        )),
        McpTemplateInstallResult::Err { error } => CommandResult::Error(template_error_text(error)),
    }
}

async fn execute_remote_confirm(token: &str, ctx: &CommandContext) -> CommandResult {
    let writer = FileMcpConfigWriter::new(ctx.cwd.clone());
    match mossen_agent::mcp::remote_install_plan::execute_mcp_remote_install_plan(token, &writer)
        .await
    {
        McpRemoteInstallResult::Ok { plan } => CommandResult::System(format!(
            "✓ Installed remote MCP server {}\nscope: {}\nsource: {}\nNo auto-connect was performed; restart or reconnect MCP to use it.",
            plan.server_name,
            scope_label(plan.scope),
            plan.source
        )),
        McpRemoteInstallResult::Err { error } => CommandResult::Error(remote_error_text(error)),
    }
}

// ── Main Directive ────────────────────────────────────────────────────

/// MCP (Model Context Protocol) management directive.
///
/// Subcommands (from mcp.tsx router):
///   (none)/no-redirect  → MCPSettings widget
///   reconnect <name>    → reconnect a server
///   templates/template  → list builtin templates
///   status/stat         → read-only status view
///   add ...             → add a server (dry-run + confirm)
///   add-template ...    → add from builtin template
///   install ...         → install from remote config URL
///   enable/disable [name|all] → toggle server(s)
///   help                → subcommand help
pub struct BridgesDirective;

/// Available MCP subcommands for help display.
const MCP_SUBCOMMANDS: &[(&str, &str)] = &[
    ("status", "Show detailed connection status for all servers"),
    ("templates", "List built-in MCP server templates"),
    ("add", "Add a new MCP server configuration (dry-run)"),
    (
        "add-template",
        "Add a server from a built-in template (dry-run)",
    ),
    ("install", "Install from a remote MCP config URL (dry-run)"),
    ("enable", "Enable an MCP server (or all)"),
    ("disable", "Disable an MCP server (or all)"),
    ("reconnect", "Reconnect a named MCP server"),
];

#[async_trait]
impl Directive for BridgesDirective {
    fn name(&self) -> &str {
        "mcp"
    }

    fn description(&self) -> &str {
        "Manage MCP servers"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[status|templates|add-template|enable|disable [server-name]]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Base /mcp command — redirect internal users to /plugins
            if ctx.is_internal_user() {
                return Ok(CommandResult::Text(
                    "Redirecting to /plugins installed tab for MCP management.\nUse /mcp no-redirect to bypass."
                        .to_string(),
                ));
            }
            // MCPSettings widget
            return Ok(CommandResult::Text(
                "MCP Settings\n\nNo MCP servers configured.\nUse /mcp add to add a server, or /mcp templates to browse built-in templates."
                    .to_string(),
            ));
        }

        let subcommand = args[0].to_lowercase();
        let rest: Vec<&str> = args[1..].to_vec();

        match subcommand.as_str() {
            "no-redirect" => {
                // Bypass the redirect for testing — show MCPSettings directly
                Ok(CommandResult::Text(
                    "MCP Settings\n\nNo MCP servers configured.\nUse /mcp add to add a server, or /mcp templates to browse built-in templates."
                        .to_string(),
                ))
            }

            "reconnect" => {
                if rest.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /mcp reconnect <server-name>".to_string(),
                    ));
                }
                let server_name = rest.join(" ");
                Ok(CommandResult::System(format!(
                    "Reconnecting MCP server: {}",
                    server_name
                )))
            }

            "templates" | "template" => Ok(CommandResult::Text(format_templates_list())),

            "status" | "stat" => {
                let clients = current_mcp_clients().await;
                Ok(CommandResult::Text(format_mcp_status(&clients)))
            }

            "add" => {
                let rest_strs: Vec<&str> = rest.to_vec();
                let parsed = parse_mcp_add_args(&rest_strs);

                if let Some(ref flag) = parsed.unsupported_flag {
                    return Ok(CommandResult::Error(format!(
                        "✗ Unsupported flag for /mcp add: {}",
                        flag
                    )));
                }

                if let Some(ref token) = parsed.confirm_token {
                    return Ok(execute_add_confirm(token, ctx).await);
                }

                match mossen_agent::mcp::slash_add_plan::get_mcp_slash_add_plan(
                    parsed.server_name.as_deref(),
                    parsed.scope.as_deref(),
                    parsed.transport.as_deref(),
                    parsed.command_or_url.as_deref(),
                    Some(&parsed.args),
                    Some(&parsed.env),
                    Some(&parsed.headers),
                ) {
                    McpSlashAddPlanResult::Ok { plan } => {
                        Ok(CommandResult::Text(format_mcp_add_plan(&plan)))
                    }
                    McpSlashAddPlanResult::Err { error } => {
                        Ok(CommandResult::Error(slash_add_error_text(error)))
                    }
                }
            }

            "add-template" => {
                let rest_strs: Vec<&str> = rest.to_vec();
                let parsed = parse_mcp_add_template_args(&rest_strs);

                if let Some(ref flag) = parsed.unsupported_flag {
                    return Ok(CommandResult::Error(format!(
                        "✗ Unsupported flag for /mcp add-template: {}",
                        flag
                    )));
                }

                if let Some(ref token) = parsed.confirm_token {
                    return Ok(execute_template_confirm(token, ctx).await);
                }

                match mossen_agent::mcp::builtin_template_plan::get_mcp_template_install_plan(
                    parsed.template_name.as_deref(),
                    parsed.server_name.as_deref(),
                    parsed.scope.as_deref(),
                    parsed.root.as_deref(),
                    parsed.db.as_deref(),
                ) {
                    McpTemplateInstallResult::Ok { plan } => {
                        Ok(CommandResult::Text(format_mcp_add_template_plan(&plan)))
                    }
                    McpTemplateInstallResult::Err { error } => {
                        Ok(CommandResult::Error(template_error_text(error)))
                    }
                }
            }

            "install" => {
                let rest_strs: Vec<&str> = rest.to_vec();
                let parsed = parse_mcp_install_args(&rest_strs);

                if let Some(ref flag) = parsed.unsupported_flag {
                    return Ok(CommandResult::Error(format!(
                        "✗ Unsupported flag for /mcp install: {}",
                        flag
                    )));
                }

                if let Some(ref token) = parsed.confirm_token {
                    return Ok(execute_remote_confirm(token, ctx).await);
                }

                match mossen_agent::mcp::remote_install_plan::get_mcp_remote_install_plan(
                    parsed.source.as_deref(),
                    parsed.server_name.as_deref(),
                    parsed.scope.as_deref(),
                )
                .await
                {
                    McpRemoteInstallResult::Ok { plan } => {
                        Ok(CommandResult::Text(format_mcp_install_plan(&plan)))
                    }
                    McpRemoteInstallResult::Err { error } => {
                        Ok(CommandResult::Error(remote_error_text(error)))
                    }
                }
            }

            "enable" | "disable" => {
                let target = if rest.is_empty() {
                    "all".to_string()
                } else {
                    rest.join(" ")
                };
                Ok(CommandResult::System(format_mcp_toggle(
                    &subcommand,
                    &target,
                )))
            }

            "help" | "-h" | "--help" => {
                let mut help = String::from("Usage: /mcp [subcommand]\n\nSubcommands:\n");
                for (cmd, desc) in MCP_SUBCOMMANDS {
                    help.push_str(&format!("  {:16} {}\n", cmd, desc));
                }
                Ok(CommandResult::Text(help))
            }

            _ => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /mcp help for available commands.",
                subcommand
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_context(cwd: PathBuf) -> CommandContext {
        CommandContext {
            cwd,
            is_non_interactive: false,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars: HashMap::new(),
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    fn result_text(result: CommandResult) -> String {
        match result {
            CommandResult::Text(text)
            | CommandResult::System(text)
            | CommandResult::Error(text) => text,
            other => panic!("unexpected command result: {other:?}"),
        }
    }

    fn extract_confirm_token(output: &str, marker: &str) -> String {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(marker))
            .map(str::trim)
            .expect("confirm token line")
            .to_string()
    }

    fn read_project_mcp_config(cwd: &Path) -> Value {
        let path = cwd.join(".mcp.json");
        serde_json::from_str(&std::fs::read_to_string(path).expect("read .mcp.json"))
            .expect("parse .mcp.json")
    }

    #[tokio::test]
    async fn mcp_add_confirm_writes_project_config_and_token_is_one_shot() {
        mossen_agent::mcp::slash_add_plan::reset_mcp_slash_add_plan_store_for_testing();
        let temp = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(temp.path().to_path_buf());

        let dry_run = BridgesDirective
            .execute(
                &[
                    "add",
                    "demo",
                    "--scope",
                    "project",
                    "--",
                    "python3",
                    "server.py",
                ],
                &ctx,
            )
            .await
            .expect("dry run");
        let dry_run_text = result_text(dry_run);
        assert!(dry_run_text.contains("MCP add dry-run"), "{dry_run_text}");
        assert!(!temp.path().join(".mcp.json").exists());
        let token = extract_confirm_token(
            &dry_run_text,
            "To install within 10 min: /mcp add --confirm ",
        );

        let confirm = BridgesDirective
            .execute(&["add", "--confirm", &token], &ctx)
            .await
            .expect("confirm");
        let confirm_text = result_text(confirm);
        assert!(
            confirm_text.contains("Installed MCP server demo"),
            "{confirm_text}"
        );
        let config = read_project_mcp_config(temp.path());
        assert_eq!(
            config["mcpServers"]["demo"]["command"].as_str(),
            Some("python3")
        );
        assert_eq!(
            config["mcpServers"]["demo"]["args"][0].as_str(),
            Some("server.py")
        );

        let second = BridgesDirective
            .execute(&["add", "--confirm", &token], &ctx)
            .await
            .expect("second confirm");
        let second_text = result_text(second);
        assert!(
            second_text.contains("Unknown or already used MCP add token"),
            "{second_text}"
        );
    }

    #[tokio::test]
    async fn mcp_add_template_confirm_writes_instantiated_builtin_template() {
        mossen_agent::mcp::builtin_template_plan::reset_mcp_template_plan_store_for_testing();
        let temp = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(temp.path().to_path_buf());
        let root = temp.path().join("repo-root");
        tokio::fs::create_dir_all(&root).await.expect("create root");
        let root_arg = root.to_string_lossy().to_string();

        let dry_run = BridgesDirective
            .execute(
                &[
                    "add-template",
                    "filesystem-readonly",
                    "--name",
                    "fs",
                    "--scope",
                    "project",
                    "--root",
                    &root_arg,
                ],
                &ctx,
            )
            .await
            .expect("dry run");
        let dry_run_text = result_text(dry_run);
        assert!(
            dry_run_text.contains("MCP add-template dry-run"),
            "{dry_run_text}"
        );
        let token = extract_confirm_token(
            &dry_run_text,
            "To install within 10 min: /mcp add-template --confirm ",
        );

        let confirm = BridgesDirective
            .execute(&["add-template", "--confirm", &token], &ctx)
            .await
            .expect("confirm");
        let confirm_text = result_text(confirm);
        assert!(
            confirm_text.contains("Installed MCP template filesystem-readonly as fs"),
            "{confirm_text}"
        );
        let config = read_project_mcp_config(temp.path());
        assert_eq!(
            config["mcpServers"]["fs"]["command"].as_str(),
            Some("mcp-server-filesystem")
        );
        assert_eq!(
            config["mcpServers"]["fs"]["args"][1].as_str(),
            Some(root_arg.as_str())
        );
    }

    #[tokio::test]
    async fn mcp_templates_lists_current_rust_builtin_inventory() {
        let ctx = test_context(tempfile::tempdir().expect("tempdir").path().to_path_buf());
        let output = BridgesDirective
            .execute(&["templates"], &ctx)
            .await
            .expect("templates");
        let text = result_text(output);
        for name in [
            "filesystem-readonly",
            "git-readonly",
            "local-docs",
            "playwright-local",
            "sqlite-readonly",
        ] {
            assert!(text.contains(name), "missing {name}: {text}");
        }
        assert!(text.contains("文件系统只读"), "{text}");
    }

    async fn serve_remote_mcp_config(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept request");
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        });
        format!("http://{addr}/mcp.json")
    }

    #[tokio::test]
    async fn mcp_remote_install_confirm_writes_selected_server() {
        mossen_agent::mcp::remote_install_plan::reset_mcp_remote_plan_store_for_testing();
        let temp = tempfile::tempdir().expect("tempdir");
        let ctx = test_context(temp.path().to_path_buf());
        let url = serve_remote_mcp_config(
            r#"{"mcpServers":{"remote":{"type":"stdio","command":"node","args":["server.js"]}}}"#,
        )
        .await;

        let dry_run = BridgesDirective
            .execute(&["install", &url, "--scope", "project"], &ctx)
            .await
            .expect("dry run");
        let dry_run_text = result_text(dry_run);
        assert!(
            dry_run_text.contains("MCP remote install dry-run"),
            "{dry_run_text}"
        );
        let token = extract_confirm_token(
            &dry_run_text,
            "To install within 10 min: /mcp install --confirm ",
        );

        let confirm = BridgesDirective
            .execute(&["install", "--confirm", &token], &ctx)
            .await
            .expect("confirm");
        let confirm_text = result_text(confirm);
        assert!(
            confirm_text.contains("Installed remote MCP server remote"),
            "{confirm_text}"
        );
        let config = read_project_mcp_config(temp.path());
        assert_eq!(
            config["mcpServers"]["remote"]["command"].as_str(),
            Some("node")
        );
        assert_eq!(
            config["mcpServers"]["remote"]["args"][0].as_str(),
            Some("server.js")
        );
    }

    #[test]
    fn mcp_status_formats_all_connection_states_and_counts() {
        let text = format_mcp_status(&[
            McpClientInfo {
                name: "ok".to_string(),
                status: "connected".to_string(),
                scope: "project".to_string(),
                transport: "stdio".to_string(),
                tool_count: 2,
                prompt_count: 1,
                resource_count: 3,
                error: None,
                reconnect_attempt: None,
                max_reconnect_attempts: None,
            },
            McpClientInfo {
                name: "auth".to_string(),
                status: "needs-auth".to_string(),
                scope: "user".to_string(),
                transport: "http".to_string(),
                tool_count: 0,
                prompt_count: 0,
                resource_count: 0,
                error: None,
                reconnect_attempt: None,
                max_reconnect_attempts: None,
            },
            McpClientInfo {
                name: "bad".to_string(),
                status: "failed".to_string(),
                scope: "project".to_string(),
                transport: "stdio".to_string(),
                tool_count: 0,
                prompt_count: 0,
                resource_count: 0,
                error: Some("boom".to_string()),
                reconnect_attempt: None,
                max_reconnect_attempts: None,
            },
        ]);
        assert!(text.contains("3 total, 1 connected"), "{text}");
        assert!(text.contains("1 needs auth, 1 failed"), "{text}");
        assert!(
            text.contains("Capabilities: 2 tools, 1 prompts/skills, 3 resources"),
            "{text}"
        );
        assert!(text.contains("error:     boom"), "{text}");
    }
}
