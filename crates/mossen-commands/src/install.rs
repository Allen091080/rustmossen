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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_NATIVE_INSTALLER_COMMAND")
    }

    fn argument_hint(&self) -> &str {
        "[latest|stable|version] [--force]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /install status\n\nNative packaging is not wired into this source checkout command runner."
                    .to_string(),
            ));
        }

        let parsed = parse_install_args(args);
        let product_name = &ctx.product_name;
        let cli_name = &ctx.cli_name;
        let channel_or_version = parsed.target.as_deref().unwrap_or("latest");

        if matches!(channel_or_version, "status" | "current" | "show") {
            return Ok(CommandResult::Text(format!(
                "Install status\nProduct: {}\nExpected user install path: {}\nNative packaging is not wired into this source checkout command runner.",
                product_name,
                get_installation_path(cli_name)
            )));
        }

        Ok(CommandResult::Error(format!(
            "Cannot install {} {} from this command runner; native packaging is not wired in this source checkout.",
            product_name, channel_or_version
        )))
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
    fn install_directive_does_not_claim_native_install_success() {
        let output = tokio_test::block_on(InstallDirective.execute(&["latest"], &test_context()))
            .expect("install command");

        let CommandResult::Error(text) = output else {
            panic!("install should not claim success when packaging is not wired");
        };
        assert!(text.contains("Cannot install"), "{text}");
        assert!(!text.contains("successfully installed"), "{text}");
        assert!(!text.contains("Auto-updates channel set"), "{text}");
    }
}
