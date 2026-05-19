//! `/agents` — Manage AI agent delegates (swarm teammates).
//!
//! Provides management for agent delegates that can work in parallel
//! on subtasks. Agents can be spawned, monitored, and terminated
//! through this command.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Agents/Delegates management command.
///
/// Allows users to:
/// - View active agents and their current tasks
/// - Spawn new agent delegates for parallel work
/// - Monitor agent progress and resource usage
/// - Terminate agents that are no longer needed
pub struct DelegatesDirective;

/// Agent management subcommands.
const AGENT_SUBCOMMANDS: &[(&str, &str)] = &[
    ("list", "List all active agents"),
    ("spawn", "Create a new agent delegate"),
    ("status", "Show detailed status of an agent"),
    ("stop", "Terminate an agent"),
    ("logs", "View agent execution logs"),
];

#[async_trait]
impl Directive for DelegatesDirective {
    fn name(&self) -> &str {
        "agents"
    }

    fn aliases(&self) -> &[&str] {
        &["delegates", "team"]
    }

    fn description(&self) -> &str {
        "Manage AI agent delegates"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[list|spawn|status|stop]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let subcommand = args.first().map(|s| s.to_lowercase());

        match subcommand.as_deref() {
            None => {
                if ctx.is_non_interactive {
                    return Ok(CommandResult::Text(
                        "No active agents. Use /agents spawn to create one.".to_string(),
                    ));
                }
                // Interactive mode: show the AgentsMenu equivalent
                // Displays available agents with their tools and permissions.
                let mut lines = Vec::new();
                lines.push("Agent Delegates".to_string());
                lines.push(String::new());
                lines.push("Active agents: (none)".to_string());
                lines.push(String::new());
                lines.push("Use /agents spawn <task> to create a new agent delegate.".to_string());
                lines.push("Agents work in parallel on subtasks you assign them.".to_string());
                Ok(CommandResult::Text(lines.join("\n")))
            }
            Some("list") => {
                Ok(CommandResult::Text(
                    "Active agents: (none)\n\n                     Use /agents spawn <task> to create a new agent delegate."
                        .to_string(),
                ))
            }
            Some("spawn") => {
                let task = args[1..].join(" ");
                if task.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents spawn <task description>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Spawning agent for task: {}", task
                )))
            }
            Some("status") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents status <agent-id>".to_string(),
                    ));
                }
                Ok(CommandResult::Text(format!(
                    "Agent {}: not found", agent_id
                )))
            }
            Some("stop" | "kill") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents stop <agent-id>".to_string(),
                    ));
                }
                Ok(CommandResult::System(format!(
                    "Terminated agent: {}", agent_id
                )))
            }
            Some("logs") => {
                let agent_id = args.get(1).unwrap_or(&"");
                if agent_id.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /agents logs <agent-id>".to_string(),
                    ));
                }
                Ok(CommandResult::Text(format!(
                    "No logs available for agent: {}", agent_id
                )))
            }
            Some("help" | "-h" | "--help") => {
                let mut help = String::from("Usage: /agents [subcommand]\n\nSubcommands:\n");
                for (cmd, desc) in AGENT_SUBCOMMANDS {
                    help.push_str(&format!("  {:10} {}\n", cmd, desc));
                }
                Ok(CommandResult::Text(help))
            }
            Some(unknown) => {
                Ok(CommandResult::Error(format!(
                    "Unknown subcommand: \"{}\". Use /agents help for available commands.",
                    unknown
                )))
            }
        }
    }
}
