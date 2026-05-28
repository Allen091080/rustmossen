//! `/effort` — Set effort level for model usage.
//!
//! Translates `commands/effort/effort.tsx` (252 lines) and
//! `commands/effort/EffortPicker.tsx` (134 lines).
//! Manages effort level (low/medium/high/max/auto) with persistence
//! and environment variable override detection.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Valid effort levels.
const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "max"];

/// Check if a string is a valid effort level.
fn is_effort_level(s: &str) -> bool {
    EFFORT_LEVELS.contains(&s)
}

/// Get a description for an effort value.
fn get_effort_value_description(value: &str) -> &'static str {
    match value {
        "low" => "Quick, straightforward implementation",
        "medium" => "Balanced approach with standard testing",
        "high" => "Comprehensive implementation with extensive testing",
        "max" => "Maximum capability with the deepest configured reasoning tier",
        _ => "Unknown effort level",
    }
}

/// Result from an effort command operation.
struct EffortCommandResult {
    message: String,
    effort_update: Option<Option<String>>, // Some(Some(value)) = set, Some(None) = unset
}

/// Build the message for an effort value without claiming a live update.
fn set_effort_value(effort_value: &str) -> EffortCommandResult {
    // Check for environment variable override
    if let Ok(env_raw) = std::env::var("MOSSEN_CODE_EFFORT_LEVEL") {
        let env_override = env_raw.to_lowercase();
        if env_override != effort_value {
            return EffortCommandResult {
                message: format!(
                    "Cannot set effort level to {} from this command runner. MOSSEN_CODE_EFFORT_LEVEL={} currently controls this session.",
                    effort_value,
                    env_raw
                ),
                effort_update: Some(Some(effort_value.to_string())),
            };
        }
    }

    let description = get_effort_value_description(effort_value);
    let is_persistable = is_effort_level(effort_value);
    let suffix = if is_persistable {
        ""
    } else {
        " (this session only)"
    };

    EffortCommandResult {
        message: format!(
            "Cannot set effort level to {}{} from this command runner. The live model request path is not attached here. {}",
            effort_value, suffix, description
        ),
        effort_update: Some(Some(effort_value.to_string())),
    }
}

/// Show the current effort level.
fn show_current_effort() -> EffortCommandResult {
    // Check for environment variable override
    if let Ok(env_raw) = std::env::var("MOSSEN_CODE_EFFORT_LEVEL") {
        let env_value = env_raw.to_lowercase();
        if is_effort_level(&env_value) {
            let description = get_effort_value_description(&env_value);
            return EffortCommandResult {
                message: format!(
                    "Current effort level: {} ({}) [from MOSSEN_CODE_EFFORT_LEVEL]",
                    env_value, description
                ),
                effort_update: None,
            };
        }
    }

    // Default to auto
    EffortCommandResult {
        message: "Effort level: auto (currently medium)".to_string(),
        effort_update: None,
    }
}

/// Unset the effort level (revert to auto).
fn unset_effort_level() -> EffortCommandResult {
    // Check for environment variable override
    if let Ok(env_raw) = std::env::var("MOSSEN_CODE_EFFORT_LEVEL") {
        return EffortCommandResult {
            message: format!(
                "Cleared effort from settings, but MOSSEN_CODE_EFFORT_LEVEL={} still controls this session",
                env_raw
            ),
            effort_update: Some(None),
        };
    }

    EffortCommandResult {
        message: "Cannot reset effort level from this command runner. The live model request path is not attached here.".to_string(),
        effort_update: Some(None),
    }
}

/// Execute effort command with arguments.
fn execute_effort(args: &str) -> EffortCommandResult {
    let normalized = args.to_lowercase();
    if normalized == "auto" || normalized == "unset" {
        return unset_effort_level();
    }

    if !is_effort_level(&normalized) {
        return EffortCommandResult {
            message: format!(
                "Invalid argument: {}. Valid options are: low, medium, high, max, auto",
                args
            ),
            effort_update: None,
        };
    }

    set_effort_value(&normalized)
}

/// `/effort` command.
pub struct EffortDirective;

#[async_trait]
impl Directive for EffortDirective {
    fn name(&self) -> &str {
        "effort"
    }

    fn description(&self) -> &str {
        "Set effort level for model usage"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn argument_hint(&self) -> &str {
        "[low|medium|high|max|auto]"
    }

    fn is_immediate(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let args_str = args.join(" ").trim().to_string();

        // Handle help
        if matches!(args_str.as_str(), "help" | "-h" | "--help") {
            return Ok(CommandResult::Text(
                "Usage: /effort [low|medium|high|max|auto]\n\n\
                 Effort levels:\n\
                 - low: Quick, straightforward implementation\n\
                 - medium: Balanced approach with standard testing\n\
                 - high: Comprehensive implementation with extensive testing\n\
                 - max: Maximum capability with the deepest configured reasoning tier\n\
                 - auto: Use the default effort level for your model"
                    .to_string(),
            ));
        }

        // Handle current/status
        if matches!(args_str.as_str(), "current" | "status") {
            let result = show_current_effort();
            return Ok(CommandResult::Text(result.message));
        }

        // No args → show picker (interactive) or current level
        if args_str.is_empty() {
            if ctx.is_non_interactive {
                let result = show_current_effort();
                return Ok(CommandResult::Text(result.message));
            }

            // In interactive mode, show effort picker as text menu
            let current = show_current_effort();
            let mut output = "Choose effort level\nCurrently: auto\n\n".to_string();
            for level in &["auto", "low", "medium", "high", "max"] {
                let desc = if *level == "auto" {
                    "Use the model's default effort level"
                } else {
                    get_effort_value_description(level)
                };
                output.push_str(&format!("  {} — {}\n", level, desc));
            }
            output.push_str("\nUse /effort <level> to set.");
            return Ok(CommandResult::Text(output));
        }

        // Execute with the given argument. The result is intentionally
        // fail-closed until the live TUI applies this to EngineConfig.
        let result = execute_effort(&args_str);
        if result.effort_update.is_some() {
            Ok(CommandResult::Error(result.message))
        } else {
            Ok(CommandResult::Text(result.message))
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
    fn effort_directive_does_not_claim_live_model_update() {
        let output = tokio_test::block_on(EffortDirective.execute(&["high"], &test_context()))
            .expect("effort command");

        let CommandResult::Error(text) = output else {
            panic!("effort should fail closed outside live request path");
        };
        assert!(text.contains("Cannot set effort level"), "{text}");
        assert!(!text.contains("Set effort level"), "{text}");
    }
}
