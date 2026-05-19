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
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Usage: /fast [on|off]\n\n                 Toggle fast/turbo mode for quicker responses.\n\n                 When enabled:\n                 - Uses a faster model variant\n                 - Reduces reasoning depth\n                 - Limits response length\n                 - Optimizes for speed over thoroughness\n\n                 Without arguments, toggles the current state."
                    .to_string(),
            ));
        }

        let current = ctx.env_vars
            .get("MOSSEN_FAST_MODE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "on"))
            .unwrap_or(false);

        match args.first().map(|s| s.to_lowercase()).as_deref() {
            Some("on" | "enable" | "true" | "1") => {
                Ok(CommandResult::System(
                    "Fast mode: enabled\n                     Responses will prioritize speed over depth."
                        .to_string(),
                ))
            }
            Some("off" | "disable" | "false" | "0") => {
                Ok(CommandResult::System(
                    "Fast mode: disabled\n                     Responses will use full reasoning depth."
                        .to_string(),
                ))
            }
            None => {
                let new_state = if current { "disabled" } else { "enabled" };
                let desc = if current {
                    "Responses will use full reasoning depth."
                } else {
                    "Responses will prioritize speed over depth."
                };
                Ok(CommandResult::System(format!(
                    "Fast mode: {}\n{}", new_state, desc
                )))
            }
            Some(v) => {
                Ok(CommandResult::Error(format!(
                    "Invalid value: \"{}\". Use on/off.", v
                )))
            }
        }
    }
}
