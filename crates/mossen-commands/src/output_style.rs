//! `/output-style` — Configure response output formatting.
//!
//! Controls how the model's responses are displayed, including
//! markdown rendering, code block formatting, and verbosity level.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Output style command — response formatting control.
///
/// Styles:
/// - `concise`: Shorter responses, less explanation
/// - `detailed`: Full explanations with examples
/// - `code-only`: Minimize prose, focus on code
/// - `markdown`: Rich markdown formatting (default)
/// - `plain`: No formatting, plain text output
pub struct OutputStyleDirective;

/// Available output styles.
const OUTPUT_STYLES: &[(&str, &str)] = &[
    ("concise", "Shorter responses with less explanation"),
    ("detailed", "Full explanations with examples"),
    ("code-only", "Minimize prose, focus on code output"),
    ("markdown", "Rich markdown formatting (default)"),
    ("plain", "No formatting, plain text output"),
];

#[async_trait]
impl Directive for OutputStyleDirective {
    fn name(&self) -> &str {
        "output-style"
    }

    fn description(&self) -> &str {
        "Configure response output formatting"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[concise|detailed|code-only|markdown|plain]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<CommandResult> {
        if args.is_empty() {
            let mut help = String::from("Current output style: markdown\n\nAvailable styles:\n");
            for (style, desc) in OUTPUT_STYLES {
                help.push_str(&format!("  {:12} {}\n", style, desc));
            }
            help.push_str("\nUsage: /output-style <style>");
            return Ok(CommandResult::Text(help));
        }

        let style = args[0].to_lowercase();
        let valid_styles: Vec<&str> = OUTPUT_STYLES.iter().map(|(s, _)| *s).collect();

        if matches!(style.as_str(), "help" | "-h" | "--help") {
            let mut help = String::from("Usage: /output-style <style>\n\nStyles:\n");
            for (s, desc) in OUTPUT_STYLES {
                help.push_str(&format!("  {:12} {}\n", s, desc));
            }
            return Ok(CommandResult::Text(help));
        }

        if !valid_styles.contains(&style.as_str()) {
            let styles = valid_styles.join(", ");
            return Ok(CommandResult::Error(format!(
                "Unknown style: \"{}\". Available: {}", style, styles
            )));
        }

        Ok(CommandResult::System(format!(
            "Output style set to: {}", style
        )))
    }
}
