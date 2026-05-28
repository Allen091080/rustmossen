//! `/onboarding` — Run the first-time setup wizard.
//!
//! Guides new users through initial configuration including
//! authentication, model selection, and preference settings.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// Onboarding command — first-time setup wizard.
///
/// Steps:
/// 1. Welcome message and product overview
/// 2. Authentication setup
/// 3. Model selection
/// 4. Editor/IDE preferences
/// 5. Shell integration
/// 6. Quick tips and getting started
pub struct OnboardingDirective;

/// Onboarding steps for progress tracking.
const ONBOARDING_STEPS: &[&str] = &["welcome", "auth", "model", "editor", "shell", "tips"];

#[async_trait]
impl Directive for OnboardingDirective {
    fn name(&self) -> &str {
        "onboarding"
    }

    fn description(&self) -> &str {
        "Run the first-time setup wizard"
    }

    fn directive_type(&self) -> DirectiveType {
        DirectiveType::LocalWidget
    }

    fn is_immediate(&self) -> bool {
        true
    }

    fn is_hidden(&self) -> bool {
        true
    }

    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<CommandResult> {
        // Allow skipping to a specific step
        if let Some(step) = args.first() {
            if matches!(*step, "help" | "-h" | "--help") {
                let steps = ONBOARDING_STEPS.join(", ");
                return Ok(CommandResult::Text(format!(
                    "Usage: /onboarding [step]\n\n                     Run the setup wizard from the beginning or a specific step.\n\n                     Steps: {}",
                    steps
                )));
            }
            let step_lower = step.to_lowercase();
            if !ONBOARDING_STEPS.contains(&step_lower.as_str()) {
                return Ok(CommandResult::Error(format!(
                    "Unknown step: \"{}\". Valid steps: {}",
                    step_lower,
                    ONBOARDING_STEPS.join(", ")
                )));
            }
        }

        if ctx.is_non_interactive {
            return Ok(CommandResult::Text(format!(
                "Welcome to {}!\n\n                 To complete setup, run this command in interactive mode.\n                 For quick start: /login, then /model to select your model.",
                ctx.product_name
            )));
        }

        // Interactive onboarding wizard — walk through each step
        let step_start = if let Some(step) = args.first() {
            let step_lower = step.to_lowercase();
            ONBOARDING_STEPS
                .iter()
                .position(|s| *s == step_lower.as_str())
                .unwrap_or(0)
        } else {
            0
        };

        let mut lines: Vec<String> = Vec::new();

        for (i, step_name) in ONBOARDING_STEPS.iter().enumerate().skip(step_start) {
            match *step_name {
                "welcome" => {
                    lines.push(format!("=== Welcome to {} ===", ctx.product_name));
                    lines.push(String::new());
                    lines.push(format!(
                        "{} is an AI-powered coding assistant that lives in your terminal.",
                        ctx.product_name
                    ));
                    lines.push("Let's walk through setup so you can get started.".to_string());
                }
                "auth" => {
                    lines.push(format!("Step {}: Authentication", i + 1));
                    lines.push(format!(
                        "  Run /{} login to authenticate with your account.",
                        ctx.cli_name
                    ));
                }
                "model" => {
                    lines.push(format!("Step {}: Model Selection", i + 1));
                    lines.push("  Run /model to choose your preferred AI model.".to_string());
                }
                "editor" => {
                    lines.push(format!("Step {}: Editor/IDE Preferences", i + 1));
                    lines.push("  Run /ide to detect and connect your editor.".to_string());
                }
                "shell" => {
                    lines.push(format!("Step {}: Shell Integration", i + 1));
                    lines.push(
                        "  Run /terminal-setup to install keybindings for your terminal."
                            .to_string(),
                    );
                }
                "tips" => {
                    lines.push(format!("Step {}: Quick Tips", i + 1));
                    lines.push("  - Type a question or task in natural language".to_string());
                    lines.push("  - Use /help to see all available commands".to_string());
                    lines.push("  - Use /compact to summarize long conversations".to_string());
                    lines.push("  - Use /model to switch between AI models".to_string());
                    lines.push(String::new());
                    lines.push("Setup complete! You're ready to start.".to_string());
                }
                _ => {}
            }
            lines.push(String::new());
        }

        Ok(CommandResult::Text(lines.join("\n")))
    }
}
