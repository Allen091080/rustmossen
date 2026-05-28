//! `/files` — List files modified in this session.
//!
//! Shows all files that have been read, created, modified, or deleted
//! during the current session. Groups files by operation type and
//! provides a summary count.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Files command — shows session file activity.
///
/// Displays files grouped by operation:
/// - Created: New files written during the session
/// - Modified: Existing files that were changed
/// - Read: Files that were read for context
/// - Deleted: Files that were removed
pub struct FilesDirective;

/// File operation types for display.
const FILE_OPS: &[&str] = &["created", "modified", "read", "deleted"];

#[async_trait]
impl Directive for FilesDirective {
    fn name(&self) -> &str {
        "files"
    }

    fn description(&self) -> &str {
        "List files modified in this session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /files [filter]\n\n                 List files modified in this session.\n\n                 Filters:\n                   --created    Show only created files\n                   --modified   Show only modified files\n                   --all        Show all file operations (default)\n\n                 Output shows files grouped by operation type."
                    .to_string(),
            ));
        }

        // Filter by operation type if specified
        let filter = args.first().and_then(|a| {
            let stripped = a.trim_start_matches('-');
            if FILE_OPS.contains(&stripped) {
                Some(stripped)
            } else {
                None
            }
        });

        let msg = match filter {
            Some(op) => format!(
                "File activity snapshot is not attached to this command runner, so {} files cannot be listed here. Use /files in the interactive TUI for the live session view.",
                op
            ),
            None => "File activity snapshot is not attached to this command runner. Use /files in the interactive TUI for the live session view.".to_string(),
        };

        Ok(CommandResult::Text(msg))
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
    fn files_directive_does_not_claim_empty_session_without_snapshot() {
        let output = tokio_test::block_on(FilesDirective.execute(&[], &test_context()))
            .expect("files command");

        let CommandResult::Text(text) = output else {
            panic!("files should return explanatory text");
        };
        assert!(text.contains("snapshot is not attached"), "{text}");
        assert!(!text.contains("No file changes recorded"), "{text}");
    }
}
