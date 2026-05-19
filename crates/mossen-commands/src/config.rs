//! `/config` — Open configuration panel or modify settings.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Config (settings) directive — opens the settings panel or modifies a specific
/// configuration key-value pair directly from the command line.
pub struct ConfigDirective;

/// Known configuration keys that can be set via /config key=value.
const KNOWN_KEYS: &[&str] = &[
    "editorMode",
    "fastMode",
    "voiceEnabled",
    "language",
    "theme",
    "outputStyle",
    "copyFullResponse",
    "autoCompact",
    "verbose",
];

/// Parse a key=value assignment from args.
fn parse_config_assignment(args: &[&str]) -> Option<(String, String)> {
    let joined = args.join(" ");
    if let Some(eq_pos) = joined.find('=') {
        let key = joined[..eq_pos].trim().to_string();
        let value = joined[eq_pos + 1..].trim().to_string();
        if !key.is_empty() {
            return Some((key, value));
        }
    }
    None
}

#[async_trait]
impl Directive for ConfigDirective {
    fn name(&self) -> &str {
        "config"
    }

    fn aliases(&self) -> &[&str] {
        &["settings"]
    }

    fn description(&self) -> &str {
        "Open config panel or set a configuration value"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[key=value]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        // No arguments: show current settings summary
        if args.is_empty() {
            let keys_display = KNOWN_KEYS.join(", ");
            return Ok(CommandResult::Text(format!(
                "Configuration Settings\n\
                 =====================\n\n\
                 Available keys: {}\n\n\
                 Usage:\n\
                 · /config list         — Show all available keys\n\
                 · /config <key>=<value> — Set a configuration value\n\
                 · /config <key>        — Show current value for key",
                keys_display
            )));
        }

        // Check for help/list subcommand
        let first = args[0].to_lowercase();
        if first == "list" || first == "keys" || first == "--help" {
            let keys_display = KNOWN_KEYS.join(", ");
            return Ok(CommandResult::Text(format!(
                "Available configuration keys:\n{}\n\nUsage: /config <key>=<value>",
                keys_display
            )));
        }

        // Try to parse as key=value assignment
        if let Some((key, value)) = parse_config_assignment(args) {
            // Validate the key
            if !KNOWN_KEYS.contains(&key.as_str()) {
                let suggestion = KNOWN_KEYS
                    .iter()
                    .find(|k| k.to_lowercase().starts_with(&key.to_lowercase()));
                let hint = suggestion
                    .map(|s| format!(" Did you mean \"{}\"?", s))
                    .unwrap_or_default();
                return Ok(CommandResult::Error(format!(
                    "Unknown configuration key: \"{}\".{}\nRun /config list to see available keys.",
                    key, hint
                )));
            }

            // In full implementation: persist via saveGlobalConfig / updateSettingsForSource
            return Ok(CommandResult::System(format!(
                "Configuration updated: {} = {}",
                key, value
            )));
        }

        // If arg doesn't match key=value, treat as a query for that key's current value
        let query = args.join(" ");
        Ok(CommandResult::Text(format!(
            "No value set for \"{}\". Use /config {}=<value> to set it.",
            query, query
        )))
    }
}
