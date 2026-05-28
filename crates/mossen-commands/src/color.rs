//! `/color` — Set the prompt bar color for this session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Color command — sets or resets the session prompt bar color.
pub struct ColorDirective;

/// Available agent colors for the prompt bar.
const AGENT_COLORS: &[&str] = &[
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink", "magenta",
];

/// Aliases that reset to the default (gray) color.
const RESET_ALIASES: &[&str] = &["default", "reset", "none", "gray", "grey"];

/// Check if the current session is a delegated child session.
fn is_delegated_session(ctx: &CommandContext) -> bool {
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
        if is_delegated_session(ctx) {
            return Ok(CommandResult::System(
                "Cannot set color for this child agent session.".to_string(),
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
            return Ok(CommandResult::Error(
                "Session color customization is not wired to live TUI state in this build."
                    .to_string(),
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

        Ok(CommandResult::Error(format!(
            "Cannot set session color to {color_arg}: color customization is not wired to live TUI state in this build."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: true,
            is_remote_mode: false,
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
    fn color_directive_does_not_claim_live_state_changes() {
        let output = tokio_test::block_on(ColorDirective.execute(&["blue"], &test_context()))
            .expect("color command");

        let CommandResult::Error(text) = output else {
            panic!("color should not claim success without live TUI state");
        };
        assert!(text.contains("Cannot set session color"), "{text}");
        assert!(!text.contains("Session color set"), "{text}");
    }

    #[test]
    fn color_reset_does_not_claim_live_state_changes() {
        let output = tokio_test::block_on(ColorDirective.execute(&["reset"], &test_context()))
            .expect("color command");

        let CommandResult::Error(text) = output else {
            panic!("color reset should not claim success without live TUI state");
        };
        assert!(text.contains("not wired to live TUI state"), "{text}");
        assert!(!text.contains("reset to default"), "{text}");
    }
}
