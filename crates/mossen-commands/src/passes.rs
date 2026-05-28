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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_hosted_platform_features() || ctx.is_env_truthy("MOSSEN_ENABLE_PASSES_COMMAND")
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
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

        let mut output = String::from("Passes\n\n");
        output.push_str("Pass status is not available from this local personal build.\n");
        output.push_str(
            "No plan, tier, or token limit is shown unless an account service provides it.\n",
        );
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
    fn passes_does_not_print_fake_hosted_plan() {
        let output = match tokio_test::block_on(PassesDirective.execute(&[], &test_context()))
            .expect("passes command")
        {
            CommandResult::Text(text) => text,
            other => panic!("unexpected result: {other:?}"),
        };

        assert!(output.contains("not available"), "{output}");
        assert!(!output.to_ascii_lowercase().contains("hosted"), "{output}");
        assert!(!output.contains("Tier: Standard"), "{output}");
        assert!(!output.contains("Tokens: Unlimited"), "{output}");
    }
}
