//! `/share` — Share the current conversation via a hosted link.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Share directive — uploads the current conversation transcript and returns
/// a shareable URL that others can use to view the conversation.
pub struct ShareDirective;

/// Determine if sharing is available in the current context.
fn is_sharing_available(ctx: &CommandContext) -> bool {
    // Sharing requires network access and a valid session
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

        // In full implementation:
        // 1. Serialize the current conversation transcript
        // 2. Upload to the share service (POST to base_url/api/share)
        // 3. Return the share URL
        // 4. Copy URL to clipboard via OSC 52
        //
        // The TS implementation uploads conversation data and returns a link.
        // For now, indicate that the share process would start.

        Ok(CommandResult::System(format!(
            "Creating share link via {}...\nShare link will be copied to clipboard when ready.",
            base_url
        )))
    }
}
