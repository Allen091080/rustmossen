//! MCP utility functions.
//!
//! Translates `services/mcp/utils.ts`.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::mcp::mcp_string_utils::mcp_info_from_string;
use crate::mcp::normalization::normalize_name_for_mcp;
use crate::mcp::types::{ConfigScope, McpServerConfig, ScopedMcpServerConfig};

/// Tool representation (minimal).
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: Option<String>,
    pub is_mcp: Option<bool>,
}

/// Command representation (minimal).
#[derive(Debug, Clone)]
pub struct Command {
    pub name: Option<String>,
    pub is_mcp: Option<bool>,
    pub r#type: Option<String>,
    pub loaded_from: Option<String>,
}

/// Server resource.
#[derive(Debug, Clone)]
pub struct ServerResource {
    pub server: String,
    pub uri: String,
    pub name: String,
}

/// MCPServerConnection minimal representation.
pub struct McpConnectionInfo {
    pub name: String,
    pub config: ScopedMcpServerConfig,
}

/// Filter tools by MCP server name.
pub fn filter_tools_by_server<'a>(tools: &'a [Tool], server_name: &'_ str) -> Vec<&'a Tool> {
    let prefix = format!("mcp__{}__", normalize_name_for_mcp(server_name));
    tools
        .iter()
        .filter(|t| {
            t.name
                .as_ref()
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect()
}

/// Check if a command belongs to a given MCP server.
pub fn command_belongs_to_server(command: &Command, server_name: &str) -> bool {
    let normalized = normalize_name_for_mcp(server_name);
    let name = match &command.name {
        Some(n) => n,
        None => return false,
    };
    name.starts_with(&format!("mcp__{}__", normalized))
        || name.starts_with(&format!("{}:", normalized))
}

/// Filter commands by MCP server name.
pub fn filter_commands_by_server<'a>(
    commands: &'a [Command],
    server_name: &str,
) -> Vec<&'a Command> {
    commands
        .iter()
        .filter(|c| command_belongs_to_server(c, server_name))
        .collect()
}

/// Filter MCP prompts (not skills) by server.
pub fn filter_mcp_prompts_by_server<'a>(
    commands: &'a [Command],
    server_name: &str,
) -> Vec<&'a Command> {
    commands
        .iter()
        .filter(|c| {
            command_belongs_to_server(c, server_name)
                && !(c.r#type.as_deref() == Some("prompt")
                    && c.loaded_from.as_deref() == Some("mcp"))
        })
        .collect()
}

/// Filter resources by MCP server name.
pub fn filter_resources_by_server<'a>(
    resources: &'a [ServerResource],
    server_name: &str,
) -> Vec<&'a ServerResource> {
    resources
        .iter()
        .filter(|r| r.server == server_name)
        .collect()
}

/// Exclude tools belonging to a specific MCP server.
pub fn exclude_tools_by_server<'a>(tools: &'a [Tool], server_name: &str) -> Vec<&'a Tool> {
    let prefix = format!("mcp__{}__", normalize_name_for_mcp(server_name));
    tools
        .iter()
        .filter(|t| {
            !t.name
                .as_ref()
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect()
}

/// Exclude commands belonging to a specific MCP server.
pub fn exclude_commands_by_server<'a>(
    commands: &'a [Command],
    server_name: &str,
) -> Vec<&'a Command> {
    commands
        .iter()
        .filter(|c| !command_belongs_to_server(c, server_name))
        .collect()
}

/// Exclude resources belonging to a specific MCP server.
pub fn exclude_resources_by_server(
    resources: &HashMap<String, Vec<ServerResource>>,
    server_name: &str,
) -> HashMap<String, Vec<ServerResource>> {
    let mut result = resources.clone();
    result.remove(server_name);
    result
}

