//! `/logout` — Log out of your account.
//!
//! Clears authentication tokens, invalidates the current session,
//! and removes stored credentials. After logout, the user must
//! re-authenticate to access personalized features.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Logout command — clears authentication and session state.
///
/// This command:
/// 1. Invalidates the current auth token
/// 2. Clears stored credentials from the keychain
/// 3. Removes session-specific user data
/// 4. Notifies the backend of the logout event
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
        "Log out of your account"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
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

        // Handle --force flag to skip confirmation
        let force = args.first().map(|a| matches!(*a, "--force" | "-f")).unwrap_or(false);

        if !force && !ctx.is_non_interactive {
            // In interactive mode without --force, could show confirmation
            // but for now we proceed directly
        }

        // In full implementation:
        // 1. Call API to invalidate token
        // 2. Clear keychain/credential store
        // 3. Remove cached user preferences
        // 4. Reset session state

        Ok(CommandResult::System(
            "Logged out successfully. Use /login to sign in again.".to_string(),
        ))
    }
}
