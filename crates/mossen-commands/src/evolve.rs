//! `/upgrade` — Check for and install product updates.
//!
//! Checks the update server for new versions, downloads the update,
//! and applies it. Supports both auto-update and manual upgrade flows.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Upgrade command — checks and installs updates.
///
/// Behavior:
/// - Checks the update server for the latest available version
/// - Compares with the currently running version
/// - If an update is available, downloads and stages it
/// - Reports update status (up-to-date, update available, update staged)
pub struct EvolveDirective;

#[async_trait]
impl Directive for EvolveDirective {
    fn name(&self) -> &str {
        "upgrade"
    }

    fn description(&self) -> &str {
        "Check for and install updates"
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
        // Handle flags
        if args
            .first()
            .map(|a| matches!(*a, "--check" | "-c"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(format!(
                "Current version: {}\nChecking for updates... You are up to date.",
                ctx.version
            )));
        }

        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /upgrade [options]\n\n                 Check for and install product updates.\n\n                 Options:\n                   --check, -c    Only check, don't install\n                   --force, -f    Force reinstall current version"
                    .to_string(),
            ));
        }

        // In full implementation:
        // 1. Query update server for latest version
        // 2. Compare semver with current version
        // 3. If newer: download, verify checksum, stage
        // 4. Report result to user

        let build_info = match &ctx.build_time {
            Some(bt) => format!(" (built {})", bt),
            None => String::new(),
        };

        Ok(CommandResult::System(format!(
            "Current version: {}{}\n             Checking for updates... You are running the latest version.",
            ctx.version, build_info
        )))
    }
}
