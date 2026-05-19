//! Pure string utility functions for MCP tool/server name parsing.
//! This file has no heavy dependencies to keep it lightweight for
//! consumers that only need string parsing (e.g., permission validation).

use super::normalization::normalize_name_for_mcp;

/// Extracted MCP server information from a tool name string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpInfo {
    pub server_name: String,
    pub tool_name: Option<String>,
}

/// Extracts MCP server information from a tool name string.
///
/// Expected format: `"mcp__serverName__toolName"`
///
/// Known limitation: If a server name contains "__", parsing will be incorrect.
/// For example, "mcp__my__server__tool" would parse as server="my" and tool="server__tool"
/// instead of server="my__server" and tool="tool". This is rare in practice since server
/// names typically don't contain double underscores.
pub fn mcp_info_from_string(tool_string: &str) -> Option<McpInfo> {
    let parts: Vec<&str> = tool_string.split("__").collect();
    if parts.len() < 2 {
        return None;
    }
    let mcp_part = parts[0];
    let server_name = parts[1];

    if mcp_part != "mcp" || server_name.is_empty() {
        return None;
    }

    // Join all parts after server name to preserve double underscores in tool names
    let tool_name = if parts.len() > 2 {
        Some(parts[2..].join("__"))
    } else {
        None
    };

    Some(McpInfo {
        server_name: server_name.to_string(),
        tool_name,
    })
}

/// Generates the MCP tool/command name prefix for a given server.
pub fn get_mcp_prefix(server_name: &str) -> String {
    format!("mcp__{}__", normalize_name_for_mcp(server_name))
}

/// Builds a fully qualified MCP tool name from server and tool names.
/// Inverse of `mcp_info_from_string()`.
pub fn build_mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}{}",
        get_mcp_prefix(server_name),
        normalize_name_for_mcp(tool_name)
    )
}

/// Tool info for permission check.
pub struct ToolForPermissionCheck {
    pub name: String,
    pub mcp_info: Option<McpServerToolInfo>,
}

/// MCP server + tool info pair.
pub struct McpServerToolInfo {
    pub server_name: String,
    pub tool_name: String,
}

/// Returns the name to use for permission rule matching.
/// For MCP tools, uses the fully qualified `mcp__server__tool` name so that
/// deny rules targeting builtins (e.g., "Write") don't match unprefixed MCP
/// replacements that share the same display name. Falls back to `tool.name`.
pub fn get_tool_name_for_permission_check(tool: &ToolForPermissionCheck) -> String {
    match &tool.mcp_info {
        Some(info) => build_mcp_tool_name(&info.server_name, &info.tool_name),
        None => tool.name.clone(),
    }
}

/// Extracts the display name from an MCP tool/command name.
///
/// Removes the `mcp__<normalizedServerName>__` prefix from `full_name`.
pub fn get_mcp_display_name(full_name: &str, server_name: &str) -> String {
    let prefix = format!("mcp__{}__", normalize_name_for_mcp(server_name));
    full_name.replacen(&prefix, "", 1)
}

/// Extracts just the tool/command display name from a `user_facing_name`.
///
/// Removes server prefix (everything before " - ") and the "(MCP)" suffix.
pub fn extract_mcp_tool_display_name(user_facing_name: &str) -> String {
    // First, remove the (MCP) suffix if present
    let re_suffix = regex::Regex::new(r"\s*\(MCP\)\s*$").unwrap();
    let without_suffix = re_suffix.replace(user_facing_name, "").trim().to_string();

    // Then, remove the server prefix (everything before " - ")
    if let Some(dash_index) = without_suffix.find(" - ") {
        let display_name = without_suffix[dash_index + 3..].trim().to_string();
        return display_name;
    }

    // If no dash found, return the string without (MCP)
    without_suffix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_info_from_string() {
        let result = mcp_info_from_string("mcp__github__list_repos").unwrap();
        assert_eq!(result.server_name, "github");
        assert_eq!(result.tool_name, Some("list_repos".to_string()));
    }

    #[test]
    fn test_mcp_info_no_tool() {
        let result = mcp_info_from_string("mcp__github").unwrap();
        assert_eq!(result.server_name, "github");
        assert_eq!(result.tool_name, None);
    }

    #[test]
    fn test_mcp_info_invalid() {
        assert!(mcp_info_from_string("not_mcp__server__tool").is_none());
        assert!(mcp_info_from_string("mcp").is_none());
    }

    #[test]
    fn test_build_mcp_tool_name() {
        let name = build_mcp_tool_name("github", "list_repos");
        assert_eq!(name, "mcp__github__list_repos");
    }

    #[test]
    fn test_extract_display_name() {
        assert_eq!(
            extract_mcp_tool_display_name("github - Add comment to issue (MCP)"),
            "Add comment to issue"
        );
    }
}
