//! `/proactive` — Toggle proactive suggestions from the model.
//!
//! When enabled, the model may proactively suggest improvements,
//! optimizations, or point out potential issues without being asked.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Proactive suggestions command — controls unsolicited model behavior.
///
/// States:
/// - `on`: Model may suggest improvements proactively
/// - `off`: Model only responds when explicitly asked
/// - (no args): Toggle current state
pub struct ProactiveDirective;

#[async_trait]
impl Directive for ProactiveDirective {
    fn name(&self) -> &str {
        "proactive"
    }

    fn description(&self) -> &str {
        "Toggle proactive suggestions from the model"
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
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /proactive [on|off]\n\n                 Toggle proactive suggestions from the model.\n\n                 When enabled, the model may:\n                 - Suggest code improvements\n                 - Point out potential bugs\n                 - Recommend best practices\n                 - Offer alternative approaches"
                    .to_string(),
            ));
        }

        let current = ctx
            .env_vars
            .get("MOSSEN_PROACTIVE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "on"))
            .unwrap_or(false);

        match args.first().map(|s| s.to_lowercase()).as_deref() {
            Some("on" | "enable" | "true" | "1") => {
                Ok(CommandResult::System(
                    "Proactive suggestions: enabled\n                     The model may now suggest improvements without being asked."
                        .to_string(),
                ))
            }
            Some("off" | "disable" | "false" | "0") => {
                Ok(CommandResult::System(
                    "Proactive suggestions: disabled\n                     The model will only respond when explicitly asked."
                        .to_string(),
                ))
            }
            None => {
                let new_state = if current { "disabled" } else { "enabled" };
                Ok(CommandResult::System(format!(
                    "Proactive suggestions: {}", new_state
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
