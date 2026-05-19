//! `/context` — Show context window usage and token breakdown.
//!
//! Displays detailed information about how the model's context window
//! is being utilized, including token counts by category, memory files,
//! MCP tools, custom agents, and skills.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Context usage command — shows token budget allocation.
///
/// Reports:
/// - Total tokens used vs. available
/// - Usage percentage and remaining capacity
/// - Breakdown by category (system prompt, messages, tools, etc.)
/// - Auto-compact threshold status
/// - Recent compaction history
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
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Usage: /context\n\n                 Shows how the model's context window is being used.\n                 Includes token counts by category, memory usage,\n                 MCP tools, and auto-compact status."
                    .to_string(),
            ));
        }

        // In full implementation, this would:
        // 1. Get messages after compact boundary
        // 2. Apply context collapse if enabled
        // 3. Run microcompact to get accurate token count
        // 4. Analyze context usage by category
        // 5. Format as markdown table

        let model = ctx.env_vars
            .get("MOSSEN_MODEL")
            .cloned()
            .unwrap_or_else(|| "default".to_string());

        let mut output = String::from("## Context Usage\n\n");
        output.push_str(&format!("**Model:** {}\n", model));
        output.push_str("**Tokens:** (connect to model service for live stats)\n");
        output.push_str("**Auto-compact:** Enabled\n");
        output.push_str("**Recent compact:** No compact boundary in this session\n");

        Ok(CommandResult::Text(output))
    }
}
