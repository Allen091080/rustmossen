//! `/model` — Multi-profile model list and session-level switching.
//!
//! Translates `commands/model/model.tsx` (175 lines).
//! Lists all configured model profiles, shows current/default,
//! and switches the active session profile by name.

use anyhow::Result;
use async_trait::async_trait;
use mossen_agent::services::config::profiles as config_profiles;

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
            "No model profiles configured.\n\n\
             Add one with:\n\
             \x20 {} --add-model-profile <name> --base-url <url> --model <model> --api-key <key>\n\n\
             Or start {} with --model <model-id> for a literal session model.",
            cli_name,
            cli_name
        );
    }

    let mut lines = Vec::new();
    lines.push(format!("Model profiles ({}):", models.len()));
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

    match &current_name {
        Some(name) => lines.push(format!("Current session profile: {}", name)),
        None => lines.push("Current session profile: <none>".to_string()),
    }
    match &default_name {
        Some(name) => lines.push(format!("Global default profile:  {}", name)),
        None => lines.push("Global default profile:  <none>".to_string()),
    }

    lines.push(String::new());
    lines.push("Usage:".to_string());
    lines.push("  /model <profile-name>          Switch this session profile".to_string());
    lines.push(format!(
        "  {} --set-model-profile <name>  Set the global default profile",
        cli_name
    ));

    lines.join("\n")
}

fn model_entries(ctx: &CommandContext) -> Vec<ModelEntry> {
    let current_profile = config_profiles::get_current_profile().map(|profile| profile.name);
    let default_profile = config_profiles::get_default_profile().map(|profile| profile.name);
    let mut entries = config_profiles::list_all_profiles()
        .into_iter()
        .map(|profile| {
            let is_current = current_profile.as_deref() == Some(profile.name.as_str());
            let is_default = default_profile.as_deref() == Some(profile.name.as_str());
            ModelEntry {
                name: profile.name.clone(),
                value: profile.name,
                model: profile.profile.model,
                source: profile_source_label(&profile.source).to_string(),
                is_current,
                is_default,
            }
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        let current_model = ctx
            .env_vars
            .get("MOSSEN_CODE_MODEL")
            .or_else(|| ctx.env_vars.get("MOSSEN_MODEL"))
            .filter(|value| !value.trim().is_empty())
            .cloned();
        if let Some(model) = current_model {
            entries.push(ModelEntry {
                name: model.clone(),
                value: model.clone(),
                model,
                source: "env".to_string(),
                is_current: true,
                is_default: false,
            });
        }
    }

    entries
}

fn profile_source_label(source: &config_profiles::ProfileSource) -> &'static str {
    match source {
        config_profiles::ProfileSource::Settings => "settings",
        config_profiles::ProfileSource::FallbackEnv => "fallback-env",
    }
}

/// Format the result of a model switch request.
fn format_switch_result(profile: &config_profiles::ListedProfile, cli_name: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Switched session profile to \"{}\".", profile.name));
    lines.push(format!("  model: {}", profile.profile.model));
    lines.push(format!(
        "  source: {}",
        profile_source_label(&profile.source)
    ));
    lines.push(String::new());
    lines.push(format!(
        "Use `{} --set-model-profile {}` to make this the global default.",
        cli_name, profile.name
    ));
    lines.join("\n")
}

