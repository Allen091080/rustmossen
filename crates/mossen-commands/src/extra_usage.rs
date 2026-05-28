//! `/extra-usage` — View and manage extended usage allowance.
//!
//! Shows information about additional token/request capacity beyond
//! the base plan, including purchased add-ons and temporary boosts.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Extra usage command — displays extended capacity information.
///
/// Shows:
/// - Base plan token allowance
/// - Additional capacity from purchased add-ons
/// - Temporary usage boosts (promotional or gifted)
/// - Current consumption against total capacity
/// - Expiration dates for time-limited allowances
pub struct ExtraUsageDirective;

/// Types of extra usage sources.
const USAGE_SOURCES: &[&str] = &["base", "add-on", "boost", "promotional"];

#[async_trait]
impl Directive for ExtraUsageDirective {
    fn name(&self) -> &str {
        "extra-usage"
    }

    fn description(&self) -> &str {
        "View extended usage allowance and add-ons"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_hidden(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            let sources = USAGE_SOURCES.join(", ");
            return Ok(CommandResult::Text(format!(
                "Usage: /extra-usage\n\n                 View your extended usage allowance, including:\n                 - Additional token capacity from your plan\n                 - Temporary usage boosts\n                 - Rate limit headroom\n\n                 Sources: {}",
                sources
            )));
        }

        Ok(CommandResult::Text(
            "Extra Usage Allowance\n\
             =====================\n\n\
             No extra usage allowance snapshot is available in this local build.\n\
             Use /usage for local session usage recorded by the current run."
                .to_string(),
        ))
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
    fn extra_usage_does_not_print_fake_plan_capacity() {
        let output = tokio_test::block_on(ExtraUsageDirective.execute(&[], &test_context()))
            .expect("extra usage command");
        let CommandResult::Text(text) = output else {
            panic!("extra usage should return text");
        };

        assert!(text.contains("No extra usage allowance snapshot"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("hosted"), "{text}");
        assert!(!text.contains("Base plan: Standard"), "{text}");
        assert!(!text.contains("Total capacity: Default limits"), "{text}");
    }
}
