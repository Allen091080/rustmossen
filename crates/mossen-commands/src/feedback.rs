//! `/feedback` — Submit feedback about the session or product.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Feedback directive — opens the feedback form or submits inline feedback.
/// Requires configured feedback URLs to be available.
pub struct FeedbackDirective;

/// Check if feedback endpoints are configured.
fn has_configured_feedback_urls(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .contains_key("MOSSEN_CODE_PLATFORM_FEEDBACK_URL")
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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        has_configured_feedback_urls(ctx)
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
            let target = ctx
                .env_vars
                .get("MOSSEN_CODE_PLATFORM_FEEDBACK_URL")
                .or_else(|| ctx.env_vars.get("MOSSEN_CODE_PLATFORM_ISSUES_URL"))
                .cloned()
                .unwrap_or_else(|| "configured feedback endpoint".to_string());
            Ok(CommandResult::Error(format!(
                "Cannot submit feedback from this command runner. Open {} and include: {}",
                target, initial_description
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_context() -> CommandContext {
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "MOSSEN_CODE_PLATFORM_FEEDBACK_URL".to_string(),
            "https://feedback.example/form".to_string(),
        );
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: true,
            is_remote_mode: false,
            is_custom_backend: false,
            user_type: None,
            env_vars,
            product_name: "Mossen".to_string(),
            cli_name: "mossen".to_string(),
            version: "test".to_string(),
            build_time: None,
            cost_snapshot: Default::default(),
        }
    }

    #[test]
    fn feedback_directive_does_not_claim_inline_submission() {
        let output = tokio_test::block_on(FeedbackDirective.execute(&["broken"], &test_context()))
            .expect("feedback command");

        let CommandResult::Error(text) = output else {
            panic!("feedback should fail closed without a submitter");
        };
        assert!(text.contains("Cannot submit feedback"), "{text}");
        assert!(!text.contains("Feedback submitted"), "{text}");
    }
}
