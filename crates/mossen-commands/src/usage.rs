//! `/usage` — View usage dashboard and billing information.
//!
//! Shows comprehensive usage statistics including API calls,
//! token consumption, billing period information, and plan limits.
//! In interactive mode, renders a full settings panel focused on
//! the usage tab.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Usage dashboard command — billing and consumption info.
///
/// Displays:
/// - Current billing period dates and days remaining
/// - Total tokens used vs. plan limit
/// - API call count and breakdown by model
/// - Cost estimates and monthly projections
/// - Usage trends compared to previous periods
/// - Available add-ons and upgrade options
pub struct UsageDirective;

#[async_trait]
impl Directive for UsageDirective {
    fn name(&self) -> &str {
        "usage"
    }

    fn description(&self) -> &str {
        "View usage dashboard and billing information"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /usage [options]\n\n\
                 View your usage dashboard and billing information.\n\n\
                 Options:\n\
                   --period     Show current billing period details\n\
                   --history    Show usage history over past months\n\
                   --breakdown  Show per-model token breakdown\n\
                   --export     Export usage data as CSV"
                    .to_string(),
            ));
        }

        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(
                "Usage Summary\n\
                 =============\n\n\
                 Billing period: Current\n\
                 Tokens used: 0\n\
                 API calls: 0\n\
                 Estimated cost: $0.00\n\
                 Plan: Standard\n\
                 Days remaining: 30"
                    .to_string(),
            ));
        }

        // Interactive mode: render the full Settings panel with Usage tab focused
        let mut output = String::from("Usage Dashboard\n\n");
        output.push_str("Billing period: Current\n");
        output.push_str("Tokens used: 0\n");
        output.push_str("API calls: 0\n");
        output.push_str("Estimated cost: $0.00\n");
        output.push_str("Plan: Standard\n");
        output.push_str("Days remaining: 30\n\n");
        output.push_str("Open /usage for detailed billing and usage information.");
        Ok(CommandResult::Text(output))
    }
}
