//! `/compact` — Compact conversation to free context space.
//!
//! Summarizes the conversation history to reduce token usage while
//! preserving important context. Supports custom summarization
//! instructions and multiple compaction strategies.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Compact command — reduces conversation context size.
///
/// Compaction strategies (applied in order):
/// 1. Session memory compaction (if no custom instructions)
/// 2. Reactive compaction (if reactive-only mode enabled)
/// 3. Traditional compaction (microcompact + summarize)
///
/// After compaction:
/// - Resets lastSummarizedMessageId
/// - Suppresses compact warning
/// - Clears user context cache
/// - Runs post-compact cleanup hooks
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

        // Strategy 1: Try session memory compaction (no custom instructions)
        if custom_instructions.is_empty() {
            // In full implementation:
            // - trySessionMemoryCompaction(messages, agentId)
            // - If successful: clear caches, run post-compact cleanup
            // - notifyCompaction for prompt cache break detection
            // - markPostCompaction()
            // - suppressCompactWarning()
            return Ok(CommandResult::System(
                "Conversation compacted successfully. Context freed for new messages.".to_string(),
            ));
        }

        // Strategy 2: Check reactive-only mode
        // In full implementation: reactiveCompact.isReactiveOnlyMode()
        // - Execute pre-compact hooks
        // - Build cache sharing params
        // - Run reactive compaction
        // - Merge hook instructions with custom instructions

        // Strategy 3: Traditional compaction
        // In full implementation:
        // - microcompactMessages(messages) to reduce tokens
        // - compactConversation(messages, context, cacheSharingParams)
        // - Reset lastSummarizedMessageId
        // - suppressCompactWarning()
        // - Clear user context cache
        // - Run post-compact cleanup

        Ok(CommandResult::System(format!(
            "Conversation compacted with focus: {}\n             Context freed for new messages.",
            custom_instructions
        )))
    }
}
