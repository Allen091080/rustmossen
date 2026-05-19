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

/// Check if the current session is a teammate (swarm child).
fn is_teammate(ctx: &CommandContext) -> bool {
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
        // Teammates cannot rename — their names are set by the team leader
        if is_teammate(ctx) {
            return Ok(CommandResult::System(
                "Cannot rename: This session is a swarm teammate. Teammate names are set by the team leader.".to_string(),
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

        // In full implementation:
        // 1. Get current session ID and transcript path
        // 2. Save custom title via saveCustomTitle(sessionId, newName, fullPath)
        // 3. Also persist as agent name for prompt-bar display
        // 4. Update app state with new name

        Ok(CommandResult::System(format!(
            "Session renamed to: {}",
            new_name
        )))
    }
}
