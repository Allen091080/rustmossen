//! `/theme` — Select a color theme.
//!
//! Translates `commands/theme/theme.tsx` (57 lines).
//! Shows a theme picker to switch between available themes (light, dark, etc.).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Available themes.
const THEMES: &[(&str, &str)] = &[
    ("light", "Light theme — optimized for light terminals"),
    ("dark", "Dark theme — optimized for dark terminals"),
    ("dark-high-contrast", "Dark High Contrast — enhanced visibility"),
    ("light-high-contrast", "Light High Contrast — enhanced visibility"),
];

/// `/theme` command.
pub struct PaletteDirective;

#[async_trait]
impl Directive for PaletteDirective {
    fn name(&self) -> &str {
        "theme"
    }

    fn description(&self) -> &str {
        "Select a color theme"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if let Some(theme_name) = args.first() {
            let lowered = theme_name.to_lowercase();
            if THEMES.iter().any(|(name, _)| *name == lowered.as_str()) {
                return Ok(CommandResult::Text(format!("Theme set to {}", lowered)));
            } else {
                let available = THEMES
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(CommandResult::Error(format!(
                    "Unknown theme: \"{}\". Available themes: {}",
                    theme_name, available
                )));
            }
        }

        // No args — show theme picker
        let mut output = String::from("Select a Theme\n\n");
        for (name, desc) in THEMES {
            output.push_str(&format!("  {} — {}\n", name, desc));
        }
        output.push_str("\nUse /theme <name> to set, or select from the list above.");
        Ok(CommandResult::Text(output))
    }
}
