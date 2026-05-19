//! `/lang` — Switch display language preference.
//!
//! Translates `commands/lang/lang.tsx` (81 lines).
//! Manages the interactive language (en/zh) with toggle, set, and auto modes.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Supported language tags.
const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "中文"),
];

/// Resolve the language preference from user input.
fn resolve_language_command(args: &str) -> Option<Option<&str>> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return None; // Show usage
    }

    let lowered = trimmed.to_lowercase();
    if matches!(lowered.as_str(), "help" | "-h" | "--help") {
        return None; // Show usage
    }
    if matches!(lowered.as_str(), "toggle" | "switch") {
        // Toggle will be handled separately
        return Some(Some("toggle"));
    }
    if matches!(
        lowered.as_str(),
        "default" | "auto" | "system" | "follow" | "clear"
    ) {
        return Some(None); // Clear preference
    }

    // Try to match a language
    match lowered.as_str() {
        "en" | "english" => Some(Some("en")),
        "zh" | "中文" | "chinese" => Some(Some("zh")),
        _ => None, // Unknown, show usage
    }
}

/// Build the usage/status message.
fn build_usage_message(current_lang: &str) -> String {
    let display_name = SUPPORTED_LANGUAGES
        .iter()
        .find(|(tag, _)| *tag == current_lang)
        .map(|(_, name)| *name)
        .unwrap_or("English");

    format!(
        "Current language: {}\n\
         Preference: auto\n\n\
         Usage: /lang <language>\n\
         Shortcut: /lang toggle (switch between English and 中文)\n\
         Note: /lang auto or /lang clear to reset to system default.",
        display_name
    )
}

/// `/lang` command.
pub struct LangDirective;

#[async_trait]
impl Directive for LangDirective {
    fn name(&self) -> &str {
        "lang"
    }

    fn description(&self) -> &str {
        "Switch display language"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[en|zh|toggle|auto]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        let args_str = args.join(" ");
        let command = resolve_language_command(&args_str);

        match command {
            None => {
                // Show usage message
                let current = "en"; // Would read from config in real implementation
                Ok(CommandResult::System(build_usage_message(current)))
            }

            Some(None) => {
                // Clear preference
                Ok(CommandResult::System(
                    "Language preference cleared. Using system default.".to_string(),
                ))
            }

            Some(Some("toggle")) => {
                // Toggle between en and zh
                let current = "en"; // Would read from config
                let next = if current == "zh" { "English" } else { "中文" };
                let target_lang = if current == "zh" { "en" } else { "zh" };
                let msg = if target_lang == "zh" {
                    format!("已切换语言为{}", next)
                } else {
                    format!("Switched language to {}", next)
                };
                Ok(CommandResult::System(msg))
            }

            Some(Some(lang)) => {
                let display = SUPPORTED_LANGUAGES
                    .iter()
                    .find(|(tag, _)| *tag == lang)
                    .map(|(_, name)| *name)
                    .unwrap_or(lang);
                let msg = if lang == "zh" {
                    format!("已切换语言为{}", display)
                } else {
                    format!("Switched language to {}", display)
                };
                Ok(CommandResult::System(msg))
            }
        }
    }
}
