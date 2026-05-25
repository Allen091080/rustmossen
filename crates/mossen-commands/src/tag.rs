//! `/tag` — Tag a message for easy reference.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Tag directive — adds a named tag/bookmark to the current position in the
/// conversation, allowing quick navigation back to important points.
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

        // Special subcommand: list
        if raw.trim().eq_ignore_ascii_case("list") {
            // In full implementation: list all tags with their positions
            return Ok(CommandResult::Text(
                "No tags set in this session yet.".to_string(),
            ));
        }

        // Validate and create the tag
        let tag_name = match validate_tag(&raw) {
            Ok(t) => t,
            Err(e) => return Ok(CommandResult::Error(e)),
        };

        // In full implementation:
        // 1. Record the current message UUID as the tag target
        // 2. Persist to the transcript JSONL
        // 3. Allow /resume or navigation to jump to tagged points

        Ok(CommandResult::System(format!(
            "Tagged current position as: \"{}\"",
            tag_name
        )))
    }
}
