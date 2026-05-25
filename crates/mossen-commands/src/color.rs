//! `/color` — Set the prompt bar color for this session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Color command — sets or resets the session prompt bar color.
/// Teammates cannot change their own color (assigned by team leader).
pub struct ColorDirective;

/// Available agent colors for the prompt bar.
const AGENT_COLORS: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink", "magenta",
];

/// Aliases that reset to the default (gray) color.
const RESET_ALIASES: &[&str] = &["default", "reset", "none", "gray", "grey"];

/// Check if the current session is a teammate (swarm child).
fn is_teammate(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .get("MOSSEN_TEAMMATE")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[async_trait]
impl Directive for ColorDirective {
    fn name(&self) -> &str {
        "color"
    }

    fn description(&self) -> &str {
        "Set the prompt bar color for this session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn argument_hint(&self) -> &str {
        "<color|default>"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Teammates cannot set their own color
        if is_teammate(ctx) {
            return Ok(CommandResult::System(
                "Cannot set color: This session is a swarm teammate. Teammate colors are assigned by the team leader.".to_string(),
            ));
        }

        // No argument: show available colors
        if args.is_empty() {
            let color_list = AGENT_COLORS.join(", ");
            return Ok(CommandResult::System(format!(
                "Please provide a color. Available colors: {}, default",
                color_list
            )));
        }

        let color_arg = args[0].trim().to_lowercase();

        // Handle reset to default
        if RESET_ALIASES.contains(&color_arg.as_str()) {
            // In full implementation: saveAgentColor(sessionId, "default", fullPath)
            // and update app state to remove color
            return Ok(CommandResult::System(
                "Session color reset to default".to_string(),
            ));
        }

        // Validate the color
        if !AGENT_COLORS.contains(&color_arg.as_str()) {
            let color_list = AGENT_COLORS.join(", ");
            return Ok(CommandResult::Error(format!(
                "Invalid color \"{}\". Available colors: {}, default",
                color_arg, color_list
            )));
        }

        // In full implementation:
        // 1. Get sessionId and transcript path
        // 2. saveAgentColor(sessionId, colorArg, fullPath)
        // 3. Update AppState standaloneAgentContext.color

        Ok(CommandResult::System(format!(
            "Session color set to: {}",
            color_arg
        )))
    }
}
