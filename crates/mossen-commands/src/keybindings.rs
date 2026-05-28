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

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_KEYBINDING_CUSTOMIZATION")
            && !ctx.is_env_truthy("DISABLE_KEYBINDING_CUSTOMIZATION")
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

        Ok(CommandResult::Error(
            "Cannot open keybindings from this command runner; editor launch is not wired here."
                .to_string(),
        ))
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
            is_non_interactive: false,
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
    fn keybindings_directive_does_not_claim_editor_open() {
        let output = tokio_test::block_on(KeybindingsDirective.execute(&[], &test_context()))
            .expect("keybindings command");

        let CommandResult::Error(text) = output else {
            panic!("keybindings should not claim editor launch when not wired");
        };
        assert!(text.contains("Cannot open keybindings"), "{text}");
        assert!(!text.contains("Opening keybindings"), "{text}");
    }
}
