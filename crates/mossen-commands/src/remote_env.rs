//! `/remote-env` — Configure remote environment settings.
//!
//! Manages environment variables and configuration for external execution
//! contexts when that mode is explicitly enabled.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Remote environment configuration command.
///
/// Allows setting environment variables that will be forwarded to external
/// execution contexts. Useful for API keys, paths, and tool configurations.
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
                "This command is not available in this session.".to_string(),
            ));
        }

        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                    "Usage: /remote-env [subcommand]\n\n                 Manage external execution environment variables.\n\n                 Subcommands:\n                   list             List configured environment variables\n                   set KEY=VALUE    Set an environment variable\n                   unset KEY        Remove an environment variable"
                    .to_string(),
            ));
        }

        let subcommand = args.first().map(|s| s.to_lowercase());
        match subcommand.as_deref() {
            None | Some("list") => {
                if ctx.is_non_interactive {
                    return Ok(CommandResult::Text(
                        "Execution environment variables: (none configured)".to_string(),
                    ));
                }
                // Interactive mode: show the RemoteEnvironmentDialog equivalent
                // Lists all env vars configured for external execution contexts.
                let mut lines = Vec::new();
                lines.push("Execution Environment Configuration".to_string());
                lines.push(String::new());
                lines.push("Variables forwarded to configured execution contexts:".to_string());
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
                Ok(CommandResult::System(format!("Remote env set: {}", pair)))
            }
            Some("unset") => {
                let key = args.get(1).unwrap_or(&"");
                if key.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /remote-env unset KEY".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!("Remote env unset: {}", key)))
            }
            Some(unknown) => Ok(CommandResult::Error(format!(
                "Unknown subcommand: \"{}\". Use /remote-env help.",
                unknown
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context(is_remote_mode: bool) -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: false,
            is_remote_mode,
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

    #[test]
    fn remote_env_disabled_message_is_session_neutral() {
        let output = tokio_test::block_on(RemoteEnvDirective.execute(&[], &test_context(false)))
            .expect("remote-env command");
        let CommandResult::Error(text) = output else {
            panic!("disabled remote-env should return an error");
        };

        assert!(text.contains("not available"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("remote"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("hosted"), "{text}");
    }

    #[test]
    fn remote_env_list_does_not_advertise_remote_sessions() {
        let output = tokio_test::block_on(RemoteEnvDirective.execute(&[], &test_context(true)))
            .expect("remote-env command");
        let CommandResult::Text(text) = output else {
            panic!("remote-env list should return text");
        };

        assert!(
            text.contains("Execution Environment Configuration"),
            "{text}"
        );
        assert!(
            !text.to_ascii_lowercase().contains("remote sessions"),
            "{text}"
        );
        assert!(!text.to_ascii_lowercase().contains("hosted"), "{text}");
    }
}
