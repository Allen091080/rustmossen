//! `/brief` — Toggle brief-only mode (local-jsx command).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Brief directive — toggle brief-only mode for user-facing output.
pub struct BriefDirective;

/// Name of the brief tool used in system reminders.
const BRIEF_TOOL_NAME: &str = "SendUserMessage";

/// Check if brief mode feature is enabled via config/feature flags.
fn is_brief_feature_enabled(ctx: &CommandContext) -> bool {
    ctx.is_env_truthy("KAIROS")
        || ctx.is_env_truthy("KAIROS_BRIEF")
        || ctx.is_env_truthy("MOSSEN_BRIEF_ENABLED")
}

/// Check if user is entitled to brief mode.
fn is_brief_entitled(ctx: &CommandContext) -> bool {
    // Brief entitlement checked via feature flag or user type
    ctx.is_internal_user() || ctx.is_env_truthy("MOSSEN_BRIEF_ENTITLED")
}

/// Check if Kairos mode is active.
fn get_kairos_active(ctx: &CommandContext) -> bool {
    ctx.is_env_truthy("KAIROS_ACTIVE")
}

#[async_trait]
impl Directive for BriefDirective {
    fn name(&self) -> &str {
        "brief"
    }

    fn description(&self) -> &str {
        "Toggle brief-only mode"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        is_brief_feature_enabled(ctx)
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check current state from env (in real impl this would be app state)
        let current_is_brief = ctx.is_env_truthy("MOSSEN_BRIEF_ONLY");
        let new_state = !current_is_brief;

        // Entitlement check only gates the on-transition
        if new_state && !is_brief_entitled(ctx) {
            return Ok(CommandResult::System(
                "Brief tool is not enabled for your account".to_string(),
            ));
        }

        // Generate system reminder for the model about the mode change
        let _kairos_active = get_kairos_active(ctx);
        let meta_message = if !get_kairos_active(ctx) {
            if new_state {
                Some(format!(
                    "<system-reminder>\nBrief mode is now enabled. Use the {} tool for all user-facing output — plain text outside it is hidden from the user's view.\n</system-reminder>",
                    BRIEF_TOOL_NAME
                ))
            } else {
                Some(format!(
                    "<system-reminder>\nBrief mode is now disabled. The {} tool is no longer available — reply with plain text.\n</system-reminder>",
                    BRIEF_TOOL_NAME
                ))
            }
        } else {
            None
        };

        let message = if new_state {
            "Brief-only mode enabled"
        } else {
            "Brief-only mode disabled"
        };

        // In a real implementation, this would also:
        // - Call setUserMsgOptIn(newState)
        // - Update app state with isBriefOnly
        // - Log analytics event
        // - Attach metaMessages to the response

        let _ = meta_message; // Used in full implementation for response metadata
        Ok(CommandResult::System(message.to_string()))
    }
}
