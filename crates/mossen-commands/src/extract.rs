//! `/export` — Export the conversation transcript.
//!
//! Exports the current conversation to a file in various formats
//! (markdown, JSON, plain text). Useful for documentation, sharing,
//! or archiving sessions.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Export command metadata.
///
/// A real export needs the live transcript snapshot. The generic command
/// runner cannot write a meaningful transcript file by itself.
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

        if args.is_empty() {
            let fmts = EXPORT_FORMATS.join(", ");
            return Ok(CommandResult::Text(format!(
                "Usage: /export [format] [filename]\n\nExport requires a live transcript snapshot. Supported formats: {}",
                fmts
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
        Ok(CommandResult::Error(format!(
            "Cannot export conversation to {} from this command runner. No live transcript snapshot is attached, so no file was written.",
            export_path.display()
        )))
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
            cwd: PathBuf::from("/tmp"),
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
    fn export_does_not_claim_file_written_without_transcript() {
        let output = tokio_test::block_on(ExtractDirective.execute(&["md"], &test_context()))
            .expect("export command");

        let CommandResult::Error(text) = output else {
            panic!("export should fail closed without transcript");
        };
        assert!(text.contains("Cannot export conversation"), "{text}");
        assert!(!text.contains("Exported conversation"), "{text}");
    }
}
