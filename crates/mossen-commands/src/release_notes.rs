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

        let version_filter = args.first().map(|v| v.trim_start_matches('v'));

        match version_filter {
            Some(v) => {
                Ok(CommandResult::Text(format!(
                    "No bundled release notes source is attached to this build for version {}.\n                     See full changelog at: {}",
                    v, CHANGELOG_URL
                )))
            }
            None => {
                Ok(CommandResult::Text(format!(
                    "Release notes are not bundled in this source checkout.\n                     Current binary version: {}\n                     See full changelog at: {}",
                    ctx.version, CHANGELOG_URL
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
    fn release_notes_do_not_invent_current_release_notes() {
        let output = tokio_test::block_on(ReleaseNotesDirective.execute(&[], &test_context()))
            .expect("release-notes command");

        let CommandResult::Text(text) = output else {
            panic!("release-notes should return text");
        };
        assert!(text.contains("not bundled"), "{text}");
        assert!(!text.contains("Current release"), "{text}");
    }
}
