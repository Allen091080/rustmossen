//! `/help` — Show available commands and usage information.
//!
//! Displays the list of all available slash commands, their descriptions,
//! and usage hints. Can also show detailed help for a specific command.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Help/Guide command — displays command reference.
///
/// Without arguments: shows all available commands grouped by category.
/// With a command name: shows detailed help for that specific command.
pub struct GuideDirective;

#[async_trait]
impl Directive for GuideDirective {
    fn name(&self) -> &str {
        "help"
    }

    fn aliases(&self) -> &[&str] {
        &["?"]
    }

    fn description(&self) -> &str {
        "Show available commands and usage information"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[command-name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let directives = crate::all_directives();

        // Specific command help
        if let Some(cmd_name) = args.first() {
            let name = cmd_name.trim_start_matches('/');
            if let Some(directive) = crate::find_directive(&directives, name)
                .filter(|directive| directive.is_enabled(ctx) && !directive.is_hidden())
            {
                return Ok(CommandResult::Text(format_command_help(directive)));
            }

            return Ok(CommandResult::Error(format!(
                "Unknown command: /{name}\nRun /help to list available commands."
            )));
        }

        // General help — list all commands
        let mut output = format!("{} — Available Commands\n", ctx.product_name);
        output.push_str(&"=".repeat(40));
        output.push_str("\n\n");
        output.push_str("Use /help <command> for detailed information about a command.\n\n");

        let mut visible = crate::visible_directives(&directives, ctx);
        visible.sort_by(|a, b| a.name().cmp(b.name()));

        let mut current_type: Option<DirectiveType> = None;
        for directive in visible {
            let dtype = directive.directive_type();
            if current_type != Some(dtype) {
                if current_type.is_some() {
                    output.push('\n');
                }
                current_type = Some(dtype);
                output.push_str(directive_type_label(dtype));
                output.push_str(" commands:\n");
            }
            output.push_str("  ");
            output.push_str(&format_usage(directive));
            output.push_str(" — ");
            output.push_str(directive.description());
            output.push('\n');
        }

        Ok(CommandResult::Text(output))
    }
}

fn format_command_help(directive: &dyn Directive) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Help for /{}", directive.name()));
    lines.push(String::new());
    lines.push(format!("Usage: {}", format_usage(directive)));
    lines.push(format!("Description: {}", directive.description()));
    lines.push(format!(
        "Type: {}",
        directive_type_label(directive.directive_type())
    ));
    if !directive.aliases().is_empty() {
        lines.push(format!(
            "Aliases: {}",
            directive
                .aliases()
                .iter()
                .map(|alias| format!("/{alias}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines.push(format!(
        "Immediate: {}",
        if directive.is_immediate() {
            "yes"
        } else {
            "no"
        }
    ));
    lines.push(format!(
        "Non-interactive: {}",
        if directive.supports_non_interactive() {
            "supported"
        } else {
            "interactive only"
        }
    ));
    lines.join("\n")
}

fn format_usage(directive: &dyn Directive) -> String {
    let hint = directive.argument_hint().trim();
    if hint.is_empty() {
        format!("/{}", directive.name())
    } else {
        format!("/{} {}", directive.name(), hint)
    }
}

fn directive_type_label(dtype: DirectiveType) -> &'static str {
    match dtype {
        DirectiveType::Local => "Local",
        DirectiveType::LocalWidget => "Widget",
        DirectiveType::Prompt => "Prompt",
    }
}
