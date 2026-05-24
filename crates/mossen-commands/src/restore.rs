//! `/resume` — Resume a previous session by ID or selection.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Restore (resume) directive — resumes a previously saved conversation session.
pub struct RestoreDirective;

/// Validate a session ID format (UUID-like string).
fn is_valid_session_id(id: &str) -> bool {
    // UUID format: 8-4-4-4-12 hex chars
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lens = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected_lens.iter())
        .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Parse the resume argument — could be a session ID, number (index), or empty (picker).
fn parse_resume_arg(args: &[&str]) -> ResumeTarget {
    if args.is_empty() {
        return ResumeTarget::Picker;
    }

    let arg = args.join(" ");
    let trimmed = arg.trim();

    if trimmed.is_empty() {
        return ResumeTarget::Picker;
    }

    // Check if it's a numeric index
    if let Ok(n) = trimmed.parse::<usize>() {
        return ResumeTarget::Index(n);
    }

    // Check if it looks like a session ID
    if is_valid_session_id(trimmed) {
        return ResumeTarget::SessionId(trimmed.to_string());
    }

    // Otherwise treat as a search query for session title
    ResumeTarget::Search(trimmed.to_string())
}

/// Target for the resume command.
enum ResumeTarget {
    /// Open the session picker UI
    Picker,
    /// Resume by numeric index (1-based from recent list)
    Index(usize),
    /// Resume by exact session UUID
    SessionId(String),
    /// Search sessions by title/content
    Search(String),
}

#[async_trait]
impl Directive for RestoreDirective {
    fn name(&self) -> &str {
        "resume"
    }

    fn aliases(&self) -> &[&str] {
        &["continue"]
    }

    fn description(&self) -> &str {
        "Resume a previous session"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[session-id|index|search]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let target = parse_resume_arg(args);

        match target {
            ResumeTarget::Picker => {
                // Show recent sessions list
                Ok(CommandResult::Text(
                    "Recent Sessions\n\
                     ===============\n\n\
                     No recent sessions found.\n\n\
                     Usage:\n\
                     · /resume <number>  — Resume by index (most recent = 1)\n\
                     · /resume <id>      — Resume by session ID"
                        .to_string(),
                ))
            }
            ResumeTarget::Index(n) => {
                if n == 0 {
                    return Ok(CommandResult::Error(
                        "Session index must be 1 or greater.".to_string(),
                    ));
                }
                // In full implementation: load session at index n from recent list
                Ok(CommandResult::System(format!(
                    "Resuming session #{} from history...",
                    n
                )))
            }
            ResumeTarget::SessionId(id) => {
                // In full implementation: load the session transcript and resume
                Ok(CommandResult::System(format!("Resuming session {}...", id)))
            }
            ResumeTarget::Search(query) => {
                // In full implementation: search sessions by title and show matches
                Ok(CommandResult::System(format!(
                    "Searching sessions for: {}",
                    query
                )))
            }
        }
    }
}
