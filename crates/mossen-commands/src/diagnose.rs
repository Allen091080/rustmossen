//! `/doctor` — Run diagnostic checks.
//!
//! Translates `commands/doctor/doctor.tsx` (7 lines).
//! Launches the Doctor diagnostic screen to check system health,
//! dependencies, and configuration.

use anyhow::Result;
use async_trait::async_trait;
use mossen_agent::services::config::doctor::model_config_doctor_snapshot;

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

        let model_config = model_config_doctor_snapshot();
        output.push_str("\nModel configuration:\n");
        output.push_str(&format!("Status: {}\n", model_config.status));
        if let Some(profile) = &model_config.current_profile {
            output.push_str(&format!(
                "Active profile: {} ({}, {})\n",
                profile.name,
                profile.provider.as_str(),
                profile.model
            ));
            output.push_str(&format!(
                "Credentials: baseURL {}, apiKey {}\n",
                if profile.base_url_present {
                    "present"
                } else {
                    "missing"
                },
                if profile.api_key_present {
                    "present"
                } else {
                    "missing"
                }
            ));
        } else {
            output.push_str("Active profile: none\n");
        }
        output.push_str(&format!(
            "Profiles: {} visible, {} valid settings entries",
            model_config.visible_profile_count, model_config.settings_profile_count
        ));
        if model_config.invalid_settings_profile_count > 0 {
            output.push_str(&format!(
                ", {} invalid settings entries",
                model_config.invalid_settings_profile_count
            ));
        }
        output.push('\n');
        if !model_config.issues.is_empty() {
            output.push_str("Issues:\n");
            for issue in &model_config.issues {
                output.push_str(&format!("- {}\n", describe_model_config_issue(issue)));
            }
        }
        output.push_str(&format!("Next: {}\n", model_config.next_action));
        if !model_config.next_commands.is_empty() {
            output.push_str("Commands:\n");
            for command in &model_config.next_commands {
                output.push_str(&format!("  $ {}\n", command));
            }
        }
        output.push_str("Raw config, base URLs, and API keys are redacted.\n");

        if model_config.status == "configured" && git_ok != "not found" {
            output.push_str("\nAll checks passed.");
        } else {
            output.push_str("\nDoctor completed with warnings.");
        }

        Ok(CommandResult::Text(output))
    }
}

fn describe_model_config_issue(issue: &str) -> &'static str {
    match issue {
        "profiles_not_object" => "mossen.profiles exists but is not an object",
        "no_valid_settings_profiles" => "mossen.profiles has entries, but none are valid",
        "some_settings_profiles_invalid" => {
            "some entries in mossen.profiles are invalid and hidden from /model"
        }
        "active_profile_not_found" => {
            "mossen.activeProfile points to a profile that does not exist or is invalid"
        }
        "no_model_profile" => "no model profile is configured",
        "custom_backend_env_incomplete" => "MOSSEN_CODE_CUSTOM_* environment variables are partial",
        _ => "unknown model configuration issue",
    }
}
