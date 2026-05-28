//! `/branch` — Create a conversation branch (fork).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Branch directive metadata.
///
/// The actual branch operation needs access to the live transcript store and
/// session switcher. That state is not attached to the generic command runner,
/// so execution fails closed instead of claiming a branch was created.
pub struct BranchDirective;

/// Derive a title from the first user message text.
fn derive_first_prompt(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_SESSION_BRANCHING")
    }

    fn argument_hint(&self) -> &str {
        "[name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|arg| matches!(*arg, "help" | "-h" | "--help" | "status"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /branch [name]\n\nConversation branching requires the live TUI session store. This command runner does not have a transcript writer or session switcher attached."
                    .to_string(),
            ));
        }

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

        let base_name = custom_title.as_deref().unwrap_or("Current conversation");
        let effective_title = generate_fork_name(base_name, 0);
        let _ = derive_first_prompt(""); // helper used in full implementation

        Ok(CommandResult::Error(format!(
            "Cannot create branch \"{}\" from this command runner. No transcript writer or live session switcher is attached, so no branch was created.",
            effective_title
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
    fn branch_directive_does_not_claim_branch_creation_without_session_store() {
        let output =
            tokio_test::block_on(BranchDirective.execute(&["experiment"], &test_context()))
                .expect("branch command");

        let CommandResult::Error(text) = output else {
            panic!("branch should fail closed outside the live session store");
        };
        assert!(text.contains("Cannot create branch"), "{text}");
        assert!(!text.contains("You are now in the branch"), "{text}");
    }
}
