//! `/privacy-settings` — Manage privacy and data collection settings.
//!
//! Controls what data is collected, stored, and shared. Allows users
//! to opt in/out of telemetry, analytics, and training data contribution.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Privacy settings command — data collection controls.
///
/// Settings:
/// - Telemetry (anonymous usage statistics)
/// - Analytics (feature usage tracking)
/// - Conversation storage (local vs. cloud)
/// - Training data contribution
pub struct PrivacyDirective;

/// Privacy setting categories.
const PRIVACY_SETTINGS: &[(&str, &str)] = &[
    ("telemetry", "Anonymous usage statistics"),
    ("analytics", "Feature usage tracking"),
    ("storage", "Conversation storage location"),
    ("training", "Training data contribution"),
];

#[async_trait]
impl Directive for PrivacyDirective {
    fn name(&self) -> &str {
        "privacy-settings"
    }

    fn description(&self) -> &str {
        "Manage privacy and data collection settings"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_PRIVACY_SETTINGS_COMMAND")
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            let mut output = String::from("Privacy Settings\n================\n\n");
            for (setting, desc) in PRIVACY_SETTINGS {
                output.push_str(&format!("  {:12} {} [local default]\n", setting, desc));
            }
            output.push_str(
                "\nThis local build does not persist privacy setting changes from this command.",
            );
            return Ok(CommandResult::Text(output));
        }

        let setting = args[0].to_lowercase();
        if matches!(setting.as_str(), "help" | "-h" | "--help") {
            let mut help =
                String::from("Usage: /privacy-settings [setting] [on|off]\n\nSettings:\n");
            for (name, desc) in PRIVACY_SETTINGS {
                help.push_str(&format!("  {:12} {}\n", name, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        let valid_settings: Vec<&str> = PRIVACY_SETTINGS.iter().map(|(s, _)| *s).collect();
        if !valid_settings.contains(&setting.as_str()) {
            return Ok(CommandResult::Error(format!(
                "Unknown setting: \"{}\". Use /privacy-settings help.",
                setting
            )));
        }

        let value = args.get(1).map(|v| v.to_lowercase());
        match value.as_deref() {
            Some("on" | "enable" | "true") | Some("off" | "disable" | "false") => {
                Ok(CommandResult::Error(format!(
                    "Cannot change privacy setting \"{}\" from this command runner; no persistent privacy settings store is wired for this local build.",
                    setting
                )))
            }
            None => Ok(CommandResult::Text(format!(
                "{}: local default (no persisted override recorded by this command)",
                setting
            ))),
            Some(v) => Ok(CommandResult::Error(format!(
                "Invalid value: \"{}\". Use on/off.",
                v
            ))),
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
    fn privacy_does_not_claim_to_persist_unwired_settings() {
        let output =
            tokio_test::block_on(PrivacyDirective.execute(&["telemetry", "off"], &test_context()))
                .expect("privacy command");

        let CommandResult::Error(text) = output else {
            panic!("privacy setting changes should fail closed when persistence is not wired");
        };
        assert!(text.contains("Cannot change privacy setting"), "{text}");
        assert!(!text.contains("disabled."), "{text}");
    }
}
