//! `/session` — Display remote session info or QR code for mobile access.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Session directive — shows remote session URL and connection info.
pub struct SessionDirective;

/// Format the session info display for terminal output.
fn format_session_info(remote_url: &str) -> String {
    let mut output = String::new();
    output.push_str("Remote session\n");
    output.push_str("──────────────\n");
    output.push_str(&format!("URL: {}\n", remote_url));
    output.push_str("\nScan QR code or open URL in browser to connect.\n");
    output.push_str("(press esc to close)");
    output
}

#[async_trait]
impl Directive for SessionDirective {
    fn name(&self) -> &str {
        "session"
    }

    fn aliases(&self) -> &[&str] {
        &["remote"]
    }

    fn description(&self) -> &str {
        "Show remote session connection info"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_remote_mode
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Check if we're in remote mode
        if !ctx.is_remote_mode {
            return Ok(CommandResult::System(
                "Not in remote mode. Start with `mossen --remote` to use this command.".to_string(),
            ));
        }

        // Get the remote session URL from environment
        let remote_url = ctx
            .env_vars
            .get("MOSSEN_REMOTE_SESSION_URL")
            .cloned()
            .unwrap_or_default();

        if remote_url.is_empty() {
            return Ok(CommandResult::System(
                "Remote session URL not available. The session may still be initializing."
                    .to_string(),
            ));
        }

        // In Phase 5 TUI this would render QR code widget; for now output text info
        Ok(CommandResult::Text(format_session_info(&remote_url)))
    }
}
