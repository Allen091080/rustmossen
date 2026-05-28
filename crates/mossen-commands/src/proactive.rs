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
        let first = args.first().map(|s| s.to_lowercase());
        if first
            .as_deref()
            .map(|a| matches!(a, "help" | "-h" | "--help"))
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

        if matches!(first.as_deref(), None | Some("status" | "current" | "show")) {
            return Ok(CommandResult::Text(format!(
                "Proactive suggestions: {}\nThis command reports the current environment only; live proactive scheduling is not attached to this command runner.",
                if current { "enabled" } else { "disabled" }
            )));
        }

        match first.as_deref() {
            Some("on" | "enable" | "true" | "1" | "off" | "disable" | "false" | "0") => {
                Ok(CommandResult::Error(
                    "Cannot change proactive suggestions from this command runner; live proactive scheduling is not attached to the session loop."
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
    fn proactive_directive_does_not_claim_live_scheduler_update() {
        let output = tokio_test::block_on(ProactiveDirective.execute(&["on"], &test_context()))
            .expect("proactive command");

        let CommandResult::Error(text) = output else {
            panic!("proactive should not claim success until the scheduler path is wired");
        };
        assert!(
            text.contains("Cannot change proactive suggestions"),
            "{text}"
        );
        assert!(!text.contains("Proactive suggestions: enabled"), "{text}");
    }
}
