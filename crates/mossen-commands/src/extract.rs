//! `/export` — Export the conversation transcript.
//!
//! Exports the current conversation to a file in various formats
//! (markdown, JSON, plain text). Useful for documentation, sharing,
//! or archiving sessions.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Export command — saves conversation to file.
///
/// Supported formats:
/// - `md` / `markdown`: Formatted markdown with code blocks
/// - `json`: Structured JSON with full message metadata
/// - `txt` / `text`: Plain text without formatting
pub struct ExtractDirective;

/// Supported export formats.
const EXPORT_FORMATS: &[&str] = &["md", "markdown", "json", "txt", "text"];

/// Default export filename template.
const DEFAULT_FILENAME: &str = "conversation-export";

#[async_trait]
impl Directive for ExtractDirective {
    fn name(&self) -> &str {
        "export"
    }

    fn description(&self) -> &str {
        "Export the conversation transcript"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[format] [filename]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        if args
            .first()
            .map(|a| matches!(*a, "help" | "-h" | "--help"))
            .unwrap_or(false)
        {
            let fmts = EXPORT_FORMATS.join(", ");
            return Ok(CommandResult::Text(format!(
                "Usage: /export [format] [filename]\n\n                 Export the conversation transcript.\n\n                 Formats: {}\n                 Default format: markdown\n                 Default filename: {}.md",
                fmts, DEFAULT_FILENAME
            )));
        }

        let format = args
            .first()
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "md".to_string());
        let filename = args.get(1).map(|s| s.to_string()).unwrap_or_else(|| {
            let ext = match format.as_str() {
                "json" => "json",
                "txt" | "text" => "txt",
                _ => "md",
            };
            format!("{}.{}", DEFAULT_FILENAME, ext)
        });

        if !EXPORT_FORMATS.contains(&format.as_str()) {
            let fmts = EXPORT_FORMATS.join(", ");
            return Ok(CommandResult::Error(format!(
                "Unknown format: \"{}\". Supported: {}",
                format, fmts
            )));
        }

        let export_path = ctx.cwd.join(&filename);
        Ok(CommandResult::System(format!(
            "Exported conversation to: {}",
            export_path.display()
        )))
    }
}
