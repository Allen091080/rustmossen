//! `/diff` — Show file changes.
//!
//! The interactive TUI shows the semantic session diff. The generic command
//! runner can only inspect the current git worktree.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Diff/Changes command metadata.
pub struct ChangesDirective;

#[async_trait]
impl Directive for ChangesDirective {
    fn name(&self) -> &str {
        "diff"
    }

    fn aliases(&self) -> &[&str] {
        &["changes"]
    }

    fn description(&self) -> &str {
        "Show file changes in this session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Handle help flag
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /diff [options]\n\n                 In the interactive TUI, shows semantic file changes from the session. In this command runner, falls back to the current git worktree.\n\n                 Options:\n                   --stat    Show only file names and change counts\n                   --full    Show complete diffs (no truncation)\n\n                 Aliases: /changes"
                    .to_string(),
            ));
        }

        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(
                "Semantic session diff is not attached to this command runner. Use /diff in the interactive TUI, or run git diff directly for worktree changes."
                    .to_string(),
            ));
        }

        // Stat-only mode
        if args.first().map(|a| *a == "--stat").unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Semantic session diff is not attached to this command runner. Use /diff in the interactive TUI, or run git diff --stat directly for worktree changes."
                    .to_string(),
            ));
        }

        // Interactive fallback outside the TUI fast path: show worktree diff
        // status only, with explicit wording that this is not the semantic
        // session diff.
        let output = std::process::Command::new("git")
            .args(["diff", "--stat"])
            .current_dir(&ctx.cwd)
            .output();

        match output {
            Ok(result) if result.status.success() => {
                let diff_output = String::from_utf8_lossy(&result.stdout).to_string();
                if diff_output.trim().is_empty() {
                    Ok(CommandResult::Text(
                        "No uncommitted git worktree changes detected. Semantic session diff is only available in the interactive TUI."
                            .to_string(),
                    ))
                } else {
                    Ok(CommandResult::Text(format!(
                        "Uncommitted git worktree changes, not semantic session diff:\n\n{}",
                        diff_output
                    )))
                }
            }
            _ => Ok(CommandResult::Text(
                "Semantic session diff is not attached to this command runner, and git diff could not be read."
                    .to_string(),
            )),
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
    fn diff_directive_does_not_claim_empty_session_without_snapshot() {
        let output = tokio_test::block_on(ChangesDirective.execute(&[], &test_context()))
            .expect("diff command");

        let CommandResult::Text(text) = output else {
            panic!("diff should return explanatory text");
        };
        assert!(
            text.contains("Semantic session diff is not attached"),
            "{text}"
        );
        assert!(!text.contains("No file changes recorded"), "{text}");
    }
}
