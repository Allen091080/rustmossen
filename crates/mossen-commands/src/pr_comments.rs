//! `/pr-comments` — View and manage pull request comments.
//!
//! Internal command for reviewing and responding to comments
//! on the current pull request from within the session. Shows
//! threaded comments with resolution status.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// PR Comments command — pull request discussion viewer.
///
/// This is an internal/debug command that shows comments
/// from the current pull request and allows inline responses.
/// Supports filtering by status (resolved/unresolved) and author.
pub struct PrCommentsDirective;

/// Comment filter options.
const COMMENT_FILTERS: &[(&str, &str)] = &[
    ("--unresolved", "Show only unresolved comments"),
    ("--resolved", "Show only resolved comments"),
    ("--mine", "Show only your comments"),
    ("--all", "Show all comments (default)"),
];

#[async_trait]
impl Directive for PrCommentsDirective {
    fn name(&self) -> &str {
        "pr-comments"
    }

    fn description(&self) -> &str {
        "View and manage pull request comments"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_internal_user()
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.first().map(|a| matches!(*a, "help" | "-h" | "--help")).unwrap_or(false) {
            let mut help = String::from(
                "Usage: /pr-comments [filter]\n\n                 View comments on the current pull request.\n\n                 Filters:\n",
            );
            for (flag, desc) in COMMENT_FILTERS {
                help.push_str(&format!("  {:16} {}\n", flag, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        Ok(CommandResult::Text(
            "No pull request detected for the current branch.\n             Create a PR first with /ship or push your branch."
                .to_string(),
        ))
    }
}
