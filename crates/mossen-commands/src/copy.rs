//! `/copy` — Copy assistant responses or the transcript to clipboard.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Copy directive metadata.
///
/// The interactive TUI owns the real implementation because it has the live
/// transcript and platform clipboard writer. The generic runner has neither.
pub struct CopyDirective;

/// Maximum number of past assistant messages to look back.
const MAX_LOOKBACK: usize = 20;

/// Supported file extensions for code blocks.
fn file_extension(lang: Option<&str>) -> &str {
    match lang {
        Some(l) if !l.is_empty() && l != "plaintext" => {
            // The direct runner has no code-block exporter; keep the helper
            // conservative until a transcript-backed picker calls it.
            ".txt"
        }
        _ => ".txt",
    }
}

/// Parse the /copy argument — an optional number N indicating which response
/// to copy (1 = latest, 2 = second-to-latest, etc.). `transcript` and `all`
/// are handled by the interactive TUI because only it owns transcript state.
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
            "Usage: /copy [N|transcript|all] where N is 1 (latest), 2, 3, ... Got: {}",
            trimmed
        )),
        Err(_) => Err(format!(
            "Usage: /copy [N|transcript|all] where N is 1 (latest), 2, 3, ... Got: {}",
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
        "Copy assistant response or transcript to clipboard"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[N|transcript|all]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|arg| matches!(*arg, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Text(
                "Usage: /copy [N|transcript|all]\n\nCopies an assistant response or the full transcript in the interactive TUI. This command runner has no transcript snapshot or clipboard writer attached."
                    .to_string(),
            ));
        }

        if args
            .first()
            .map(|arg| matches!(*arg, "transcript" | "all"))
            .unwrap_or(false)
        {
            return Ok(CommandResult::Error(
                "Cannot copy the transcript from this command runner. No transcript snapshot or clipboard writer is attached."
                    .to_string(),
            ));
        }

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

        if args.is_empty() {
            Ok(CommandResult::Text(
                "Use /copy in the interactive TUI to copy the latest assistant response. This command runner has no transcript snapshot or clipboard writer attached."
                    .to_string(),
            ))
        } else {
            Ok(CommandResult::Error(format!(
                "Cannot copy assistant response #{} from this command runner. No transcript snapshot or clipboard writer is attached.",
                age + 1
            )))
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
    fn copy_directive_does_not_claim_clipboard_write_without_tui_state() {
        let output = tokio_test::block_on(CopyDirective.execute(&["2"], &test_context()))
            .expect("copy command");

        let CommandResult::Error(text) = output else {
            panic!("copy should fail closed without transcript and clipboard state");
        };
        assert!(text.contains("Cannot copy assistant response #2"), "{text}");
        assert!(!text.contains("Copied latest assistant response"), "{text}");
        assert!(!text.contains("to clipboard..."), "{text}");
    }
}
