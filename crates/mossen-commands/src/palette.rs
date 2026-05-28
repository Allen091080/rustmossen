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
    (
        "dark-high-contrast",
        "Dark High Contrast — enhanced visibility",
    ),
    (
        "light-high-contrast",
        "Light High Contrast — enhanced visibility",
    ),
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
            if matches!(*theme_name, "help" | "-h" | "--help") {
                let available = THEMES
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(CommandResult::Text(format!(
                    "Usage: /theme [name]\n\nAvailable themes: {}",
                    available
                )));
            }
            let lowered = theme_name.to_lowercase();
            if THEMES.iter().any(|(name, _)| *name == lowered.as_str()) {
                return Ok(CommandResult::Error(format!(
                    "Cannot set theme to {} from this command runner. Use /theme in the interactive TUI so the live renderer state is updated.",
                    lowered
                )));
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
    fn theme_directive_does_not_claim_live_renderer_update() {
        let output = tokio_test::block_on(PaletteDirective.execute(&["dark"], &test_context()))
            .expect("theme command");

        let CommandResult::Error(text) = output else {
            panic!("theme should fail closed outside the live TUI renderer");
        };
        assert!(text.contains("Cannot set theme"), "{text}");
        assert!(!text.contains("Theme set to"), "{text}");
    }
}
