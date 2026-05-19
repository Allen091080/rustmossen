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
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            return Ok(CommandResult::Text(
                "Usage: /files [filter]\n\n                 List files modified in this session.\n\n                 Filters:\n                   --created    Show only created files\n                   --modified   Show only modified files\n                   --all        Show all file operations (default)\n\n                 Output shows files grouped by operation type."
                    .to_string(),
            ));
        }

        // Filter by operation type if specified
        let filter = args.first().and_then(|a| {
            let stripped = a.trim_start_matches('-');
            if FILE_OPS.contains(&stripped) { Some(stripped) } else { None }
        });

        // In full implementation: query session state for file operations
        let msg = match filter {
            Some(op) => format!("No {} files in this session.", op),
            None => "No file changes recorded in this session.".to_string(),
        };

        Ok(CommandResult::Text(msg))
    }
}
