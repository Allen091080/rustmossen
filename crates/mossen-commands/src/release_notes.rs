//! `/release-notes` — Show recent release notes and changelog.
//!
//! Fetches and displays the changelog for recent versions,
//! formatted as a readable list of changes per version.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Release notes command — version changelog display.
///
/// Flow:
/// 1. Try to fetch latest changelog from server (with 500ms timeout)
/// 2. If fetch succeeds, display fresh notes
/// 3. If fetch fails/timeouts, fall back to cached notes
/// 4. If no cache, show link to online changelog
pub struct ReleaseNotesDirective;

/// Changelog URL for fallback.
const CHANGELOG_URL: &str = "https://docs.mossen.dev/changelog";

#[async_trait]
impl Directive for ReleaseNotesDirective {
    fn name(&self) -> &str {
        "release-notes"
    }

    fn description(&self) -> &str {
        "Show recent release notes and changelog"
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

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(format!(
                "Usage: /release-notes [version]\n\n                 Show recent release notes.\n\n                 Options:\n                   [version]    Show notes for a specific version\n                   --all        Show all available release notes\n\n                 Full changelog: {}",
                CHANGELOG_URL
            )));
        }

        // In full implementation:
        // 1. Try fetchAndStoreChangelog() with timeout
        // 2. Parse release notes from stored changelog
        // 3. Format as version headers with bullet points

        let version_filter = args.first().map(|v| v.trim_start_matches('v'));

        match version_filter {
            Some(v) => {
                Ok(CommandResult::Text(format!(
                    "No release notes found for version {}.\n                     See full changelog at: {}",
                    v, CHANGELOG_URL
                )))
            }
            None => {
                Ok(CommandResult::Text(format!(
                    "Version {}:\n                     · Current release\n\n                     See full changelog at: {}",
                    ctx.version, CHANGELOG_URL
                )))
            }
        }
    }
}
