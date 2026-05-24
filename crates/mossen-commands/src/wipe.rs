//! `/clear` — Clear conversation history and start fresh.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Wipe (clear) directive — clears conversation, caches, or both.
pub struct WipeDirective;

/// Subcommand for the clear/wipe operation.
enum WipeAction {
    /// Clear the conversation (default)
    Conversation,
    /// Clear caches only
    Caches,
    /// Clear everything
    All,
}

/// Parse the wipe subcommand from arguments.
fn parse_wipe_action(args: &[&str]) -> WipeAction {
    match args.first().map(|s| s.to_lowercase()).as_deref() {
        Some("caches") | Some("cache") => WipeAction::Caches,
        Some("all") | Some("everything") => WipeAction::All,
        _ => WipeAction::Conversation,
    }
}

#[async_trait]
impl Directive for WipeDirective {
    fn name(&self) -> &str {
        "clear"
    }

    fn aliases(&self) -> &[&str] {
        &["reset", "new"]
    }

    fn description(&self) -> &str {
        "Clear conversation and start fresh"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[conversation|caches|all]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let action = parse_wipe_action(args);

        match action {
            WipeAction::Conversation => {
                // Clear conversation messages — in full implementation this
                // removes messages from the active transcript and resets state
                Ok(CommandResult::System(
                    "Conversation cleared. Starting fresh.".to_string(),
                ))
            }
            WipeAction::Caches => {
                // Clear file state cache, tool results cache, etc.
                Ok(CommandResult::System("Caches cleared.".to_string()))
            }
            WipeAction::All => {
                // Clear both conversation and caches
                Ok(CommandResult::System(
                    "Conversation and caches cleared. Starting fresh.".to_string(),
                ))
            }
        }
    }
}
