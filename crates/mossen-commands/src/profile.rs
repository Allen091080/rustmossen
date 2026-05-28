//! `/profile` — Show local runtime profile status.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Profile directive — displays local user/runtime profile status.
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
        "Show local runtime profile status"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[status]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|arg| matches!(*arg, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /profile [status]\n\nShows local runtime identity and backend configuration. Use /model to list or switch model profiles."
                    .to_string(),
            ));
        }

        if args
            .first()
            .map(|arg| matches!(*arg, "status" | "current" | "show" | "summary"))
            .unwrap_or(args.is_empty())
        {
            let profile = get_profile_display(ctx);
            let backend = if ctx.is_custom_backend {
                "custom backend"
            } else {
                "not configured"
            };
            return Ok(CommandResult::Text(format!(
                "Local profile: {}\nBackend: {}\nCWD: {}\nModel profiles: use /model to list or switch.",
                profile,
                backend,
                ctx.cwd.display()
            )));
        }

        Ok(CommandResult::Error(
            "Unsupported /profile argument. Use /profile status for local runtime status, or /model <profile> to switch model profiles."
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
    fn profile_status_does_not_claim_hosted_or_switching() {
        let output = tokio_test::block_on(ProfileDirective.execute(&[], &test_context()))
            .expect("profile command");
        let CommandResult::Text(text) = output else {
            panic!("profile should return text");
        };
        assert!(text.contains("Backend: not configured"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("hosted"), "{text}");
        assert!(!text.contains("Use /profile <name> to switch"), "{text}");
        assert!(text.contains("use /model to list or switch"), "{text}");
    }
}
