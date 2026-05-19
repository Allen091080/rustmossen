//! ListMcpResourcesTool prompt.
pub const LIST_MCP_RESOURCES_TOOL_NAME: &str = "ListMcpResourcesTool";

pub const DESCRIPTION: &str = r#"
Lists available resources from configured MCP servers.
Each resource object includes a 'server' field indicating which server it's from.

Usage examples:
- List all resources from all servers: `listMcpResources`
- List resources from a specific server: `listMcpResources({ server: "myserver" })`
"#;

pub const PROMPT: &str = r#"
List available resources from configured MCP servers.
Each returned resource will include all standard MCP resource fields plus a 'server' field
indicating which server the resource belongs to.

Parameters:
- server (optional): The name of a specific MCP server to get resources from. If not provided,
  resources from all servers will be returned.
"#;
