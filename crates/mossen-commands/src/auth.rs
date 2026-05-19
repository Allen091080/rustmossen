//! `/login` — Authenticate with the platform.
//!
//! Translates `commands/login/login.tsx` (36 lines).
//! Shows auth status and instructions for configuring backend credentials.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Get the message shown when hosted auth is disabled.
fn get_hosted_auth_disabled_message(product_name: &str) -> String {
    format!(
        "{} does not use a built-in account flow on this branch. \
         Configure backend credentials with MOSSEN_CODE_CUSTOM_BASE_URL plus \
         MOSSEN_CODE_CUSTOM_API_KEY or MOSSEN_CODE_CUSTOM_AUTH_TOKEN. \
         If you intentionally wrap an external hosted service, \
         enable that Mossen adapter explicitly with \
         MOSSEN_CODE_ENABLE_HOSTED_AUTH_ADAPTER=1 and inject its credentials there.",
        product_name
    )
}

/// `/login` command.
pub struct AuthDirective;

#[async_trait]
impl Directive for AuthDirective {
    fn name(&self) -> &str {
        "login"
    }

    fn description(&self) -> &str {
        "Authenticate with the platform"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;
        Ok(CommandResult::System(get_hosted_auth_disabled_message(product_name)))
    }
}
