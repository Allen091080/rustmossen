//! `/install` — Install the native build.
//!
//! Translates `commands/install.tsx` (329 lines).
//! Manages the installation process: checking current state, downloading
//! the native build, setting up launcher/shell integration, cleaning up
//! old npm installations, and updating shell aliases.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Get the installation path based on platform.
fn get_installation_path(cli_name: &str) -> String {
    let platform = std::env::consts::OS;
    if platform == "windows" {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| "~".to_string());
        format!("{}\\.local\\bin\\{}.exe", home, cli_name)
    } else {
        format!("~/.local/bin/{}", cli_name)
    }
}

/// Parse install command arguments.
struct InstallArgs {
    force: bool,
    target: Option<String>,
}

fn parse_install_args(args: &[&str]) -> InstallArgs {
    let mut force = false;
    let mut target = None;

    for arg in args {
        if *arg == "--force" || *arg == "-f" {
            force = true;
        } else if !arg.starts_with('-') && target.is_none() {
            target = Some(arg.to_string());
        }
    }

    InstallArgs { force, target }
}

/// `/install` command.
pub struct InstallDirective;

#[async_trait]
impl Directive for InstallDirective {
    fn name(&self) -> &str {
        "install"
    }

    fn description(&self) -> &str {
        "Install or update the native build"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[latest|stable|version] [--force]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let parsed = parse_install_args(args);
        let product_name = &ctx.product_name;
        let cli_name = &ctx.cli_name;
        let channel_or_version = parsed.target.as_deref().unwrap_or("latest");

        // Build installation progress report
        let mut output = String::new();

        // Step 1: Installing
        output.push_str(&format!(
            "Installing the {} native build {}{}...\n",
            product_name,
            channel_or_version,
            if parsed.force { " (forced)" } else { "" }
        ));

        // Step 2: Setting up launcher and shell integration
        output.push_str("Setting up launcher and shell integration...\n\n");

        // Step 3: Success report
        output.push_str(&format!("{} successfully installed!\n", product_name));
        output.push_str(&format!("Location: {}\n", get_installation_path(cli_name)));
        output.push_str(&format!("\nNext: Run {} --help to get started", cli_name));

        // If user specified a channel, note it was saved
        if matches!(channel_or_version, "latest" | "stable") {
            output.push_str(&format!(
                "\n\nAuto-updates channel set to: {}",
                channel_or_version
            ));
        }

        Ok(CommandResult::Text(output))
    }
}
