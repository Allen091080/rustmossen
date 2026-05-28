//! `/login` — Show backend credential setup.
//!
//! Translates `commands/login/login.tsx` (36 lines).
//! Shows auth status and instructions for configuring backend credentials.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Get the message shown when no interactive account flow is attached.
fn get_backend_setup_message(product_name: &str) -> String {
    format!(
        "{} personal edition does not use a built-in account flow. \
         Configure backend credentials with MOSSEN_CODE_CUSTOM_BASE_URL plus \
         MOSSEN_CODE_CUSTOM_API_KEY or MOSSEN_CODE_CUSTOM_AUTH_TOKEN.",
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
        "Show backend credential setup"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;
        Ok(CommandResult::System(get_backend_setup_message(
            product_name,
        )))
    }
}
