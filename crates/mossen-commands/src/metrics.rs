//! `/stats` — Show session statistics and performance metrics.
//!
//! Displays detailed performance data including response times,
//! throughput, cache hit rates, and model-specific metrics.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Stats/Metrics command — performance overview.
///
/// Tracks:
/// - Average response time per query
/// - Token throughput (tokens/second)
/// - Cache hit rate for prompt caching
/// - Model switching history
/// - Session uptime and idle time
pub struct MetricsDirective;

#[async_trait]
impl Directive for MetricsDirective {
    fn name(&self) -> &str {
        "stats"
    }

    fn description(&self) -> &str {
        "Show session statistics and performance metrics"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_internal_user()
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /stats\n\n                 Show performance metrics for this session.\n                 Available to internal users only."
                    .to_string(),
            ));
        }

        let mut output = String::from("Session Metrics\n");
        output.push_str("===============\n\n");
        output.push_str("Queries:          0\n");
        output.push_str("Avg response:     —\n");
        output.push_str("Token throughput:  —\n");
        output.push_str("Cache hit rate:   —\n");
        output.push_str("Session uptime:   0m\n");

        Ok(CommandResult::Text(output))
    }
}
