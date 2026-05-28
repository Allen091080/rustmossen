//! `/tag` — Tag a message for easy reference.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Tag directive metadata.
///
/// Real tags require a live transcript cursor and writer.
pub struct TagDirective;

/// Maximum length for a tag name.
const MAX_TAG_LENGTH: usize = 50;

/// Validate a tag name.
fn validate_tag(tag: &str) -> Result<String, String> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err("Tag name cannot be empty. Usage: /tag <name>".to_string());
    }
    if trimmed.len() > MAX_TAG_LENGTH {
        return Err(format!(
            "Tag name too long ({} chars). Maximum is {} characters.",
            trimmed.len(),
            MAX_TAG_LENGTH
        ));
    }
    // Tags should be simple identifiers: alphanumeric + hyphens/underscores
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(
            "Tag name can only contain letters, numbers, hyphens, underscores, and spaces."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

#[async_trait]
impl Directive for TagDirective {
    fn name(&self) -> &str {
        "tag"
    }

    fn aliases(&self) -> &[&str] {
        &["bookmark"]
    }

    fn description(&self) -> &str {
        "Tag a point in the conversation for easy reference"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_hidden(&self) -> bool {
        true
    }

    fn argument_hint(&self) -> &str {
        "<name>"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            // Show existing tags or usage hint
            return Ok(CommandResult::System(
                "Usage: /tag <name> — Tag the current point in conversation.\nUse /tag list to see all tags."
                    .to_string(),
            ));
        }

        let raw = args.join(" ");

        if raw.trim().eq_ignore_ascii_case("list") {
            return Ok(CommandResult::Text(
                "Tag storage is not attached to this command runner.".to_string(),
            ));
        }

        // Validate and create the tag
        let tag_name = match validate_tag(&raw) {
            Ok(t) => t,
            Err(e) => return Ok(CommandResult::Error(e)),
        };

        Ok(CommandResult::Error(format!(
            "Cannot tag current position as \"{}\" from this command runner. No live transcript cursor or writer is attached.",
            tag_name
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
    fn tag_directive_does_not_claim_transcript_bookmark_without_writer() {
        let output = tokio_test::block_on(TagDirective.execute(&["mark"], &test_context()))
            .expect("tag command");

        let CommandResult::Error(text) = output else {
            panic!("tag should fail closed without transcript writer");
        };
        assert!(text.contains("Cannot tag current position"), "{text}");
        assert!(!text.contains("Tagged current position"), "{text}");
    }
}
