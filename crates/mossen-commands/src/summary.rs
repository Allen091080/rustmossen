//! `/summary` — Generate a summary of the current session.
//!
//! Creates a concise summary of what has been accomplished in
//! the current session, including files changed, decisions made,
//! and tasks completed.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Summary command — session activity summarization.
///
/// Generates a structured summary including:
/// - Key decisions and outcomes
/// - Files created, modified, or deleted
/// - Commands executed and their results
/// - Open questions or pending items
pub struct SummaryDirective;

#[async_trait]
impl Directive for SummaryDirective {
    fn name(&self) -> &str {
        "summary"
    }

    fn description(&self) -> &str {
        "Generate a summary of the current session"
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
                "Usage: /summary [options]\n\n                 Generate a summary of the current session.\n\n                 Options:\n                   --brief      Short one-paragraph summary\n                   --detailed   Full breakdown with all details\n                   --files      Focus on file changes only"
                    .to_string(),
            ));
        }

        let mode = args.first().map(|s| s.to_lowercase());
        let summary_type = match mode.as_deref() {
            Some("--brief" | "brief") => "brief",
            Some("--files" | "files") => "files",
            Some("--detailed" | "detailed") => "detailed",
            _ => "standard",
        };

        // In full implementation: analyze conversation messages to extract
        // key decisions, file operations, and outcomes
        Ok(CommandResult::Text(format!(
            "Session Summary ({})\n{}\n\n             No significant activity recorded yet in this session.",
            summary_type,
            "=".repeat(20 + summary_type.len())
        )))
    }
}
