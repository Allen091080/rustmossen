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
                Ok(CommandResult::Error(format!(
                    "Cannot resume session #{} from this command runner. Use /resume in the interactive TUI so the render snapshot and live session state are updated together.",
                    n
                )))
            }
            ResumeTarget::SessionId(id) => {
                Ok(CommandResult::Error(format!(
                    "Cannot resume session {} from this command runner. Use /resume in the interactive TUI so the render snapshot and live session state are updated together.",
                    id
                )))
            }
            ResumeTarget::Search(query) => {
                Ok(CommandResult::Error(format!(
                    "Cannot search/resume sessions for \"{}\" from this command runner. Use /resume in the interactive TUI.",
                    query
                )))
            }
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
    fn resume_directive_does_not_claim_live_session_switch() {
        let output = tokio_test::block_on(RestoreDirective.execute(&["1"], &test_context()))
            .expect("resume command");

        let CommandResult::Error(text) = output else {
            panic!("resume by index should not claim success outside TUI");
        };
        assert!(text.contains("Cannot resume session"), "{text}");
        assert!(!text.contains("Resuming session"), "{text}");
    }
}
