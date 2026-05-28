//! `/logout` — Report local credential logout status.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Logout command metadata.
///
/// Real logout needs a writable credential store. The generic command runner
/// only sees an environment snapshot.
pub struct DeauthDirective;

#[async_trait]
impl Directive for DeauthDirective {
    fn name(&self) -> &str {
        "logout"
    }

    fn aliases(&self) -> &[&str] {
        &["signout"]
    }

    fn description(&self) -> &str {
        "Report local credential logout status"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_ACCOUNT_FLOW")
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check if actually logged in
        if !ctx.env_vars.contains_key("MOSSEN_AUTH_TOKEN") {
            return Ok(CommandResult::System(
                "Not currently logged in.".to_string(),
            ));
        }

        if args
            .first()
            .map(|arg| matches!(*arg, "help" | "-h" | "--help" | "status"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Logout status\nA token is present in the environment snapshot, but no credential store is attached to this command runner."
                    .to_string(),
            ));
        }

        Ok(CommandResult::Error(
            "Cannot log out from this command runner. A token is present in the environment snapshot, but no writable credential store is attached, so no credentials were cleared."
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
        let mut env_vars = HashMap::new();
        env_vars.insert("MOSSEN_AUTH_TOKEN".to_string(), "token".to_string());
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
    fn logout_does_not_claim_credentials_cleared_without_store() {
        let output = tokio_test::block_on(DeauthDirective.execute(&["--force"], &test_context()))
            .expect("logout command");

        let CommandResult::Error(text) = output else {
            panic!("logout should fail closed without credential store");
        };
        assert!(text.contains("Cannot log out"), "{text}");
        assert!(!text.contains("Logged out successfully"), "{text}");
    }
}
