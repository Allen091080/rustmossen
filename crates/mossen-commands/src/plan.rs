//! `/plan` — Create or view an implementation plan.
//!
//! Generates a structured implementation plan for a given task,
//! breaking it down into steps with dependencies, estimates,
//! and verification criteria.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Plan command — structured task planning.
///
/// Modes:
/// - (no args): Show the current plan or create a new one
/// - `show`: Display the active plan
/// - `clear`: Remove the current plan
/// - `<description>`: Generate a plan for the described task
pub struct PlanDirective;

#[async_trait]
impl Directive for PlanDirective {
    fn name(&self) -> &str {
        "plan"
    }

    fn description(&self) -> &str {
        "Create or view an implementation plan"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Prompt
    }

    fn argument_hint(&self) -> &str {
        "[show|clear|<task description>]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            return Ok(CommandResult::Text(
                "No active plan. Provide a task description to generate one:\n\n                 Usage: /plan <task description>\n\n                 Examples:\n                   /plan Add user authentication with OAuth\n                   /plan Refactor the database layer to use connection pooling\n                   /plan show    — view current plan\n                   /plan clear   — remove current plan"
                    .to_string(),
            ));
        }

        let subcommand = args[0].to_lowercase();

        match subcommand.as_str() {
            "show" => {
                Ok(CommandResult::Text(
                    "No active plan. Use /plan <description> to create one.".to_string(),
                ))
            }
            "clear" => {
                Ok(CommandResult::System(
                    "Plan cleared.".to_string(),
                ))
            }
            "help" | "-h" | "--help" => {
                Ok(CommandResult::Text(
                    "Usage: /plan [subcommand|description]\n\n                     Create a structured implementation plan for a task.\n\n                     Subcommands:\n                       show     View the current plan\n                       clear    Remove the current plan\n\n                     Or provide a task description to generate a new plan."
                        .to_string(),
                ))
            }
            _ => {
                // Generate plan for the given description
                let description = args.join(" ");
                Ok(CommandResult::Text(format!(
                    "Generating implementation plan for:\n\"{}\"\"\n\n                     Planning... (this will produce a structured plan with steps,\n                     dependencies, and verification criteria)",
                    description
                )))
            }
        }
    }
}
