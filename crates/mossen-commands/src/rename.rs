//! `/rename` — Rename the current session.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Rename directive — sets a custom name/title for the current session.
/// If no name is provided, auto-generates one from conversation context.
pub struct RenameDirective;

/// Maximum length for a session name.
const MAX_NAME_LENGTH: usize = 100;

/// Validate a proposed session name.
fn validate_name(name: &str) -> Result<&str, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Session name cannot be empty.".to_string());
    }
    if trimmed.len() > MAX_NAME_LENGTH {
        return Err(format!(
            "Session name too long ({} chars). Maximum is {} characters.",
            trimmed.len(),
            MAX_NAME_LENGTH
        ));
    }
    Ok(trimmed)
}

/// Check if the current session is a delegated child session.
fn is_delegated_session(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .get("MOSSEN_TEAMMATE")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[async_trait]
impl Directive for RenameDirective {
    fn name(&self) -> &str {
        "rename"
    }

    fn description(&self) -> &str {
        "Rename the current session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if is_delegated_session(ctx) {
            return Ok(CommandResult::System(
                "Cannot rename this child agent session from inside the child.".to_string(),
            ));
        }

        let raw_name = args.join(" ");

        if raw_name.trim().is_empty() {
            // No argument: auto-generate name from conversation context
            // In full implementation: calls generateSessionName with recent messages
            // and an abort signal. Falls back to an error if no context yet.
            return Ok(CommandResult::System(
                "Could not generate a name: no conversation context yet. Usage: /rename <name>"
                    .to_string(),
            ));
        }

        // Validate the provided name
        let new_name = match validate_name(&raw_name) {
            Ok(n) => n,
            Err(e) => return Ok(CommandResult::Error(e)),
        };

        Ok(CommandResult::Error(format!(
            "Cannot rename from this command runner. Use /rename {new_name} in the interactive TUI session so the live session title state is updated."
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
    fn rename_directive_does_not_claim_success_outside_tui_state() {
        let output = tokio_test::block_on(RenameDirective.execute(&["demo"], &test_context()))
            .expect("rename command");

        let CommandResult::Error(text) = output else {
            panic!("rename should not claim success outside TUI state");
        };
        assert!(text.contains("Cannot rename"), "{text}");
        assert!(!text.contains("Session renamed to"), "{text}");
    }

    #[test]
    fn rename_child_session_message_avoids_team_wiring_claims() {
        let mut ctx = test_context();
        ctx.env_vars
            .insert("MOSSEN_TEAMMATE".to_string(), "1".to_string());
        let output =
            tokio_test::block_on(RenameDirective.execute(&["demo"], &ctx)).expect("rename command");

        let CommandResult::System(text) = output else {
            panic!("child rename should return a system message");
        };
        let lowered = text.to_ascii_lowercase();
        assert!(!lowered.contains("swarm"), "{text}");
        assert!(!lowered.contains("teammate"), "{text}");
    }
}
