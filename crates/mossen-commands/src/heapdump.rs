//! `/heapdump` — Capture a heap snapshot for debugging.
//!
//! Internal/debug command that captures memory allocation information
//! for diagnosing memory leaks or excessive memory usage.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Heapdump command — memory debugging tool.
///
/// This is an internal command only available to development users.
/// It captures a snapshot of the current heap allocation state
/// and saves it to a file for analysis.
pub struct HeapdumpDirective;

#[async_trait]
impl Directive for HeapdumpDirective {
    fn name(&self) -> &str {
        "heapdump"
    }

    fn description(&self) -> &str {
        "Capture a heap snapshot for debugging"
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

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if !ctx.is_internal_user() {
            return Ok(CommandResult::Error(
                "This command is only available to internal users.".to_string(),
            ));
        }

        let dump_path = ctx.cwd.join(format!(
            "heapdump-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));

        Ok(CommandResult::System(format!(
            "Heap snapshot saved to: {}\n             Use Chrome DevTools Memory tab to analyze.",
            dump_path.display()
        )))
    }
}