fn format_unknown_profile(name: &str, models: &[ModelEntry]) -> String {
    let available = if models.is_empty() {
        "<none>".to_string()
    } else {
        models
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("Unknown model profile: {name}\nAvailable profiles: {available}")
}

/// `/model` command.
pub struct SwitchModelDirective;

#[async_trait]
impl Directive for SwitchModelDirective {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "List configured model profiles or switch session profile"
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

        let selected = config_profiles::list_all_profiles()
            .into_iter()
            .find(|profile| profile.name == name);

        match selected {
            Some(profile) => {
                config_profiles::set_session_active_profile(&profile.name)
                    .map_err(anyhow::Error::msg)?;
                config_profiles::apply_profile_to_custom_backend_env(&profile);
                output.push_str(&format_switch_result(&profile, cli_name));
                Ok(CommandResult::Text(output))
            }
            None => {
                let models = model_entries(ctx);
                output.push_str(&format_unknown_profile(name, &models));
                Ok(CommandResult::Error(output))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CommandContext;
    use mossen_agent::services::config::{facade, types::ConfigOverrideScope};
    use std::collections::HashMap;
    use std::env;
    use std::sync::{Mutex, OnceLock};

    fn config_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    const PROFILE_ENV_KEYS: &[&str] = &[
        "MOSSEN_CODE_USE_CUSTOM_BACKEND",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
        "MOSSEN_CODE_CUSTOM_NAME",
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_API_BASE_URL",
        "MOSSEN_API_KEY",
    ];

    struct EnvRestore(Vec<(&'static str, Option<String>)>);

    impl EnvRestore {
        fn capture(keys: &'static [&'static str]) -> Self {
            Self(
                keys.iter()
                    .map(|key| (*key, env::var(key).ok()))
                    .collect::<Vec<_>>(),
            )
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    fn test_context() -> CommandContext {
        CommandContext {
            cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            is_non_interactive: false,
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

    fn seed_profiles() {
        facade::reset_facade_for_testing();
        facade::set_mossen_config_override(
            "mossen.profiles",
            serde_json::json!({
                "fast": {
                    "provider": "openai-compatible",
                    "baseURL": "https://fast.example.com/v1",
                    "model": "example-fast",
                    "apiKey": "sk-test-fast-secret"
                },
                "large": {
                    "provider": "openai-compatible",
                    "baseURL": "https://large.example.com/v1",
                    "model": "example-large",
                    "apiKey": "sk-test-large-secret"
                }
            }),
            ConfigOverrideScope::Override,
        );
        facade::set_mossen_config_override(
            "mossen.activeProfile",
            serde_json::Value::String("large".to_string()),
            ConfigOverrideScope::Override,
        );
    }

    #[tokio::test]
    async fn model_directive_lists_configured_profiles_without_secrets() {
        let _guard = config_lock();
        seed_profiles();
        let result = SwitchModelDirective
            .execute(&[], &test_context())
            .await
            .expect("model list");
        let CommandResult::Text(text) = result else {
            panic!("expected text");
        };

        assert!(text.contains("Model profiles (2)"));
        assert!(text.contains("large [session]"));
        assert!(text.contains("fast"));
        assert!(text.contains("example-large"));
        assert!(!text.contains("sk-test-large-secret"));
        assert!(!text.contains("https://large.example.com/v1"));
        facade::reset_facade_for_testing();
    }

    #[tokio::test]
    async fn model_directive_switches_session_profile() {
        let _guard = config_lock();
        let _env_guard = crate::test_support::env_lock();
        let _env_restore = EnvRestore::capture(PROFILE_ENV_KEYS);
        for key in PROFILE_ENV_KEYS {
            env::remove_var(key);
        }
        seed_profiles();
        let result = SwitchModelDirective
            .execute(&["fast"], &test_context())
            .await
            .expect("model switch");
        let CommandResult::Text(text) = result else {
            panic!("expected text");
        };

        assert!(text.contains("Switched session profile to \"fast\""));
        assert!(text.contains("model: example-fast"));
        assert_eq!(
            config_profiles::get_active_profile_name().as_deref(),
            Some("fast")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL").as_deref(),
            Ok("openai-compatible")
        );
        assert_eq!(env::var("MOSSEN_CODE_CUSTOM_NAME").as_deref(), Ok("fast"));
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_BASE_URL").as_deref(),
            Ok("https://fast.example.com/v1")
        );
        assert_eq!(
            env::var("MOSSEN_CODE_CUSTOM_MODEL").as_deref(),
            Ok("example-fast")
        );
        assert_eq!(
            env::var("MOSSEN_API_BASE_URL").as_deref(),
            Ok("https://fast.example.com/v1")
        );
        facade::reset_facade_for_testing();
    }
}
