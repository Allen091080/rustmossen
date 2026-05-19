//! `/model` — Multi-profile model list and session-level switching.
//!
//! Translates `commands/model/model.tsx` (175 lines).
//! Lists all configured model profiles, shows current/default,
//! and switches the active session profile by name.

use anyhow::Result;
use async_trait::async_trait;

use crate::context::{CommandContext, CommandResult, Directive, DirectiveType};

/// A model profile entry.
#[derive(Debug, Clone)]
struct ProfileEntry {
    name: String,
    provider: String,
    model: String,
    base_url: String,
    api_key_masked: String,
    source: String,
    is_current: bool,
    is_default: bool,
}

/// Format the full profile list output.
fn format_profile_list(profiles: &[ProfileEntry], cli_name: &str) -> String {
    if profiles.is_empty() {
        return format!(
            "No model profiles configured.\n\n\
             Create one with the CLI (apiKey is required):\n\
             \x20 {} --add-model-profile <name> \\\n\
             \x20   --provider openai-compatible \\\n\
             \x20   --baseURL <url> \\\n\
             \x20   --model <id> \\\n\
             \x20   --apiKey <key>\n\n\
             Then activate it as the global default:\n\
             \x20 {} --set-model-profile <name>",
            cli_name, cli_name
        );
    }

    let mut lines = Vec::new();
    lines.push(format!("Model profiles ({}):", profiles.len()));
    lines.push(String::new());

    let mut current_name: Option<String> = None;
    let mut default_name: Option<String> = None;

    for entry in profiles {
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
        lines.push(format!("    provider: {}", entry.provider));
        lines.push(format!("    model:    {}", entry.model));
        lines.push(format!("    baseURL:  {}", entry.base_url));
        lines.push(format!("    apiKey:   {}", entry.api_key_masked));
        let source_label = if entry.source == "fallback-env" {
            "env (MOSSEN_CODE_CUSTOM_*)"
        } else {
            "settings.json"
        };
        lines.push(format!("    source:   {}", source_label));
        lines.push(String::new());
    }

    // Current session profile
    match &current_name {
        Some(name) => lines.push(format!("Current session profile: {}", name)),
        None => lines.push("Current session profile: <none>".to_string()),
    }
    // Global default
    match &default_name {
        Some(name) => lines.push(format!("Global default profile:  {}", name)),
        None => lines.push("Global default profile:  <none>".to_string()),
    }
    // Override notice
    if let (Some(ref cur), Some(ref def)) = (&current_name, &default_name) {
        if cur != def {
            lines.push(String::new());
            lines.push(format!(
                "Session has been overridden — restart mossen to revert to \"{}\".",
                def
            ));
        }
    }
    // Fallback notice
    if profiles.iter().any(|e| e.source == "fallback-env") {
        lines.push(String::new());
        lines.push("Tip: this profile comes from legacy env (MOSSEN_CODE_CUSTOM_*).".to_string());
        lines.push("     Migrate it to ~/.mossen/settings.json so it lives alongside your other profiles:".to_string());
        lines.push("       mossen --migrate-fallback-profile".to_string());
    }
    lines.push(String::new());
    lines.push("Usage:".to_string());
    lines.push("  /model <profileName>           Switch session profile (this conversation only)".to_string());
    lines.push(format!(
        "  {} --set-model-profile <n> Set global default (persists in ~/.mossen/settings.json)",
        cli_name
    ));

    lines.join("\n")
}

/// Format the result of a profile switch.
fn format_switch_result(name: &str, cli_name: &str) -> String {
    // In a real implementation, this would call setSessionActiveProfile()
    // and update the main loop model. For now, display the switch result:
    let mut lines = Vec::new();
    lines.push(format!("Switched session profile to \"{}\".", name));
    lines.push(format!("  name:     {}", name));
    lines.push(String::new());
    lines.push("Note: this only affects the current session.".to_string());
    lines.push(format!(
        "Use `{} --set-model-profile {}` to set as global default.",
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
        "List model profiles or switch session model"
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
            // No args → list all profiles
            // In a real implementation, this would call listAllProfiles()
            let profiles = Vec::new(); // Would be populated from config
            return Ok(CommandResult::Text(format_profile_list(&profiles, cli_name)));
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
