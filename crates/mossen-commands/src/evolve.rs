//! `/upgrade` — Check for and install product updates.
//!
//! Checks the update server for new versions, downloads the update,
//! and applies it. Supports both auto-update and manual upgrade flows.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Upgrade command metadata.
///
/// Native update delivery is not attached in this source checkout.
pub struct EvolveDirective;

#[async_trait]
impl Directive for EvolveDirective {
    fn name(&self) -> &str {
        "upgrade"
    }

    fn description(&self) -> &str {
        "Check for and install updates"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Handle flags
        if args
            .first()
            .map(|a| matches!(*a, "--check" | "-c"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(format!(
                "Current version: {}\nUpdate service is not attached in this source checkout.",
                ctx.version
            )));
        }

        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /upgrade [options]\n\n                 Check for and install product updates.\n\n                 Options:\n                   --check, -c    Only check, don't install\n                   --force, -f    Force reinstall current version"
                    .to_string(),
            ));
        }

        let build_info = match &ctx.build_time {
            Some(bt) => format!(" (built {})", bt),
            None => String::new(),
        };

        Ok(CommandResult::Error(format!(
            "Cannot check or install updates from this command runner. Current version: {}{}. Native update delivery is not attached in this source checkout.",
            ctx.version, build_info
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
    fn upgrade_does_not_claim_latest_without_update_service() {
        let output = tokio_test::block_on(EvolveDirective.execute(&[], &test_context()))
            .expect("upgrade command");

        let CommandResult::Error(text) = output else {
            panic!("upgrade should fail closed without update service");
        };
        assert!(text.contains("Cannot check or install updates"), "{text}");
        assert!(!text.contains("latest version"), "{text}");
    }
}
