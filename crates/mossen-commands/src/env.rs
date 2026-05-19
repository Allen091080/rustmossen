//! `/env` — Show environment variables and session configuration.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Env directive — displays environment variables relevant to the current session,
/// including model configuration, feature flags, and backend settings.
pub struct EnvDirective;

/// Environment variable prefixes that are relevant to display.
const RELEVANT_PREFIXES: &[&str] = &[
    "MOSSEN_",
    "ANTHROPIC_",
    "OPENAI_",
    "NODE_ENV",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
];

/// Check if an environment variable should be redacted (contains secrets).
fn should_redact(key: &str) -> bool {
    let secret_patterns = ["KEY", "TOKEN", "SECRET", "PASSWORD", "CREDENTIAL"];
    secret_patterns
        .iter()
        .any(|p| key.to_uppercase().contains(p))
}

/// Redact a value, showing only first/last chars.
fn redact_value(value: &str) -> String {
    if value.len() <= 8 {
        return "****".to_string();
    }
    format!("{}...{}", &value[..3], &value[value.len() - 3..])
}

#[async_trait]
impl Directive for EnvDirective {
    fn name(&self) -> &str {
        "env"
    }

    fn description(&self) -> &str {
        "Show environment variables and session configuration"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::Local
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn supports_non_interactive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        let filter = if args.is_empty() {
            None
        } else {
            Some(args.join(" ").to_uppercase())
        };

        let mut lines: Vec<String> = Vec::new();
        lines.push("Environment:".to_string());
        lines.push("─".repeat(40));

        // Collect and sort relevant env vars
        let mut relevant_vars: Vec<(&String, &String)> = ctx
            .env_vars
            .iter()
            .filter(|(key, _)| {
                // Filter by prefix or user-provided filter
                if let Some(ref f) = filter {
                    key.to_uppercase().contains(f.as_str())
                } else {
                    RELEVANT_PREFIXES
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                }
            })
            .collect();

        relevant_vars.sort_by_key(|(k, _)| k.as_str());

        if relevant_vars.is_empty() {
            if let Some(ref f) = filter {
                lines.push(format!("No environment variables matching \"{}\"", f));
            } else {
                lines.push("No relevant environment variables found.".to_string());
            }
        } else {
            for (key, value) in &relevant_vars {
                let display_value = if should_redact(key) {
                    redact_value(value)
                } else {
                    value.to_string()
                };
                lines.push(format!("  {}={}", key, display_value));
            }
            lines.push(format!("\n({} variables shown)", relevant_vars.len()));
        }

        Ok(CommandResult::Text(lines.join("\n")))
    }
}
