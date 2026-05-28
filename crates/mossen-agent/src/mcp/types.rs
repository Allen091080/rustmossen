//! MCP service layer types.
//!
//! Configuration schemas and types for MCP servers, connections, and serialization.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ─── Configuration Scope ─────────────────────────────────────────────────────

/// MCP server configuration scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    Local,
    User,
    Project,
    Dynamic,
    Enterprise,
    Hosted,
    Managed,
}

/// Transport type for MCP connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Stdio,
    #[serde(rename = "sse")]
    Sse,
    #[serde(rename = "sse-ide")]
    SseIde,
    Http,
    Ws,
    #[serde(rename = "ws-ide")]
    WsIde,
    Sdk,
    #[serde(rename = "hosted-proxy")]
    HostedProxy,
}

/// TS `export type Transport = z.infer<…>` — narrower enum without the IDE /
/// proxy variants (mirrors the TS Zod schema).
pub type Transport = TransportType;

/// Validator for the TS `TransportSchema` Zod schema. Accepts one of the
/// strings: `stdio` | `sse` | `sse-ide` | `http` | `ws` | `sdk`.
pub struct TransportSchema;

impl TransportSchema {
    pub fn parse(value: &str) -> Result<Transport, String> {
        match value {
            "stdio" => Ok(TransportType::Stdio),
            "sse" => Ok(TransportType::Sse),
            "sse-ide" => Ok(TransportType::SseIde),
            "http" => Ok(TransportType::Http),
            "ws" => Ok(TransportType::Ws),
            "sdk" => Ok(TransportType::Sdk),
            other => Err(format!("invalid transport: {other}")),
        }
    }
}

// ─── OAuth Configuration ─────────────────────────────────────────────────────

/// MCP OAuth configuration for a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_server_metadata_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xaa: Option<bool>,
}

// ─── Server Config Variants ──────────────────────────────────────────────────

/// stdio transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStdioServerConfig {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// SSE transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSseServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<McpOAuthConfig>,
}

/// SSE-IDE transport configuration (IDE internal use).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSseIdeServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    pub ide_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide_running_in_windows: Option<bool>,
}

/// WebSocket-IDE transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpWebSocketIdeServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    pub ide_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ide_running_in_windows: Option<bool>,
}

/// HTTP (Streamable HTTP) transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpHttpServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<McpOAuthConfig>,
}

/// WebSocket transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpWebSocketServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<String>,
}

/// SDK transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSdkServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub name: String,
}

/// Hosted proxy server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpHostedProxyServerConfig {
    #[serde(rename = "type")]
    pub transport_type: String,
    pub url: String,
    pub id: String,
}

/// MCP server configuration union type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerConfig {
    #[serde(rename = "stdio")]
    Stdio {
        #[serde(default)]
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    #[serde(rename = "sse")]
    Sse {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "headersHelper")]
        headers_helper: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        oauth: Option<McpOAuthConfig>,
    },
    #[serde(rename = "sse-ide")]
    SseIde {
        url: String,
        #[serde(rename = "ideName")]
        ide_name: String,
        #[serde(
            skip_serializing_if = "Option::is_none",
            rename = "ideRunningInWindows"
        )]
        ide_running_in_windows: Option<bool>,
    },
    #[serde(rename = "ws-ide")]
    WsIde {
        url: String,
        #[serde(rename = "ideName")]
        ide_name: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "authToken")]
        auth_token: Option<String>,
        #[serde(
            skip_serializing_if = "Option::is_none",
            rename = "ideRunningInWindows"
        )]
        ide_running_in_windows: Option<bool>,
    },
    #[serde(rename = "http")]
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "headersHelper")]
        headers_helper: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        oauth: Option<McpOAuthConfig>,
    },
    #[serde(rename = "ws")]
    Ws {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "headersHelper")]
        headers_helper: Option<String>,
    },
    #[serde(rename = "sdk")]
    Sdk { name: String },
    #[serde(rename = "hosted-proxy")]
    HostedProxy { url: String, id: String },
}

/// Scoped MCP server config — config + scope + optional plugin source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedMcpServerConfig {
    #[serde(flatten)]
    pub config: McpServerConfig,
    pub scope: ConfigScope,
    /// For plugin-provided servers: the providing plugin's `LoadedPlugin.source`
    /// (e.g. 'slack@mossen').
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_source: Option<String>,
}

// ─── MCP JSON Config File ────────────────────────────────────────────────────

/// `.mcp.json` file format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpJsonConfig {
    #[serde(default)]
    pub mcp_servers: HashMap<String, Value>,
}

// ─── Server Connection Types ─────────────────────────────────────────────────

/// Server capabilities (stub — full definition in mossen-mcp crate).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Server info (name + version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Connected MCP server.
#[derive(Debug, Clone)]
pub struct ConnectedMcpServer {
    pub name: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Option<ServerInfo>,
    pub instructions: Option<String>,
    pub config: ScopedMcpServerConfig,
}

/// Failed MCP server.
#[derive(Debug, Clone)]
pub struct FailedMcpServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
    pub error: Option<String>,
}

/// MCP server that needs authentication.
#[derive(Debug, Clone)]
pub struct NeedsAuthMcpServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
}

/// Pending MCP server (connecting).
#[derive(Debug, Clone)]
pub struct PendingMcpServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
    pub reconnect_attempt: Option<u32>,
    pub max_reconnect_attempts: Option<u32>,
}

/// Disabled MCP server.
#[derive(Debug, Clone)]
pub struct DisabledMcpServer {
    pub name: String,
    pub config: ScopedMcpServerConfig,
}

/// MCP server connection state (discriminated union).
#[derive(Debug, Clone)]
pub enum McpServerConnection {
    Connected(ConnectedMcpServer),
    Failed(FailedMcpServer),
    NeedsAuth(NeedsAuthMcpServer),
    Pending(PendingMcpServer),
    Disabled(DisabledMcpServer),
}

impl McpServerConnection {
    /// Get the connection type as a string.
    pub fn connection_type(&self) -> &'static str {
        match self {
            Self::Connected(_) => "connected",
            Self::Failed(_) => "failed",
            Self::NeedsAuth(_) => "needs-auth",
            Self::Pending(_) => "pending",
            Self::Disabled(_) => "disabled",
        }
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        match self {
            Self::Connected(s) => &s.name,
            Self::Failed(s) => &s.name,
            Self::NeedsAuth(s) => &s.name,
            Self::Pending(s) => &s.name,
            Self::Disabled(s) => &s.name,
        }
    }

    /// Get the server config.
    pub fn config(&self) -> &ScopedMcpServerConfig {
        match self {
            Self::Connected(s) => &s.config,
            Self::Failed(s) => &s.config,
            Self::NeedsAuth(s) => &s.config,
            Self::Pending(s) => &s.config,
            Self::Disabled(s) => &s.config,
        }
    }
}

// ─── Resource Types ──────────────────────────────────────────────────────────

/// A resource with its owning server name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResource {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub server: String,
}

// ─── MCP CLI State Types ─────────────────────────────────────────────────────

/// Serialized tool information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerializedTool {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mcp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_tool_name: Option<String>,
}

/// Serialized client information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedClient {
    pub name: String,
    #[serde(rename = "type")]
    pub connection_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ServerCapabilities>,
}

/// MCP CLI state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCliState {
    pub clients: Vec<SerializedClient>,
    pub configs: HashMap<String, ScopedMcpServerConfig>,
    pub tools: Vec<SerializedTool>,
    pub resources: HashMap<String, Vec<ServerResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_names: Option<HashMap<String, String>>,
}
