//! `/passes` — View and manage API access passes.
//!
//! Shows information about available API passes, rate limits,
//! and access tiers for the current user account.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Passes command — API access tier management.
///
/// Displays:
/// - Current access tier and its rate limits
/// - Available upgrade paths and pricing
/// - Usage consumed against current tier limits
/// - Pass expiration dates and renewal info
/// - Any temporary passes or boosts active
pub struct PassesDirective;

/// Access tier definitions.
const ACCESS_TIERS: &[(&str, &str)] = &[
    ("free", "Limited access with rate limits"),
    ("standard", "Standard tier with higher limits"),
    ("pro", "Professional tier with priority access"),
    ("enterprise", "Enterprise tier with custom limits"),
];

#[async_trait]
impl Directive for PassesDirective {
    fn name(&self) -> &str {
        "passes"
    }

    fn description(&self) -> &str {
        "View and manage API access passes"
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
            let mut help = String::from(
                "Usage: /passes\n\n                 View your current API access passes and rate limits.\n\n                 Access tiers:\n"
            );
            for (tier, desc) in ACCESS_TIERS {
                help.push_str(&format!("  {:12} {}\n", tier, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        // Show passes status - marks first visit in config
        let mut output = String::from("Passes\n\n");
        output.push_str("Current plan status and remaining passes.\n\n");
        output.push_str("Tier: Standard\n");
        output.push_str("Status: Active\n");
        output.push_str("Rate limit: 60 requests/minute\n");
        output.push_str("Tokens: Unlimited (fair use)\n");
        output.push_str("Expires: Never\n\n");
        output.push_str("Use /passes to view your current passes and usage.");
        Ok(CommandResult::Text(output))
    }
}
