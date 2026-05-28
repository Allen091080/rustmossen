//! `/context` — Show context window usage and token breakdown.
//!
//! Displays detailed information about how the model's context window
//! is being utilized, including token counts by category, memory files,
//! MCP tools, custom agents, and skills.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Context usage command.
///
/// The generic command runner can report configured model/window metadata. Live
/// prompt token breakdown requires the TUI/structured runtime snapshot.
pub struct EnvironDirective;

#[async_trait]
impl Directive for EnvironDirective {
    fn name(&self) -> &str {
        "context"
    }

    fn description(&self) -> &str {
        "Show context window usage"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Handle help
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /context\n\n                 Shows how the model's context window is being used.\n                 Includes token counts by category, memory usage,\n                 MCP tools, and auto-compact status."
                    .to_string(),
            ));
        }

        let model = ctx
            .env_vars
            .get("MOSSEN_MODEL")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let context_window = mossen_utils::context::terminal_context_window_tokens(&model)
            .unwrap_or(mossen_utils::context::MODEL_CONTEXT_WINDOW_DEFAULT);

        let mut output = String::from("## Context Usage\n\n");
        output.push_str(&format!("**Model:** {}\n", model));
        output.push_str(&format!(
            "**Configured context window:** {} tokens\n",
            context_window
        ));
        output.push_str("**Live token usage:** not attached to this command runner\n");
        output.push_str("**Auto-compact state:** not attached to this command runner\n");
        output.push_str("**Recent compact:** not attached to this command runner\n");

        Ok(CommandResult::Text(output))
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
    fn context_directive_does_not_claim_live_token_or_compact_state() {
        let output = tokio_test::block_on(EnvironDirective.execute(&[], &test_context()))
            .expect("context command");

        let CommandResult::Text(text) = output else {
            panic!("context should return text");
        };
        assert!(text.contains("Live token usage:** not attached"), "{text}");
        assert!(!text.contains("**Auto-compact:** Enabled"), "{text}");
        assert!(
            !text.contains("No compact boundary in this session"),
            "{text}"
        );
    }
}
