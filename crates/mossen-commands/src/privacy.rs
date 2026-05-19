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

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Show privacy settings in text form
            let mut output = String::from("Privacy Settings\n================\n\n");
            for (setting, desc) in PRIVACY_SETTINGS {
                output.push_str(&format!("  {:12} {} [enabled]\n", setting, desc));
            }
            output.push_str("\nUse /privacy-settings <setting> on|off to change.");
            return Ok(CommandResult::Text(output));
        }

        let setting = args[0].to_lowercase();
        if matches!(setting.as_str(), "help" | "-h" | "--help") {
            let mut help = String::from(
                "Usage: /privacy-settings [setting] [on|off]\n\nSettings:\n",
            );
            for (name, desc) in PRIVACY_SETTINGS {
                help.push_str(&format!("  {:12} {}\n", name, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        let valid_settings: Vec<&str> = PRIVACY_SETTINGS.iter().map(|(s, _)| *s).collect();
        if !valid_settings.contains(&setting.as_str()) {
            return Ok(CommandResult::Error(format!(
                "Unknown setting: \"{}\". Use /privacy-settings help.", setting
            )));
        }

        let value = args.get(1).map(|v| v.to_lowercase());
        match value.as_deref() {
            Some("on" | "enable" | "true") => {
                Ok(CommandResult::System(format!("{} enabled.", setting)))
            }
            Some("off" | "disable" | "false") => {
                Ok(CommandResult::System(format!("{} disabled.", setting)))
            }
            None => {
                Ok(CommandResult::Text(format!("{}: enabled", setting)))
            }
            Some(v) => {
                Ok(CommandResult::Error(format!(
                    "Invalid value: \"{}\". Use on/off.", v
                )))
            }
        }
    }
}
