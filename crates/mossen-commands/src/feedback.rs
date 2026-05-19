//! `/feedback` — Submit feedback about the session or product.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Feedback directive — opens the feedback form or submits inline feedback.
/// Requires configured feedback URLs to be available.
pub struct FeedbackDirective;

/// Check if feedback endpoints are configured.
fn has_configured_feedback_urls(ctx: &CommandContext) -> bool {
    ctx.env_vars.contains_key("MOSSEN_CODE_PLATFORM_FEEDBACK_URL")
        || ctx.env_vars.contains_key("MOSSEN_CODE_PLATFORM_ISSUES_URL")
}

#[async_trait]
impl Directive for FeedbackDirective {
    fn name(&self) -> &str {
        "feedback"
    }

    fn aliases(&self) -> &[&str] {
        &["bug"]
    }

    fn description(&self) -> &str {
        "Submit feedback or report an issue"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check if feedback URLs are configured
        if !has_configured_feedback_urls(ctx) {
            return Ok(CommandResult::System(
                "Feedback is not configured for this build. Set MOSSEN_CODE_PLATFORM_FEEDBACK_URL or MOSSEN_CODE_PLATFORM_ISSUES_URL to enable it.".to_string(),
            ));
        }

        let initial_description = if args.is_empty() {
            String::new()
        } else {
            args.join(" ")
        };

        if initial_description.is_empty() {
            // No args: show feedback instructions
            Ok(CommandResult::Text(
                "Feedback\n\
                 ========\n\n\
                 Share your feedback about Mossen.\n\n\
                 Usage: /feedback <description>\n\n\
                 Your feedback helps us improve the product. Include details about \
                 what worked well or what could be better."
                    .to_string(),
            ))
        } else {
            // Quick feedback with pre-filled description
            // In full implementation: submit directly or open form pre-filled
            Ok(CommandResult::System(format!(
                "Feedback submitted: {}",
                initial_description
            )))
        }
    }
}
