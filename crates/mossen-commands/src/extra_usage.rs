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

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
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

        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(
                "Extra Usage Allowance\n                 =====================\n\n                 Base plan: Standard\n                 Add-ons: None\n                 Boosts: None\n                 Total capacity: Default limits"
                    .to_string(),
            ));
        }

        // Open the usage controls page in the browser
        let product_name = &ctx.product_name;
        let backend_name = if ctx.is_custom_backend {
            "Custom backend"
        } else {
            "Mossen backend"
        };
        let url = "https://mossen.ai/usage";
        Ok(CommandResult::Text(format!(
            "Open usage controls for {}: {}",
            backend_name, url
        )))
    }
}
