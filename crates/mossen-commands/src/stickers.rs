//! `/stickers` — Experimental sticker reaction metadata.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Stickers command metadata.
pub struct StickersDirective;

/// Available sticker categories.
const STICKER_CATEGORIES: &[&str] = &["reactions", "celebrations", "status", "custom"];

#[async_trait]
impl Directive for StickersDirective {
    fn name(&self) -> &str {
        "stickers"
    }

    fn description(&self) -> &str {
        "Order stickers"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[add|list|remove|history]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        ctx.is_env_truthy("MOSSEN_ENABLE_STICKERS")
    }

    fn supports_non_interactive(&self) -> bool {
        false
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            return Ok(CommandResult::Text(
                "Sticker reactions are not wired into this personal build. Set MOSSEN_ENABLE_STICKERS only after adding a live reaction store."
                    .to_string(),
            ));
        }

        let subcommand = args[0].to_lowercase();
        match subcommand.as_str() {
            "add" | "react" => {
                let emoji = args.get(1).unwrap_or(&"");
                if emoji.is_empty() {
                    return Ok(CommandResult::Error(
                        "Usage: /stickers add <emoji>".to_string(),
                    ));
                }
                Ok(CommandResult::Error(format!(
                    "Cannot react with {} from this command runner. No live reaction store is attached.",
                    emoji
                )))
            }
            "list" => {
                let cats = STICKER_CATEGORIES.join(", ");
                Ok(CommandResult::Text(format!(
                    "Sticker categories: {}\n\nNo live reaction store is attached to this command runner.",
                    cats
                )))
            }
            "remove" => Ok(CommandResult::Error(
                "Cannot remove reaction from this command runner. No live reaction store is attached."
                    .to_string(),
            )),
            "history" => Ok(CommandResult::Text(
                "Reaction history is not attached to this command runner.".to_string(),
            )),
            "help" | "-h" | "--help" => {
                Ok(CommandResult::Text(
                    "Usage: /stickers [subcommand]\n\n                     Subcommands:\n                       add <emoji>    React to the last message\n                       list           List sticker categories\n                       remove         Remove your reaction\n                       history        Show reaction history"
                        .to_string(),
                ))
            }
            _ => {
                Ok(CommandResult::Error(format!(
                    "Unknown subcommand: \"{}\". Use /stickers help.", subcommand
                )))
            }
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
    fn stickers_hidden_by_default_in_personal_build() {
        assert!(!StickersDirective.is_enabled(&test_context()));
    }

    #[test]
    fn stickers_does_not_claim_reaction_without_store() {
        let output =
            tokio_test::block_on(StickersDirective.execute(&["add", "ok"], &test_context()))
                .expect("stickers command");

        let CommandResult::Error(text) = output else {
            panic!("stickers should fail closed without reaction store");
        };
        assert!(text.contains("Cannot react"), "{text}");
        assert!(!text.contains("Reacted with"), "{text}");
    }
}
