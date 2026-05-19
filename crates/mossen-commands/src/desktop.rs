//! `/desktop` — Open the desktop app or handoff to it.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Desktop directive — launches or hands off the session to the Mossen desktop app.
/// Shows download instructions if the app is not installed.
pub struct DesktopDirective;

/// Check if the desktop app appears to be installed.
fn is_desktop_app_available(ctx: &CommandContext) -> bool {
    ctx.env_vars
        .get("MOSSEN_DESKTOP_INSTALLED")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Get the download URL for the desktop app.
fn get_desktop_download_url(ctx: &CommandContext) -> String {
    ctx.env_vars
        .get("MOSSEN_DESKTOP_DOWNLOAD_URL")
        .cloned()
        .unwrap_or_else(|| "https://mossen.dev/downloads/desktop".to_string())
}

#[async_trait]
impl Directive for DesktopDirective {
    fn name(&self) -> &str {
        "desktop"
    }

    fn aliases(&self) -> &[&str] {
        &["app"]
    }

    fn description(&self) -> &str {
        "Open in the desktop app"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if is_desktop_app_available(ctx) {
            // In full implementation: launch the desktop app via deep link or IPC
            // and hand off the current session
            Ok(CommandResult::System(
                "Opening in desktop app...".to_string(),
            ))
        } else {
            let url = get_desktop_download_url(ctx);
            // Phase 5 TUI: render DesktopHandoff widget with QR/link
            Ok(CommandResult::Text(format!(
                "Desktop app not detected.\nDownload it at: {}\n\nAfter installing, run /desktop again to hand off this session.",
                url
            )))
        }
    }
}
