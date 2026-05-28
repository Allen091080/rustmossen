//! `/version` — Print the current version information.
//!
//! Shows the running version of the product, build time,
//! and relevant environment details. Only available to
//! internal users for debugging purposes.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Version command — displays build and runtime information.
///
/// Output includes:
/// - Semantic version number (MAJOR.MINOR.PATCH)
/// - Build timestamp (ISO 8601 format, if available)
/// - Runtime environment details
/// - Whether the current version is the latest
///
/// This command is restricted to internal users (`USER_TYPE=internal`)
/// to prevent version fingerprinting in production environments.
pub struct VersionDirective;

#[async_trait]
impl Directive for VersionDirective {
    fn name(&self) -> &str {
        "version"
    }

    fn description(&self) -> &str {
        "Print the version this session is running"
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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_internal_user()
    }

    fn is_hidden(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let version_text = match &ctx.build_time {
            Some(build_time) => format!("{} (built {})", ctx.version, build_time),
            None => ctx.version.clone(),
        };
        Ok(CommandResult::Text(version_text))
    }
}
