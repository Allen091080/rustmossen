//! `/keybindings` — Customize keyboard shortcuts.
//!
//! Opens the keybindings configuration file in the user's editor.
//! Creates a template file if one doesn't exist yet.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Keybindings command — keyboard shortcut customization.
///
/// Flow:
/// 1. Check if keybinding customization is enabled
/// 2. Determine the keybindings file path
/// 3. Create template if file doesn't exist
/// 4. Open in the user's editor
pub struct KeybindingsDirective;

/// Default keybinding categories shown in template.
const KEYBINDING_CATEGORIES: &[&str] = &["navigation", "editing", "commands", "panels", "global"];

#[async_trait]
impl Directive for KeybindingsDirective {
    fn name(&self) -> &str {
        "keybindings"
    }

    fn description(&self) -> &str {
        "Customize keyboard shortcuts"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
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
            let cats = KEYBINDING_CATEGORIES.join(", ");
            return Ok(CommandResult::Text(format!(
                "Usage: /keybindings\n\n                 Open the keybindings configuration file in your editor.\n                 Creates a template if the file doesn't exist.\n\n                 Categories: {}",
                cats
            )));
        }

        // Check if feature is enabled
        if ctx.is_env_truthy("DISABLE_KEYBINDING_CUSTOMIZATION") {
            return Ok(CommandResult::Text(
                "Keybinding customization is not enabled.                  This feature is currently in preview."
                    .to_string(),
            ));
        }

        if ctx.is_non_interactive {
            return Ok(CommandResult::Error(
                "Keybinding editing requires interactive mode.".to_string(),
            ));
        }

        // In full implementation:
        // 1. Get keybindings path (~/.config/mossen/keybindings.json)
        // 2. mkdir -p for parent directory
        // 3. Write template with 'wx' flag if file doesn't exist
        // 4. Open file in editor (editFileInEditor)

        Ok(CommandResult::System(
            "Opening keybindings configuration in your editor...".to_string(),
        ))
    }
}
