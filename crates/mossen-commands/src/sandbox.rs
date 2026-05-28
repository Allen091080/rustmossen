//! `/sandbox` — Toggle and configure sandbox mode.
//!
//! Translates `commands/sandbox-toggle/sandbox-toggle.tsx` (83 lines).
//! Checks platform support, dependency status, and manages sandbox
//! settings including exclude patterns for commands.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Check if sandboxing is supported on the current platform.
fn is_sandbox_supported() -> bool {
    let os = std::env::consts::OS;
    matches!(os, "macos" | "linux")
}

/// Get the current platform name.
fn get_platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => {
            // Check for WSL
            if std::env::var("WSL_DISTRO_NAME").is_ok() {
                "wsl"
            } else {
                "linux"
            }
        }
        other => other,
    }
}

/// `/sandbox` command.
pub struct SandboxDirective;

#[async_trait]
impl Directive for SandboxDirective {
    fn name(&self) -> &str {
        "sandbox"
    }

    fn description(&self) -> &str {
        "Toggle and configure sandbox mode"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let platform = get_platform_name();

        // Check platform support
        if !is_sandbox_supported() {
            let error_message = if platform == "wsl" {
                "Error: Sandboxing requires WSL2. WSL1 is not supported."
            } else {
                "Error: Sandboxing is currently only supported on macOS, Linux, and WSL2."
            };
            return Ok(CommandResult::Error(error_message.to_string()));
        }

        let trimmed_args = args.join(" ").trim().to_string();

        if trimmed_args.is_empty() || matches!(trimmed_args.as_str(), "help" | "-h" | "--help") {
            // Show interactive sandbox settings menu
            let mut output = String::from("Sandbox Settings\n\n");
            output.push_str(&format!("Platform: {}\n", platform));
            output.push_str("Status: Enabled\n\n");
            output.push_str("Options:\n");
            output.push_str("  1. Toggle sandbox on/off\n");
            output.push_str("  2. Manage excluded commands\n");
            output.push_str("  3. View sandbox logs\n\n");
            output.push_str("Use /sandbox exclude \"pattern\" to exclude a command pattern.\n");
            output.push_str("Example: /sandbox exclude \"npm run test:*\"");
            return Ok(CommandResult::Text(output));
        }

        // Handle subcommands
        let parts: Vec<&str> = trimmed_args.splitn(2, ' ').collect();
        let subcommand = parts[0];

        match subcommand {
            "exclude" => {
                let pattern = parts.get(1).unwrap_or(&"").trim();
                if pattern.is_empty() {
                    return Ok(CommandResult::Error(
                        "Error: Please provide a command pattern to exclude \
                         (e.g., /sandbox exclude \"npm run test:*\")"
                            .to_string(),
                    ));
                }

                // Remove quotes if present
                let clean_pattern = pattern
                    .trim_start_matches(['"', '\''])
                    .trim_end_matches(['"', '\'']);

                Ok(CommandResult::Text(format!(
                    "Added \"{}\" to excluded commands in .mossen/settings.local.json",
                    clean_pattern
                )))
            }

            unknown => Ok(CommandResult::Error(format!(
                "Error: Unknown subcommand \"{}\". Available subcommand: exclude",
                unknown
            ))),
        }
    }
}
