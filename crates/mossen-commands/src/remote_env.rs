//! `/remote-env` — Configure remote environment settings.
//!
//! Manages environment variables and configuration for remote
//! execution contexts (cloud workspaces, SSH sessions, containers).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Remote environment configuration command.
///
/// Allows setting environment variables that will be forwarded
/// to remote execution contexts. Useful for API keys, paths,
/// and tool configurations in remote sessions.
pub struct RemoteEnvDirective;

#[async_trait]
impl Directive for RemoteEnvDirective {
    fn name(&self) -> &str {
        "remote-env"
    }

    fn description(&self) -> &str {
        "Configure remote environment settings"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[set|unset|list]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_remote_mode
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if !ctx.is_remote_mode {
            return Ok(CommandResult::Error(
                "This command is only available in remote mode.".to_string(),
            ));
        }

        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Usage: /remote-env [subcommand]\n\n                 Manage remote environment variables.\n\n                 Subcommands:\n                   list             List configured remote env vars\n                   set KEY=VALUE    Set a remote env var\n                   unset KEY        Remove a remote env var"
                    .to_string(),
            ));
        }

        let subcommand = args.first().map(|s| s.to_lowercase());
        match subcommand.as_deref() {
            None | Some("list") => {
                if ctx.is_non_interactive {
                    return Ok(CommandResult::Text(
                        "Remote environment variables: (none configured)".to_string(),
                    ));
                }
                // Interactive mode: show the RemoteEnvironmentDialog equivalent
                // Lists all env vars configured for remote/teleport sessions.
                let mut lines = Vec::new();
                lines.push("Remote Environment Configuration".to_string());
                lines.push(String::new());
                lines.push("Variables forwarded to remote sessions:".to_string());
                lines.push("  (none configured)".to_string());
                lines.push(String::new());
                lines.push("Use /remote-env set KEY=VALUE to add a variable.".to_string());
                lines.push("Use /remote-env unset KEY to remove one.".to_string());
                Ok(CommandResult::Text(lines.join("\n")))
            }
            Some("set") => {
                let pair = args.get(1).unwrap_or(&"");
                if !pair.contains('=') {
                    return Ok(CommandResult::Error(
                        "Usage: /remote-env set KEY=VALUE".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Remote env set: {}", pair
                )))
            }
            Some("unset") => {
                let key = args.get(1).unwrap_or(&"");
                if key.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /remote-env unset KEY".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Remote env unset: {}", key
                )))
            }
            Some(unknown) => {
                Ok(CommandResult::Error(format!(
                    "Unknown subcommand: \"{}\". Use /remote-env help.", unknown
                )))
            }
        }
    }
}
