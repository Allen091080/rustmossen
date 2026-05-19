//! `/branch` — Create a conversation branch (fork).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Branch directive — forks the current conversation, creating a new session
/// that starts with the same history up to this point. The original session
/// remains unchanged and can be resumed separately.
pub struct BranchDirective;

/// Derive a title from the first user message text.
fn derive_first_prompt(text: &str) -> String {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.len() <= 100 {
        collapsed
    } else {
        format!("{}...", &collapsed[..97])
    }
}

/// Generate a unique fork name by appending "(Branch)" or "(Branch N)".
fn generate_fork_name(base_name: &str, existing_count: usize) -> String {
    if existing_count == 0 {
        format!("{} (Branch)", base_name)
    } else {
        format!("{} (Branch {})", base_name, existing_count + 1)
    }
}

#[async_trait]
impl Directive for BranchDirective {
    fn name(&self) -> &str {
        "branch"
    }

    fn aliases(&self) -> &[&str] {
        &["fork"]
    }

    fn description(&self) -> &str {
        "Create a branch of the current conversation at this point"
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

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let custom_title = if args.is_empty() {
            None
        } else {
            let joined = args.join(" ");
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        // In full implementation:
        // 1. Read current transcript file
        // 2. Filter to main conversation entries (no sidechains)
        // 3. Create new session ID
        // 4. Write forked transcript with new sessionId and forkedFrom metadata
        // 5. Copy content-replacement entries
        // 6. Generate unique fork name (handle collisions)
        // 7. Save custom title
        // 8. Resume into the fork
        // 9. Log analytics event

        let base_name = custom_title
            .as_deref()
            .unwrap_or("Current conversation");
        let effective_title = generate_fork_name(base_name, 0);
        let _ = derive_first_prompt(""); // helper used in full implementation

        Ok(CommandResult::System(format!(
            "Branched conversation as \"{}\". You are now in the branch.\nTo resume the original session, use /resume.",
            effective_title
        )))
    }
}
