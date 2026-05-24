//! `/advisor` — Configure advisor model (local).

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Advisor directive — set, unset, or display the advisor model.
pub struct AdvisorDirective;

/// Check if the given model supports being used as an advisor.
fn model_supports_advisor(base_model: &str) -> bool {
    // Models that support extended thinking / advisor role
    let supported_prefixes = [
        "mossen-3-5-balanced",
        "mossen-3-max",
        "mossen-balanced",
        "mossen-max",
        "gpt-4",
        "o1",
        "o3",
    ];
    supported_prefixes
        .iter()
        .any(|prefix| base_model.starts_with(prefix))
}

/// Check if the given model is a valid advisor model.
fn is_valid_advisor_model(model: &str) -> bool {
    let valid_models = [
        "max",
        "mossen-3-max",
        "mossen-max",
        "o1",
        "o1-preview",
        "o3",
        "o3-mini",
        "deepseek-r1",
    ];
    valid_models.iter().any(|m| model.starts_with(m))
}

/// Check if the current user can configure advisor.
fn can_user_configure_advisor(ctx: &CommandContext) -> bool {
    ctx.is_internal_user()
        || ctx
            .env_vars
            .get("MOSSEN_CODE_ENABLE_ADVISOR")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
}

/// Normalize model string for API usage.
fn normalize_model_string_for_api(input: &str) -> String {
    let lower = input.to_lowercase();
    match lower.as_str() {
        "max" => "mossen-3-max-20240229".to_string(),
        "balanced" => "mossen-3-5-balanced-20241022".to_string(),
        "fast" => "mossen-3-fast-20240307".to_string(),
        _ => lower,
    }
}

/// Parse user-specified model string to a resolved model identifier.
fn parse_user_specified_model(input: &str) -> String {
    normalize_model_string_for_api(input)
}

/// Get the default main loop model setting.
fn get_default_main_loop_model_setting() -> String {
    "mossen-balanced-4-20250514".to_string()
}

#[async_trait]
impl Directive for AdvisorDirective {
    fn name(&self) -> &str {
        "advisor"
    }

    fn description(&self) -> &str {
        "Configure the advisor model"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn argument_hint(&self) -> &str {
        "[<model>|off]"
    }

    fn is_enabled(&self, ctx: &CommandContext) -> bool {
        can_user_configure_advisor(ctx)
    }

    fn is_hidden(&self) -> bool {
        false
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let arg = args.join(" ").trim().to_lowercase();
        let base_model = parse_user_specified_model(
            ctx.env_vars
                .get("MAIN_LOOP_MODEL")
                .map(|s| s.as_str())
                .unwrap_or(&get_default_main_loop_model_setting()),
        );

        if arg.is_empty() {
            // Show current advisor status
            let current = ctx.env_vars.get("ADVISOR_MODEL");
            match current {
                None => {
                    return Ok(CommandResult::Text(
                        "Advisor: not set\nUse \"/advisor <model>\" to enable (e.g. \"/advisor max\").".to_string(),
                    ));
                }
                Some(v) if v.is_empty() => {
                    return Ok(CommandResult::Text(
                        "Advisor: not set\nUse \"/advisor <model>\" to enable (e.g. \"/advisor max\").".to_string(),
                    ));
                }
                Some(current_model) => {
                    if !model_supports_advisor(&base_model) {
                        return Ok(CommandResult::Text(format!(
                            "Advisor: {} (inactive)\nThe current model ({}) does not support advisors.",
                            current_model, base_model
                        )));
                    }
                    return Ok(CommandResult::Text(format!(
                        "Advisor: {}\nUse \"/advisor unset\" to disable or \"/advisor <model>\" to change.",
                        current_model
                    )));
                }
            }
        }

        if arg == "unset" || arg == "off" {
            let prev = ctx.env_vars.get("ADVISOR_MODEL");
            match prev {
                Some(prev_model) if !prev_model.is_empty() => {
                    return Ok(CommandResult::Text(format!(
                        "Advisor disabled (was {}).",
                        prev_model
                    )));
                }
                _ => {
                    return Ok(CommandResult::Text("Advisor already unset.".to_string()));
                }
            }
        }

        let normalized_model = normalize_model_string_for_api(&arg);
        let resolved_model = parse_user_specified_model(&arg);

        // Validate model exists
        if normalized_model.is_empty() {
            return Ok(CommandResult::Text(format!(
                "Unknown model: {} ({})",
                arg, resolved_model
            )));
        }

        if !is_valid_advisor_model(&resolved_model) {
            return Ok(CommandResult::Text(format!(
                "The model {} ({}) cannot be used as an advisor",
                arg, resolved_model
            )));
        }

        if !model_supports_advisor(&base_model) {
            return Ok(CommandResult::Text(format!(
                "Advisor set to {}.\nNote: Your current model ({}) does not support advisors. Switch to a supported model to use the advisor.",
                normalized_model, base_model
            )));
        }

        Ok(CommandResult::Text(format!(
            "Advisor set to {}.",
            normalized_model
        )))
    }
}