/// Stable hash of an MCP server config for change detection.
pub fn hash_mcp_config(config: &ScopedMcpServerConfig) -> String {
    let serialized = serde_json::to_string(&config.config).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Remove stale MCP clients and their tools/commands/resources.
pub fn exclude_stale_plugin_clients(
    clients: &[McpConnectionInfo],
    configs: &HashMap<String, ScopedMcpServerConfig>,
) -> Vec<String> {
    let mut stale_names = Vec::new();
    for client in clients {
        match configs.get(&client.name) {
            None => {
                if client.config.scope == ConfigScope::Dynamic {
                    stale_names.push(client.name.clone());
                }
            }
            Some(fresh) => {
                if hash_mcp_config(&client.config) != hash_mcp_config(fresh) {
                    stale_names.push(client.name.clone());
                }
            }
        }
    }
    stale_names
}

/// Check if a tool name belongs to a specific MCP server.
pub fn is_tool_from_mcp_server(tool_name: &str, server_name: &str) -> bool {
    mcp_info_from_string(tool_name)
        .map(|info| info.server_name == server_name)
        .unwrap_or(false)
}

/// Check if a tool belongs to any MCP server.
pub fn is_mcp_tool(tool: &Tool) -> bool {
    tool.name
        .as_ref()
        .map(|n| n.starts_with("mcp__"))
        .unwrap_or(false)
        || tool.is_mcp == Some(true)
}

/// Check if a command belongs to any MCP server.
pub fn is_mcp_command(command: &Command) -> bool {
    command
        .name
        .as_ref()
        .map(|n| n.starts_with("mcp__"))
        .unwrap_or(false)
        || command.is_mcp == Some(true)
}

/// Describe the file path for a given MCP config scope.
pub fn describe_mcp_config_file_path(
    scope: ConfigScope,
    global_file: &str,
    cwd: &str,
    enterprise_path: &str,
) -> String {
    match scope {
        ConfigScope::User => global_file.to_string(),
        ConfigScope::Project => format!("{}/.mcp.json", cwd),
        ConfigScope::Local => format!("{} [project: {}]", global_file, cwd),
        ConfigScope::Dynamic => "Dynamically configured".to_string(),
        ConfigScope::Enterprise => enterprise_path.to_string(),
        ConfigScope::Hosted => "Hosted platform".to_string(),
        ConfigScope::Managed => "Managed by platform".to_string(),
    }
}

/// Get scope label for display.
pub fn get_scope_label(scope: ConfigScope) -> &'static str {
    match scope {
        ConfigScope::Local => "Local config (private to you in this project)",
        ConfigScope::Project => "Project config (shared via .mcp.json)",
        ConfigScope::User => "User config (available in all your projects)",
        ConfigScope::Dynamic => "Dynamic config (from command line)",
        ConfigScope::Enterprise => "Enterprise config (managed by your organization)",
        ConfigScope::Hosted => "Hosted platform config",
        ConfigScope::Managed => "Managed by platform",
    }
}

/// Ensure a valid config scope.
pub fn ensure_config_scope(scope: Option<&str>) -> Result<ConfigScope, String> {
    match scope {
        None => Ok(ConfigScope::Local),
        Some("local") => Ok(ConfigScope::Local),
        Some("project") => Ok(ConfigScope::Project),
        Some("user") => Ok(ConfigScope::User),
        Some("dynamic") => Ok(ConfigScope::Dynamic),
        Some("enterprise") => Ok(ConfigScope::Enterprise),
        Some("hosted") => Ok(ConfigScope::Hosted),
        Some(other) => Err(format!(
            "Invalid scope: {}. Must be one of: local, project, user, dynamic, enterprise, hosted",
            other
        )),
    }
}

/// Ensure a valid transport type.
pub fn ensure_transport(transport: Option<&str>) -> Result<&'static str, String> {
    match transport {
        None => Ok("stdio"),
        Some("stdio") => Ok("stdio"),
        Some("sse") => Ok("sse"),
        Some("http") => Ok("http"),
        Some(other) => Err(format!(
            "Invalid transport type: {}. Must be one of: stdio, sse, http",
            other
        )),
    }
}

/// Parse headers from "Key: Value" format.
pub fn parse_headers(header_array: &[String]) -> Result<HashMap<String, String>, String> {
    let mut headers = HashMap::new();
    for header in header_array {
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
        headers.insert(key, value);
    }
    Ok(headers)
}

/// Type guards for MCP server config types.
pub fn is_stdio_config(config: &McpServerConfig) -> bool {
    matches!(config, McpServerConfig::Stdio { .. })
}

pub fn is_sse_config(config: &McpServerConfig) -> bool {
    matches!(config, McpServerConfig::Sse { .. })
}

pub fn is_http_config(config: &McpServerConfig) -> bool {
    matches!(config, McpServerConfig::Http { .. })
}

pub fn is_websocket_config(config: &McpServerConfig) -> bool {
    matches!(config, McpServerConfig::Ws { .. })
}

