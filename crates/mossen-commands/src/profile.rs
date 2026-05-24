//! `/profile` — Show or switch user profile.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Profile directive — displays the current user profile or switches between profiles.
pub struct ProfileDirective;

/// Get the current profile display name.
fn get_profile_display(ctx: &CommandContext) -> String {
    let user = ctx
        .env_vars
        .get("USER")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let user_type = ctx.user_type.as_deref().unwrap_or("standard");
    format!("{} ({})", user, user_type)
}

#[async_trait]
impl Directive for ProfileDirective {
    fn name(&self) -> &str {
        "profile"
    }

    fn description(&self) -> &str {
        "Show or switch user profile"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Show current profile info
            let profile = get_profile_display(ctx);
            let backend = if ctx.is_custom_backend {
                "custom backend"
            } else {
                "hosted"
            };
            return Ok(CommandResult::Text(format!(
                "Current profile: {}\nBackend: {}\nCWD: {}",
                profile,
                backend,
                ctx.cwd.display()
            )));
        }

        // Show profile selector information
        let profile = ctx
            .env_vars
            .get("MOSSEN_PROFILE")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let backend = if ctx.is_custom_backend {
            "custom"
        } else {
            "hosted"
        };
        Ok(CommandResult::Text(format!(
            "Profile: {}\n\
             Backend: {}\n\
             CWD: {}\n\n\
             Use /profile <name> to switch profiles.",
            profile,
            backend,
            ctx.cwd.display()
        )))
    }
}
