//! `/rewind` — Undo/rewind to a previous message checkpoint.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Undo (rewind) directive — opens the message selector to rewind conversation
/// to an earlier point, discarding subsequent messages.
pub struct UndoDirective;

/// Parse undo arguments to determine how many steps to rewind.
fn parse_undo_arg(args: &[&str]) -> UndoTarget {
    if args.is_empty() {
        return UndoTarget::Selector;
    }

    let arg = args.join(" ");
    let trimmed = arg.trim();

    if trimmed.is_empty() {
        return UndoTarget::Selector;
    }

    // Try to parse as a number of steps
    if let Ok(n) = trimmed.parse::<usize>() {
        if n == 0 {
            return UndoTarget::Selector;
        }
        return UndoTarget::Steps(n);
    }

    // "last" keyword for quick single-step undo
    if trimmed.eq_ignore_ascii_case("last") {
        return UndoTarget::Steps(1);
    }

    UndoTarget::Selector
}

/// Target for the undo operation.
enum UndoTarget {
    /// Open the message selector to pick a checkpoint
    Selector,
    /// Undo a specific number of message pairs
    Steps(usize),
}

#[async_trait]
impl Directive for UndoDirective {
    fn name(&self) -> &str {
        "rewind"
    }

    fn aliases(&self) -> &[&str] {
        &["undo", "checkpoint"]
    }

    fn description(&self) -> &str {
        "Rewind conversation to a previous point"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[steps|last]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let target = parse_undo_arg(args);

        match target {
            UndoTarget::Selector => {
                // Show checkpoint selection options
                Ok(CommandResult::Text(
                    "Rewind/Checkpoint\n\
                     =================\n\n\
                     No checkpoints available in current session.\n\n\
                     Usage:\n\
                     · /undo <N>        — Rewind N message pairs\n\
                     · /checkpoint      — Create a named checkpoint"
                        .to_string(),
                ))
            }
            UndoTarget::Steps(n) => {
                Ok(CommandResult::Error(format!(
                    "Cannot rewind {} message {} from this command runner. Use /undo in the interactive TUI so the visible transcript and engine history are updated together.",
                    n,
                    if n == 1 { "pair" } else { "pairs" }
                )))
            }
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
    fn undo_directive_does_not_claim_history_rewrite_outside_tui() {
        let output = tokio_test::block_on(UndoDirective.execute(&["1"], &test_context()))
            .expect("undo command");

        let CommandResult::Error(text) = output else {
            panic!("undo should not claim success outside TUI");
        };
        assert!(text.contains("Cannot rewind"), "{text}");
        assert!(!text.contains("Rewound"), "{text}");
    }
}
