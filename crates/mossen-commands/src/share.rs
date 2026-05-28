//! `/share` — Share the current conversation via a hosted link.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Share directive — uploads the current conversation transcript and returns
/// a shareable URL that others can use to view the conversation.
pub struct ShareDirective;

/// Determine if sharing is at least configured in the current context.
fn is_sharing_available(ctx: &CommandContext) -> bool {
    !ctx.is_non_interactive
        && ctx
            .env_vars
            .get("MOSSEN_DISABLE_SHARING")
            .map(|v| !matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(true)
}

/// Get the base URL for share links.
fn get_share_base_url(ctx: &CommandContext) -> String {
    ctx.env_vars
        .get("MOSSEN_SHARE_BASE_URL")
        .cloned()
        .unwrap_or_else(|| "https://share.mossen.dev".to_string())
}

#[async_trait]
impl Directive for ShareDirective {
    fn name(&self) -> &str {
        "share"
    }

    fn description(&self) -> &str {
        "Share the current conversation via link"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.can_use_hosted_platform_features() && !ctx.is_env_truthy("MOSSEN_DISABLE_SHARING")
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check if sharing is available
        if !is_sharing_available(ctx) {
            return Ok(CommandResult::Error(
                "Sharing is not available in the current mode.".to_string(),
            ));
        }

        let base_url = get_share_base_url(ctx);

        Ok(CommandResult::Error(format!(
            "Cannot create share link via {} from this command runner. No transcript serializer, uploader, or clipboard writer is attached, so nothing was shared.",
            base_url
        )))
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
            "MOSSEN_SHARE_BASE_URL".to_string(),
            "https://share.example".to_string(),
        );
        CommandContext {
            cwd: PathBuf::from("."),
            is_non_interactive: false,
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
    fn share_does_not_claim_link_creation_without_uploader() {
        let output = tokio_test::block_on(ShareDirective.execute(&[], &test_context()))
            .expect("share command");

        let CommandResult::Error(text) = output else {
            panic!("share should fail closed without uploader");
        };
        assert!(text.contains("Cannot create share link"), "{text}");
        assert!(!text.contains("will be copied"), "{text}");
    }
}
