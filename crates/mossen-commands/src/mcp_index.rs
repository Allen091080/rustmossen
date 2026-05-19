//! MCP command module
//!
//! This module provides the MCP command definition.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// MCP command directive for managing MCP servers
pub struct McpDirective;

#[async_trait]
impl Directive for McpDirective {
    fn name(&self) -> &str {
        "mcp"
    }

    fn aliases(&self) -> &[&str] {
        &[]
    }

    fn description(&self) -> &str {
        "Manage MCP servers"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn argument_hint(&self) -> &str {
        "[status|templates|add-template|enable|disable [server-name]]"
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        // MCP command handling is delegated to bridges module
        Ok(CommandResult::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_directive_name() {
        let directive = McpDirective;
        assert_eq!(directive.name(), "mcp");
    }

    #[test]
    fn test_mcp_directive_description() {
        let directive = McpDirective;
        assert_eq!(directive.description(), "Manage MCP servers");
    }

    #[test]
    fn test_mcp_directive_immediate() {
        let directive = McpDirective;
        assert!(directive.is_immediate());
    }
}
