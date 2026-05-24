//! `/model` — Multi-profile model list and session-level switching.
//!
//! Translates `commands/model/model.tsx` (175 lines).
//! Lists all configured model profiles, shows current/default,
//! and switches the active session profile by name.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// A model option entry shown by `/model`.
#[derive(Debug, Clone)]
struct ModelEntry {
    name: String,
    value: String,
    model: String,
    source: String,
    is_current: bool,
    is_default: bool,
}

/// Format the full profile list output.
fn format_model_list(models: &[ModelEntry], cli_name: &str) -> String {
    if models.is_empty() {
        return format!(
            "No model options are available.\n\n\
             Set a literal session model with:\n\
             \x20 /model <model-id>\n\n\
             Or start {} with --model <model-id>.",
            cli_name
        );
    }

    let mut lines = Vec::new();
    lines.push(format!("Model options ({}):", models.len()));
    lines.push(String::new());

    let mut current_name: Option<String> = None;
    let mut default_name: Option<String> = None;

    for entry in models {
        let mut tags = Vec::new();
        if entry.is_current {
            tags.push("session");
            current_name = Some(entry.name.clone());
        }
        if entry.is_default {
            tags.push("default");
            default_name = Some(entry.name.clone());
        }
        if entry.source == "fallback-env" {
            tags.push("fallback");
        }
        let tag_str = if tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", tags.join(", "))
        };

        lines.push(format!("  {}{}", entry.name, tag_str));
        lines.push(format!("    name:     {}", entry.name));
        lines.push(format!("    value:    {}", entry.value));
        lines.push(format!("    model:    {}", entry.model));
        lines.push(format!("    source:   {}", entry.source));
        lines.push(String::new());
    }

    // Current session profile
    match &current_name {
        Some(name) => lines.push(format!("Current session model: {}", name)),
        None => lines.push("Current session model: <default>".to_string()),
    }
    // Global default
    match &default_name {
        Some(name) => lines.push(format!("Default option:         {}", name)),
        None => lines.push("Default option:         <none>".to_string()),
    }

    lines.push(String::new());
    lines.push("Usage:".to_string());
    lines.push("  /model <model-id-or-alias>     Switch this session model".to_string());
    lines.push(format!(
        "  {} --model <model-id>          Start with a model",
        cli_name
    ));

    lines.join("\n")
}

fn model_entries(ctx: &CommandContext) -> Vec<ModelEntry> {
    let current_model = ctx
        .env_vars
        .get("MOSSEN_CODE_MODEL")
        .or_else(|| ctx.env_vars.get("MOSSEN_MODEL"))
        .filter(|value| !value.trim().is_empty())
        .cloned();

    mossen_utils::model_utils::get_model_options()
        .into_iter()
        .map(|option| {
            let value = option
                .value
                .clone()
                .unwrap_or_else(|| "default".to_string());
            let is_default = option.value.is_none();
            let is_current = current_model
                .as_deref()
                .map(|model| option.value.as_deref() == Some(model) || option.label == model)
                .unwrap_or(is_default);
            ModelEntry {
                name: option.label,
                value: value.clone(),
                model: option.description_for_model.unwrap_or(value),
                source: "model-options".to_string(),
                is_current,
                is_default,
            }
        })
        .collect()
}

/// Format the result of a model switch request.
fn format_switch_result(name: &str, cli_name: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Session model override requested: \"{}\".", name));
    lines.push(format!("  model: {}", name));
    lines.push(String::new());
    lines.push("In the TUI this is applied to the active session engine.".to_string());
    lines.push(format!(
        "Use `{} --model {}` when starting a new session.",
        cli_name, name
    ));
    lines.join("\n")
}

/// `/model` command.
pub struct SwitchModelDirective;

#[async_trait]
impl Directive for SwitchModelDirective {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "List model options or switch session model"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[profile-name]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let trimmed = args.join(" ").trim().to_string();
        let cli_name = &ctx.cli_name;

        if trimmed.is_empty() {
            // No args -> list all model options.
            let models = model_entries(ctx);
            return Ok(CommandResult::Text(format_model_list(&models, cli_name)));
        }

        // Split into profile name and any extra args
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let name = tokens[0];
        let rest = &tokens[1..];

        let mut output = String::new();
        if !rest.is_empty() {
            output.push_str(&format!(
                "/model: ignoring extra arguments: {}\n\n",
                rest.join(" ")
            ));
        }

        output.push_str(&format_switch_result(name, cli_name));
        Ok(CommandResult::Text(output))
    }
}
