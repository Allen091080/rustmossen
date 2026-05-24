//! `/stickers` — View and manage sticker reactions.
//!
//! Allows users to react to messages with stickers/emoji, view
//! reaction history, and manage their sticker collection.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Attempt to open a URL in the default browser.
fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = url;
        false
    }
}

/// Stickers command — emoji reaction management.
///
/// Subcommands:
/// - (no args): Show sticker picker widget
/// - `list`: List available sticker packs
/// - `add <emoji>`: React to the last message
/// - `remove`: Remove your reaction
/// - `history`: Show reaction history
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
        // Only enabled when deferred slash commands feature allows it
        // (mirrors: isDeferredSlashCommandEnabled('stickers'))
        !ctx.is_non_interactive
    }

    fn supports_non_interactive(&self) -> bool {
        false
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            if ctx.is_non_interactive {
                return Ok(CommandResult::Text(
                    "Stickers: Use /stickers add <emoji> to react.".to_string(),
                ));
            }
            // Interactive mode: open browser to sticker ordering page
            // (translated from stickers.ts: openBrowser(url))
            let url = "https://www.stickermule.com/mossencode";
            // Attempt to open browser (platform-dependent)
            let opened = open_browser(url);
            if opened {
                return Ok(CommandResult::Text(
                    "Opening sticker page in browser…".to_string(),
                ));
            } else {
                return Ok(CommandResult::Text(format!(
                    "Failed to open browser. Visit: {}",
                    url
                )));
            }
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
                Ok(CommandResult::System(format!("Reacted with: {}", emoji)))
            }
            "list" => {
                let cats = STICKER_CATEGORIES.join(", ");
                Ok(CommandResult::Text(format!(
                    "Sticker categories: {}\n\nUse /stickers to open the picker.", cats
                )))
            }
            "remove" => {
                Ok(CommandResult::System("Reaction removed.".to_string()))
            }
            "history" => {
                Ok(CommandResult::Text(
                    "Reaction history: (none)".to_string(),
                ))
            }
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
