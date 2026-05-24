//! `/config` — Open configuration panel or modify settings.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Config (settings) directive — opens the settings panel or modifies a specific
/// configuration key-value pair directly from the command line.
pub struct ConfigDirective;

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

fn known_keys_display() -> String {
    mossen_utils::config::GLOBAL_CONFIG_KEYS.join(", ")
}

fn parse_config_value(value: &str) -> JsonValue {
    serde_json::from_str::<JsonValue>(value)
        .unwrap_or_else(|_| JsonValue::String(value.to_string()))
}

fn read_config_value(key: &str) -> Option<JsonValue> {
    let config = mossen_utils::config::get_global_config();
    serde_json::to_value(config).ok()?.get(key).cloned()
}

fn persist_config_value(key: String, value: JsonValue) {
    mossen_utils::config::save_global_config(move |current| {
        let mut as_json = serde_json::to_value(current).unwrap_or(JsonValue::Null);
        if let Some(object) = as_json.as_object_mut() {
            object.insert(key.clone(), value.clone());
        }
        serde_json::from_value(as_json).unwrap_or_else(|_| current.clone())
    });
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
            let keys_display = known_keys_display();
            return Ok(CommandResult::Text(format!(
                "Configuration Settings\n\
                 =====================\n\n\
                 Path: {}\n\n\
                 Available keys: {}\n\n\
                 Usage:\n\
                 · /config list         — Show all available keys\n\
                 · /config <key>=<value> — Set a configuration value\n\
                 · /config <key>        — Show current value for key",
                mossen_utils::config::get_global_mossen_file(),
                keys_display
            )));
        }

        // Check for help/list subcommand
        let first = args[0].to_lowercase();
        if first == "list" || first == "keys" || first == "--help" {
            let keys_display = known_keys_display();
            return Ok(CommandResult::Text(format!(
                "Available configuration keys:\n{}\n\nUsage: /config <key>=<value>",
                keys_display
            )));
        }

        // Try to parse as key=value assignment
        if let Some((key, value)) = parse_config_assignment(args) {
            // Validate the key
            if !mossen_utils::config::is_global_config_key(&key) {
                let suggestion = mossen_utils::config::GLOBAL_CONFIG_KEYS
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

            let parsed_value = parse_config_value(&value);
            persist_config_value(key.clone(), parsed_value.clone());
            return Ok(CommandResult::System(format!(
                "Configuration updated: {} = {}",
                key, parsed_value
            )));
        }

        // If arg doesn't match key=value, treat as a query for that key's current value
        let query = args.join(" ");
        if !mossen_utils::config::is_global_config_key(&query) {
            return Ok(CommandResult::Error(format!(
                "Unknown configuration key: \"{}\".\nRun /config list to see available keys.",
                query
            )));
        }

        let value = read_config_value(&query).unwrap_or(JsonValue::Null);
        Ok(CommandResult::Text(format!("{} = {}", query, value)))
    }
}
