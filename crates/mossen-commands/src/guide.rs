//! `/help` — Show available commands and usage information.
//!
//! Displays the list of all available slash commands, their descriptions,
//! and usage hints. Can also show detailed help for a specific command.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Help/Guide command — displays command reference.
///
/// Without arguments: shows all available commands grouped by category.
/// With a command name: shows detailed help for that specific command.
pub struct GuideDirective;

#[async_trait]
impl Directive for GuideDirective {
    fn name(&self) -> &str {
        "help"
    }

    fn aliases(&self) -> &[&str] {
        &["?"]
    }

    fn description(&self) -> &str {
        "Show available commands and usage information"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[command-name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Specific command help
        if let Some(cmd_name) = args.first() {
            let name = cmd_name.trim_start_matches('/');
            return Ok(CommandResult::Text(format!(
                "Help for /{name}:\n\n                 Use /{name} to see its usage. Detailed per-command help                  will be available when the command registry is fully connected.",
            )));
        }

        // General help — list all commands
        let mut output = format!("{} — Available Commands\n", ctx.product_name);
        output.push_str(&"=".repeat(40));
        output.push_str("\n\n");
        output.push_str("Use /help <command> for detailed information about a command.\n\n");
        output.push_str("Session: /compact, /context, /exit, /clear, /export, /session\n");
        output.push_str("Code: /review, /commit, /ship, /branch, /plan, /project\n");
        output.push_str("Config: /config, /model, /effort, /color, /lang, /vim\n");
        output.push_str("System: /mcp, /plugin, /agents, /skills, /memory\n");
        output.push_str("Info: /status, /cost, /files, /diff, /version, /doctor\n");

        Ok(CommandResult::Text(output))
    }
}