/// Extracts the MCP server base URL (without query string) for analytics logging.
pub fn get_logging_safe_mcp_base_url(config: &McpServerConfig) -> Option<String> {
    let url_str = match config {
        McpServerConfig::Sse { url, .. }
        | McpServerConfig::Http { url, .. }
        | McpServerConfig::Ws { url, .. }
        | McpServerConfig::HostedProxy { url, .. } => url,
        _ => return None,
    };

    match url::Url::parse(url_str) {
        Ok(mut u) => {
            u.set_query(None);
            Some(u.to_string().trim_end_matches('/').to_string())
        }
        Err(_) => None,
    }
}

/// Agent MCP server info for display.
#[derive(Debug, Clone)]
pub struct AgentMcpServerInfo {
    pub name: String,
    pub source_agents: Vec<String>,
    pub transport: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub needs_auth: bool,
}

/// Agent definition (minimal projection used here). Mirrors the parts of the
/// TS `AgentDefinition` type consumed by `extractAgentMcpServers`.
#[derive(Debug, Clone, Default)]
pub struct AgentDefinitionForMcp {
    pub agent_type: String,
    pub mcp_servers: Vec<AgentMcpServerSpec>,
}

/// An item of the `mcpServers` list inside an agent frontmatter.
///
/// TS spec accepts either a string reference (name of an already-registered
/// server in the global config) or an inline definition `{ [name]: config }`.
#[derive(Debug, Clone)]
pub enum AgentMcpServerSpec {
    /// Reference to an already-registered server by name.
    Reference(String),
    /// Inline server definition.
    Inline {
        name: String,
        config: McpServerConfig,
    },
}

/// `utils.ts` `getMcpServerScopeFromToolName` — resolves the MCP config scope
/// for a tool name shaped `mcp__<serverName>__<toolName>`. Returns the scope
/// of the resolved server config, or `Some(ConfigScope::Hosted)` for hosted
/// servers (their normalised names start with `"hosted_"`), or `None` when
/// the tool is not an MCP tool / the server is unknown.
pub fn get_mcp_server_scope_from_tool_name(
    tool_name: &str,
    server_configs: &HashMap<String, ScopedMcpServerConfig>,
) -> Option<ConfigScope> {
    let probe = Tool {
        name: Some(tool_name.to_string()),
        is_mcp: None,
    };
    if !is_mcp_tool(&probe) {
        return None;
    }
    let info = mcp_info_from_string(tool_name)?;
    if let Some(scoped) = server_configs.get(&info.server_name) {
        return Some(scoped.scope);
    }
    if info.server_name.starts_with("hosted_") {
        return Some(ConfigScope::Hosted);
    }
    None
}

/// `utils.ts` `extractAgentMcpServers` — flattens agent-frontmatter MCP
/// declarations into a sorted list of `AgentMcpServerInfo` for display.
pub fn extract_agent_mcp_servers(agents: &[AgentDefinitionForMcp]) -> Vec<AgentMcpServerInfo> {
    // server name -> (config, list-of-source-agents)
    let mut server_map: HashMap<String, (McpServerConfig, Vec<String>)> = HashMap::new();
    for agent in agents {
        for spec in &agent.mcp_servers {
            let AgentMcpServerSpec::Inline { name, config } = spec else {
                continue;
            };
            server_map
                .entry(name.clone())
                .and_modify(|(_, sources)| {
                    if !sources.contains(&agent.agent_type) {
                        sources.push(agent.agent_type.clone());
                    }
                })
                .or_insert_with(|| (config.clone(), vec![agent.agent_type.clone()]));
        }
    }

    let mut result: Vec<AgentMcpServerInfo> = server_map
        .into_iter()
        .filter_map(|(name, (config, source_agents))| match config {
            McpServerConfig::Stdio { command, .. } => Some(AgentMcpServerInfo {
                name,
                source_agents,
                transport: "stdio".to_string(),
                command: Some(command),
                url: None,
                needs_auth: false,
            }),
            McpServerConfig::Sse { url, .. } => Some(AgentMcpServerInfo {
                name,
                source_agents,
                transport: "sse".to_string(),
                command: None,
                url: Some(url),
                needs_auth: true,
            }),
            McpServerConfig::Http { url, .. } => Some(AgentMcpServerInfo {
                name,
                source_agents,
                transport: "http".to_string(),
                command: None,
                url: Some(url),
                needs_auth: true,
            }),
            McpServerConfig::Ws { url, .. } => Some(AgentMcpServerInfo {
                name,
                source_agents,
                transport: "ws".to_string(),
                command: None,
                url: Some(url),
                needs_auth: false,
            }),
            // Skip unsupported transport types (sdk, hosted-proxy, sse-ide, ws-ide).
            _ => None,
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}
