//! Model configuration diagnostics shared by CLI and slash-command surfaces.

use serde::Serialize;
use serde_json::Value;

use super::facade;
use super::profiles::{self, ProfileProvider, ProfileSource};
use super::types::ConfigValueSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentModelProfileDiagnostic {
    pub name: String,
    pub source: ProfileSource,
    pub provider: ProfileProvider,
    pub model: String,
    pub base_url_present: bool,
    pub base_url_redacted: bool,
    pub api_key_present: bool,
    pub api_key_redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigEnvDiagnostic {
    pub custom_backend_enabled: bool,
    pub protocol_present: bool,
    pub base_url_present: bool,
    pub api_key_present: bool,
    pub auth_token_present: bool,
    pub model_present: bool,
    pub values_redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigDoctorSnapshot {
    pub status: String,
    pub issues: Vec<String>,
    pub next_action: String,
    pub next_command: String,
    pub next_commands: Vec<String>,
    pub settings_profile_count: usize,
    pub visible_profile_count: usize,
    pub raw_profile_entry_count: usize,
    pub invalid_settings_profile_count: usize,
    pub invalid_profile_names_included: bool,
    pub active_profile_name: Option<String>,
    pub default_profile_name: Option<String>,
    pub raw_active_profile_present: bool,
    pub raw_active_profile_valid: bool,
    pub active_profile_source: &'static str,
    pub profiles_source: &'static str,
    pub fallback_env_available: bool,
    pub fallback_env_partial: bool,
    pub env: ModelConfigEnvDiagnostic,
    pub current_profile: Option<CurrentModelProfileDiagnostic>,
    pub raw_config_included: bool,
    pub base_urls_redacted: bool,
    pub api_keys_redacted: bool,
}

fn env_key_present(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn config_source_label(source: ConfigValueSource) -> &'static str {
    match source {
        ConfigValueSource::Env => "env",
        ConfigValueSource::Project => "project",
        ConfigValueSource::User => "user",
        ConfigValueSource::Default => "default",
        ConfigValueSource::Remote => "remote",
        ConfigValueSource::Override => "override",
    }
}

pub fn model_config_doctor_snapshot() -> ModelConfigDoctorSnapshot {
    let raw_profiles = facade::resolve_mossen_config("mossen.profiles", Value::Null);
    let raw_active = facade::resolve_mossen_config("mossen.activeProfile", Value::Null);
    let raw_active_name = raw_active
        .value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let raw_profile_entry_count = raw_profiles
        .value
        .as_object()
        .map_or(0, serde_json::Map::len);
    let invalid_settings_profile_count = raw_profiles
        .value
        .as_object()
        .map(|profile_map| {
            profile_map
                .values()
                .filter(|profile| profiles::validate_profile(profile).is_err())
                .count()
        })
        .unwrap_or(0);
    let settings_profiles = profiles::get_profiles();
    let settings_profile_count = settings_profiles.len();
    let fallback_profile_available = profiles::get_fallback_profile().is_some();
    let visible_profiles = profiles::list_all_profiles();
    let current_profile = profiles::get_current_profile();
    let active_profile_name = profiles::get_active_profile_name();
    let default_profile_name = profiles::get_default_active_profile_name();
    let active_profile_missing = raw_active_name.is_some() && active_profile_name.is_none();
    let fallback_env_any = [
        "MOSSEN_CODE_CUSTOM_BASE_URL",
        "MOSSEN_CODE_CUSTOM_API_KEY",
        "MOSSEN_CODE_CUSTOM_AUTH_TOKEN",
        "MOSSEN_CODE_CUSTOM_MODEL",
        "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL",
    ]
    .iter()
    .any(|key| env_key_present(key));
    let fallback_env_partial = fallback_env_any && !fallback_profile_available;

    let mut issues = Vec::new();
    if !raw_profiles.value.is_null() && !raw_profiles.value.is_object() {
        issues.push("profiles_not_object".to_string());
    }
    if raw_profile_entry_count > 0 && settings_profile_count == 0 {
        issues.push("no_valid_settings_profiles".to_string());
    } else if invalid_settings_profile_count > 0 {
        issues.push("some_settings_profiles_invalid".to_string());
    }
    if active_profile_missing {
        issues.push("active_profile_not_found".to_string());
    }
    if visible_profiles.is_empty() {
        issues.push("no_model_profile".to_string());
    }
    if fallback_env_partial {
        issues.push("custom_backend_env_incomplete".to_string());
    }

    let (status, next_action, next_commands) = if visible_profiles.is_empty() {
        (
            "missing",
            "Add and activate a model profile before starting an interactive coding session.",
            vec![
                "mossen --add-model-profile my-model --provider openai-compatible --baseURL https://api.example.com/v1 --model your-model-name --apiKey \"$YOUR_API_KEY\"",
                "mossen --set-model-profile my-model",
            ],
        )
    } else if active_profile_missing {
        (
            "warning",
            "Select an existing profile with --set-model-profile or /model.",
            vec![
                "mossen --list-model-profiles",
                "mossen --set-model-profile <profile-name>",
            ],
        )
    } else if fallback_env_partial {
        (
            "warning",
            "Complete MOSSEN_CODE_CUSTOM_* variables or migrate them into a model profile.",
            vec![
                "mossen --add-model-profile my-model --provider openai-compatible --baseURL https://api.example.com/v1 --model your-model-name --apiKey \"$YOUR_API_KEY\"",
            ],
        )
    } else if invalid_settings_profile_count > 0 {
        (
            "warning",
            "Fix invalid entries in mossen.profiles; /model only lists valid profiles.",
            vec!["mossen --list-model-profiles"],
        )
    } else {
        (
            "configured",
            "Model configuration is present; use --test-model-profile to verify provider reachability.",
            vec!["mossen --test-model-profile <profile-name> --timeout 30000"],
        )
    };
    let next_commands = next_commands
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let next_command = next_commands.first().cloned().unwrap_or_default();

    let current_profile = current_profile
        .as_ref()
        .map(|profile| CurrentModelProfileDiagnostic {
            name: profile.name.clone(),
            source: profile.source.clone(),
            provider: profile.profile.provider.clone(),
            model: profile.profile.model.clone(),
            base_url_present: !profile.profile.base_url.trim().is_empty(),
            base_url_redacted: true,
            api_key_present: !profile.profile.api_key.trim().is_empty(),
            api_key_redacted: true,
        });

    ModelConfigDoctorSnapshot {
        status: status.to_string(),
        issues,
        next_action: next_action.to_string(),
        next_command,
        next_commands,
        settings_profile_count,
        visible_profile_count: visible_profiles.len(),
        raw_profile_entry_count,
        invalid_settings_profile_count,
        invalid_profile_names_included: false,
        active_profile_name,
        default_profile_name,
        raw_active_profile_present: raw_active_name.is_some(),
        raw_active_profile_valid: raw_active_name.is_none() || !active_profile_missing,
        active_profile_source: config_source_label(raw_active.source),
        profiles_source: config_source_label(raw_profiles.source),
        fallback_env_available: fallback_profile_available,
        fallback_env_partial,
        env: ModelConfigEnvDiagnostic {
            custom_backend_enabled: env_key_present("MOSSEN_CODE_USE_CUSTOM_BACKEND"),
            protocol_present: env_key_present("MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL"),
            base_url_present: env_key_present("MOSSEN_CODE_CUSTOM_BASE_URL"),
            api_key_present: env_key_present("MOSSEN_CODE_CUSTOM_API_KEY"),
            auth_token_present: env_key_present("MOSSEN_CODE_CUSTOM_AUTH_TOKEN"),
            model_present: env_key_present("MOSSEN_CODE_CUSTOM_MODEL"),
            values_redacted: true,
        },
        current_profile,
        raw_config_included: false,
        base_urls_redacted: true,
        api_keys_redacted: true,
    }
}
