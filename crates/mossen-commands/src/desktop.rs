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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_DESKTOP_HANDOFF")
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if is_desktop_app_available(ctx) {
            Ok(CommandResult::Error(
                "Desktop app availability was signaled, but no desktop handoff IPC or deep link launcher is attached to this command runner. No session was handed off."
                    .to_string(),
            ))
        } else {
            let url = get_desktop_download_url(ctx);
            Ok(CommandResult::Text(format!(
                "Desktop app not detected.\nDownload it at: {}\n\nAfter installing, run /desktop again to hand off this session.",
                url
            )))
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
    fn desktop_directive_does_not_claim_handoff_without_launcher() {
        let mut ctx = test_context();
        ctx.env_vars
            .insert("MOSSEN_DESKTOP_INSTALLED".to_string(), "1".to_string());
        let output =
            tokio_test::block_on(DesktopDirective.execute(&[], &ctx)).expect("desktop command");

        let CommandResult::Error(text) = output else {
            panic!("desktop should fail closed when IPC is not attached");
        };
        assert!(text.contains("no desktop handoff IPC"), "{text}");
        assert!(!text.contains("Opening in desktop app"), "{text}");
    }
}
