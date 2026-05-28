//! `/fast` — Toggle fast/turbo mode for quicker responses.
//!
//! When enabled, turbo mode optimizes for response speed by:
//! - Using a faster (possibly smaller) model
//! - Reducing reasoning depth
//! - Limiting tool use to essential operations
//! - Shortening response length

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Turbo/Fast mode command — speed optimization toggle.
///
/// Turbo mode trades response quality for speed. Useful for:
/// - Quick questions that don't need deep analysis
/// - Rapid iteration cycles
/// - Simple file edits and lookups
/// - When operating under time pressure
pub struct TurboDirective;

#[async_trait]
impl Directive for TurboDirective {
    fn name(&self) -> &str {
        "fast"
    }

    fn aliases(&self) -> &[&str] {
        &["turbo"]
    }

    fn description(&self) -> &str {
        "Toggle fast mode for quicker responses"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[on|off]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let first = args.first().map(|s| s.to_lowercase());
        if first
            .as_deref()
            .map(|a| matches!(a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /fast [on|off]\n\n                 Toggle fast/turbo mode for quicker responses.\n\n                 When enabled:\n                 - Uses a faster model variant\n                 - Reduces reasoning depth\n                 - Limits response length\n                 - Optimizes for speed over thoroughness\n\n                 Without arguments, toggles the current state."
                    .to_string(),
            ));
        }

        let current = ctx
            .env_vars
            .get("MOSSEN_FAST_MODE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "on"))
            .unwrap_or(false);

        if matches!(first.as_deref(), None | Some("status" | "current" | "show")) {
            return Ok(CommandResult::Text(format!(
                "Fast mode: {}\nThis command reports the current environment only; live fast-mode switching is not attached to this command runner.",
                if current { "enabled" } else { "disabled" }
            )));
        }

        match first.as_deref() {
            Some("on" | "enable" | "true" | "1" | "off" | "disable" | "false" | "0") => {
                Ok(CommandResult::Error(
                    "Cannot change fast mode from this command runner; live fast-mode switching is not attached to the engine request path."
                        .to_string(),
                ))
            }
            Some(v) => Ok(CommandResult::Error(format!(
                "Invalid value: \"{}\". Use on/off.",
                v
            ))),
            None => unreachable!("handled above"),
        }
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
    fn fast_directive_does_not_claim_live_engine_update() {
        let output = tokio_test::block_on(TurboDirective.execute(&["on"], &test_context()))
            .expect("fast command");

        let CommandResult::Error(text) = output else {
            panic!("fast should not claim success until the engine path is wired");
        };
        assert!(text.contains("Cannot change fast mode"), "{text}");
        assert!(!text.contains("Fast mode: enabled"), "{text}");
    }
}
