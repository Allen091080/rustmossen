//! `/rate-limit-options` — View and configure rate limit settings.
//!
//! Shows current rate limit status and allows configuration of
//! retry behavior, backoff strategies, and limit notifications.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Rate limit options command — rate limiting configuration.
///
/// Displays:
/// - Current rate limit status (requests remaining)
/// - Time until limit reset
/// - Retry policy configuration
/// - Backoff strategy settings (exponential, linear, fixed)
/// - Notification preferences for approaching limits
pub struct RateLimitDirective;

/// Backoff strategy options.
const BACKOFF_STRATEGIES: &[(&str, &str)] = &[
    ("exponential", "Double wait time between retries (default)"),
    ("linear", "Fixed increment between retries"),
    ("fixed", "Same wait time between all retries"),
];

#[async_trait]
impl Directive for RateLimitDirective {
    fn name(&self) -> &str {
        "rate-limit-options"
    }

    fn description(&self) -> &str {
        "View and configure rate limit settings"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            let mut help = String::from(
                "Usage: /rate-limit-options [setting] [value]\n\n                 View and configure how rate limits are handled.\n\n                 Settings:\n                   backoff <strategy>    Set backoff strategy\n                   threshold <percent>   Set notification threshold\n                   auto-retry <on|off>   Toggle auto-retry\n\n                 Backoff strategies:\n",
            );
            for (name, desc) in BACKOFF_STRATEGIES {
                help.push_str(&format!("  {:14} {}\n", name, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(
                "Rate Limit Status\n\
                 =================\n\n\
                 Requests remaining: unlimited\n\
                 Reset: N/A\n\
                 Retry policy: auto-retry with exponential backoff\n\
                 Notification: warn at 80% usage"
                    .to_string(),
            ));
        }

        // Show rate limit options menu
        Ok(CommandResult::Text(
            "Rate Limit Options\n\
             ==================\n\n\
             What do you want to do?\n\n\
             1. Buy extra usage\n\
             2. Upgrade your plan\n\
             3. Stop and wait for limit to reset"
                .to_string(),
        ))
    }
}
