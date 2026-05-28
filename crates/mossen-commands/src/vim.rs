//! `/vim` — Toggle between Vim and Normal editing modes.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive};

/// Vim directive — toggle between Vim and Normal editing modes.
pub struct VimDirective;

/// Get the current editor mode from config.
fn get_current_editor_mode(ctx: &CommandContext) -> String {
    ctx.env_vars
        .get("MOSSEN_EDITOR_MODE")
        .cloned()
        .unwrap_or_else(|| "normal".to_string())
}

/// Normalize backward-compatible mode values.
fn normalize_mode(mode: &str) -> &str {
    // Handle backward compatibility - treat 'emacs' as 'normal'
    match mode {
        "emacs" => "normal",
        other => other,
    }
}

#[async_trait]
impl Directive for VimDirective {
    fn name(&self) -> &str {
        "vim"
    }

    fn description(&self) -> &str {
        "Toggle between Vim and Normal editing modes"
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let raw_mode = get_current_editor_mode(ctx);
        let current_mode = normalize_mode(&raw_mode);
        if args
            .first()
            .map(|arg| matches!(*arg, "status" | "show" | "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(format!(
                "Current editor mode: {current_mode}\nUsage: /vim to toggle between normal and vim modes."
            )));
        }

        let new_mode = if current_mode == "normal" {
            "vim"
        } else {
            "normal"
        };

        mossen_utils::config::save_global_config(|current| {
            let mut next = current.clone();
            next.editor_mode = Some(new_mode.to_string());
            next
        });

        let message = if new_mode == "vim" {
            format!(
                "Editor mode set to {}. Use Escape key to toggle between INSERT and NORMAL modes.",
                new_mode
            )
        } else {
            format!(
                "Editor mode set to {}. Using standard (readline) keyboard bindings.",
                new_mode
            )
        };

        Ok(CommandResult::Text(message))
    }
}
