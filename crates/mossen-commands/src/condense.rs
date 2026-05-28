//! `/compact` — Compact conversation to free context space.
//!
//! Summarizes the conversation history to reduce token usage while
//! preserving important context. Supports custom summarization
//! instructions and multiple compaction strategies.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Compact command metadata.
///
/// Real compaction is owned by the TUI/structured runtime because it requires
/// live model history, hook context, cancellation, and transcript updates.
pub struct CondenseDirective;

#[async_trait]
impl Directive for CondenseDirective {
    fn name(&self) -> &str {
        "compact"
    }

    fn description(&self) -> &str {
        "Clear conversation history but keep a summary in context"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[custom summarization instructions]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        !ctx.is_env_truthy("DISABLE_COMPACT")
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let custom_instructions = if args.is_empty() {
            String::new()
        } else {
            args.join(" ")
        };

        if matches!(
            args.first().copied(),
            Some("help" | "-h" | "--help" | "status" | "plan" | "preview")
        ) || custom_instructions.is_empty()
        {
            return Ok(CommandResult::Text(
                "Compact status\nNo live conversation history is attached to this command runner. In the interactive TUI, use /compact plan, /compact status, or /compact run to perform real compaction."
                    .to_string(),
            ));
        }

        Ok(CommandResult::Error(format!(
            "Cannot compact conversation with focus \"{}\" from this command runner. No live model history, hook context, or transcript updater is attached.",
            custom_instructions
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
    fn compact_directive_does_not_claim_history_mutation_without_runtime_state() {
        let output = tokio_test::block_on(
            CondenseDirective.execute(&["keep", "decisions"], &test_context()),
        )
        .expect("compact command");

        let CommandResult::Error(text) = output else {
            panic!("compact should fail closed without live history");
        };
        assert!(text.contains("Cannot compact conversation"), "{text}");
        assert!(!text.contains("Conversation compacted"), "{text}");
        assert!(!text.contains("Context freed"), "{text}");
    }
}
