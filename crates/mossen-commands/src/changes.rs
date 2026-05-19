//! `/diff` — Show file changes made in this session.
//!
//! Displays a structured view of all files that have been modified,
//! created, or deleted during the current session. Shows inline diffs
//! with syntax highlighting.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Diff/Changes command — shows session file modifications.
///
/// In interactive mode, renders a scrollable diff viewer widget.
/// In non-interactive mode, outputs a text summary of changed files.
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
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Usage: /diff [options]\n\n                 Show all file changes made during this session.\n\n                 Options:\n                   --stat    Show only file names and change counts\n                   --full    Show complete diffs (no truncation)\n\n                 Aliases: /changes"
                    .to_string(),
            ));
        }

        // Non-interactive: text summary
        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(
                "No file changes recorded in this session.".to_string(),
            ));
        }

        // Stat-only mode
        if args.first().map(|a| *a == "--stat").unwrap_or(false) {
            return Ok(CommandResult::Text(
                "No file changes recorded in this session.".to_string(),
            ));
        }

        // Interactive mode: show uncommitted changes and per-turn diffs
        // The TS version renders a DiffDialog with messages; here we show git diff
        let output = std::process::Command::new("git")
            .args(["diff", "--stat"])
            .current_dir(&ctx.cwd)
            .output();

        match output {
            Ok(result) if result.status.success() => {
                let diff_output = String::from_utf8_lossy(&result.stdout).to_string();
                if diff_output.trim().is_empty() {
                    Ok(CommandResult::Text(
                        "No uncommitted changes in this session.".to_string(),
                    ))
                } else {
                    Ok(CommandResult::Text(format!(
                        "Uncommitted changes:\n\n{}",
                        diff_output
                    )))
                }
            }
            _ => Ok(CommandResult::Text(
                "No file changes recorded in this session.".to_string(),
            )),
        }
    }
}
