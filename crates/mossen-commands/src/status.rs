//! `/status` — Show session status, model info, and configuration summary.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Status directive — displays session overview and settings panel.
pub struct StatusDirective;

/// Build a status summary string from the current context.
fn build_status_summary(ctx: &CommandContext) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "┌─ {} Status ─────────────────────",
        ctx.product_name
    ));
    lines.push(format!("│ Version: {}", ctx.version));

    if let Some(ref build_time) = ctx.build_time {
        lines.push(format!("│ Built: {}", build_time));
    }

    lines.push(format!("│ CWD: {}", ctx.cwd.display()));

    // Mode indicators
    let mut modes = Vec::new();
    if ctx.is_remote_mode {
        modes.push("remote");
    }
    if ctx.is_non_interactive {
        modes.push("non-interactive");
    }
    if ctx.is_custom_backend {
        modes.push("custom-backend");
    }
    if !modes.is_empty() {
        lines.push(format!("│ Mode: {}", modes.join(", ")));
    }

    // User type
    if let Some(ref user_type) = ctx.user_type {
        lines.push(format!("│ User type: {}", user_type));
    }

    lines.push("└──────────────────────────────────".to_string());

    lines.join("\n")
}

#[async_trait]
impl Directive for StatusDirective {
    fn name(&self) -> &str {
        "status"
    }

    fn aliases(&self) -> &[&str] {
        &["info"]
    }

    fn description(&self) -> &str {
        "Show session status and configuration"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // In Phase 5, this renders the full Settings widget with defaultTab="Status"
        // For now, output a text-based status summary
        let summary = build_status_summary(ctx);
        Ok(CommandResult::Text(summary))
    }
}
