//! `/chrome` — Manage Chrome browser integration.
//!
//! Translates `commands/chrome/chrome.tsx` (289 lines).
//! Provides a menu for managing the Chrome extension: install, reconnect,
//! manage permissions, and toggle default-enabled state.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Menu actions for Chrome integration.
#[derive(Debug, Clone)]
enum ChromeMenuAction {
    InstallExtension,
    Reconnect,
    ManagePermissions,
    ToggleDefault,
}

/// Check if Chrome extension is installed (stub - would check filesystem/registry).
fn is_chrome_extension_installed() -> bool {
    false
}

/// Check if the user can use Chrome integration (subscription check).
fn can_use_chrome_integration() -> bool {
    true
}

/// Check if running in WSL environment.
fn is_wsl() -> bool {
    std::env::var("WSL_DISTRO_NAME").is_ok()
        || std::env::var("WSLENV").is_ok()
}

/// `/chrome` command.
pub struct ChromeDirective;

#[async_trait]
impl Directive for ChromeDirective {
    fn name(&self) -> &str {
        "chrome"
    }

    fn description(&self) -> &str {
        "Manage Chrome browser integration"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;
        let extension_installed = is_chrome_extension_installed();
        let is_subscriber = can_use_chrome_integration();
        let wsl = is_wsl();

        let mut output = format!("{} in Chrome (Beta)\n\n", product_name);

        // Status section
        output.push_str("Status:\n");
        if extension_installed {
            output.push_str("  Extension: Installed\n");
            output.push_str("  Connection: Not connected\n\n");
        } else {
            output.push_str("  Extension: Not installed\n\n");
        }

        // Build menu options
        output.push_str("Options:\n");

        if !extension_installed {
            output.push_str("  1. Install Chrome Extension\n");
            output.push_str("     Download from the Chrome Web Store to enable browser integration.\n\n");
        }

        if extension_installed {
            output.push_str("  2. Reconnect to Chrome\n");
            output.push_str("     Re-establish connection with Chrome browser.\n\n");

            output.push_str("  3. Manage Permissions\n");
            output.push_str("     Configure which sites the extension can access.\n\n");

            output.push_str("  4. Toggle Default Enabled\n");
            output.push_str("     Set whether Chrome integration is enabled by default.\n\n");
        }

        // WSL warning
        if wsl {
            output.push_str("Note: Chrome integration in WSL requires Chrome to be running on the Windows host.\n");
            output.push_str("The extension communicates with the CLI through a native messaging host.\n\n");
        }

        // Subscription check
        if !is_subscriber {
            output.push_str("Chrome integration requires an active subscription.\n\n");
        }

        output.push_str("Use /chrome to manage your Chrome browser integration.");

        Ok(CommandResult::Text(output))
    }
}
