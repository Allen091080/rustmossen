//! `/cost` — Show token usage and estimated cost for this session.
//!
//! Displays a breakdown of token consumption, API calls, and
//! estimated monetary cost for the current session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Cost/Meter command — session resource usage tracking.
///
/// Shows:
/// - Total tokens consumed (input + output)
/// - Number of API calls made
/// - Estimated cost based on model pricing
/// - Cost per message breakdown
pub struct MeterDirective;

#[async_trait]
impl Directive for MeterDirective {
    fn name(&self) -> &str {
        "cost"
    }

    fn description(&self) -> &str {
        "Show token usage and estimated cost for this session"
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

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /cost [options]\n\n                 Show token usage and cost for this session.\n\n                 Options:\n                   --detailed    Show per-message breakdown\n                   --total       Show only session totals"
                    .to_string(),
            ));
        }

        // In full implementation: query session metrics
        let mut output = String::from("Session Cost Summary\n");
        output.push_str("====================\n\n");
        output.push_str("Input tokens:  0\n");
        output.push_str("Output tokens: 0\n");
        output.push_str("Total tokens:  0\n");
        output.push_str("API calls:     0\n");
        output.push_str("Est. cost:     $0.00\n");

        Ok(CommandResult::Text(output))
    }
}
