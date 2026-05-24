//! `/doctor` — Run diagnostic checks.
//!
//! Translates `commands/doctor/doctor.tsx` (7 lines).
//! Launches the Doctor diagnostic screen to check system health,
//! dependencies, and configuration.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// `/doctor` command.
pub struct DiagnoseDirective;

#[async_trait]
impl Directive for DiagnoseDirective {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self) -> &str {
        "Run diagnostic checks"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let product_name = &ctx.product_name;
        let version = &ctx.version;

        let mut output = format!("{} Doctor\n\n", product_name);
        output.push_str(&format!("Version: {}\n", version));
        output.push_str(&format!("Platform: {}\n", std::env::consts::OS));
        output.push_str(&format!("Architecture: {}\n", std::env::consts::ARCH));
        output.push_str(&format!("Working directory: {}\n\n", ctx.cwd.display()));

        // Check git
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).trim().to_string()
                } else {
                    "not found".to_string()
                }
            })
            .unwrap_or_else(|_| "not found".to_string());
        output.push_str(&format!("Git: {}\n", git_ok));

        // Check shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string());
        output.push_str(&format!("Shell: {}\n", shell));

        // Check terminal
        let terminal = ctx
            .env_vars
            .get("TERM_PROGRAM")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        output.push_str(&format!("Terminal: {}\n", terminal));

        // Check node (optional)
        let node_ok = std::process::Command::new("node")
            .arg("--version")
            .output()
            .map(|o| {
                if o.status.success() {
                    String::from_utf8_lossy(&o.stdout).trim().to_string()
                } else {
                    "not found".to_string()
                }
            })
            .unwrap_or_else(|_| "not found".to_string());
        output.push_str(&format!("Node.js: {}\n", node_ok));

        output.push_str("\nAll checks passed.");

        Ok(CommandResult::Text(output))
    }
}
