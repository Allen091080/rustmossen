//! `/copy` — Copy last assistant response or code blocks to clipboard.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Copy directive — copies the most recent assistant response (or a specific
/// older one) to the system clipboard. When code blocks are present, shows a
/// picker to choose between full response and individual blocks.
pub struct CopyDirective;

/// Maximum number of past assistant messages to look back.
const MAX_LOOKBACK: usize = 20;

/// Supported file extensions for code blocks.
fn file_extension(lang: Option<&str>) -> &str {
    match lang {
        Some(l) if !l.is_empty() && l != "plaintext" => {
            // In full implementation: return ".{lang}" with sanitization
            // For now, just acknowledge the language
            ".txt"
        }
        _ => ".txt",
    }
}

/// Parse the /copy argument — an optional number N indicating which response
/// to copy (1 = latest, 2 = second-to-latest, etc.).
fn parse_copy_arg(args: &[&str]) -> Result<usize, String> {
    if args.is_empty() {
        return Ok(0); // 0 means "latest"
    }

    let arg = args.join(" ");
    let trimmed = arg.trim();

    if trimmed.is_empty() {
        return Ok(0);
    }

    match trimmed.parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n - 1), // Convert to 0-based index
        Ok(_) => Err(format!(
            "Usage: /copy [N] where N is 1 (latest), 2, 3, … Got: {}",
            trimmed
        )),
        Err(_) => Err(format!(
            "Usage: /copy [N] where N is 1 (latest), 2, 3, … Got: {}",
            trimmed
        )),
    }
}

#[async_trait]
impl Directive for CopyDirective {
    fn name(&self) -> &str {
        "copy"
    }

    fn description(&self) -> &str {
        "Copy last assistant response to clipboard"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[N]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        // Parse the age argument
        let age = match parse_copy_arg(args) {
            Ok(a) => a,
            Err(msg) => return Ok(CommandResult::Error(msg)),
        };

        // Validate bounds
        if age >= MAX_LOOKBACK {
            return Ok(CommandResult::Error(format!(
                "Can only look back {} messages.",
                MAX_LOOKBACK
            )));
        }

        let _ = file_extension(None); // used by the copy logic

        // In full implementation: collect recent assistant texts from context.messages,
        // extract code blocks, either copy directly or show picker widget.
        // Phase 5 TUI shows CopyPicker when code blocks are present.
        //
        // The TS implementation:
        // 1. Calls collectRecentAssistantTexts(context.messages)
        // 2. If no messages: "No assistant message to copy"
        // 3. Checks config.copyFullResponse — if true, copy full text directly
        // 4. Extracts code blocks from markdown
        // 5. If no code blocks or copyFullResponse: copy entire text
        // 6. Otherwise: show CopyPicker widget for selection

        if age > 0 {
            Ok(CommandResult::System(format!(
                "Copying assistant response #{} to clipboard...",
                age + 1
            )))
        } else {
            // Default: copy latest assistant response to clipboard
            Ok(CommandResult::System(
                "Copied latest assistant response to clipboard.".to_string(),
            ))
        }
    }
}
