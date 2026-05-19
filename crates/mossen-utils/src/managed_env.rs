//! Managed environment variable application.
//!
//! Applies environment variables from settings to process.env equivalent,
//! handling SSH tunnel vars, host-managed provider vars, and CCD spawn env keys.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use once_cell::sync::Lazy;

/// Provider-managed env var names that should be stripped when host manages providers.
const PROVIDER_MANAGED_VARS: &[&str] = &[
    "MOSSEN_CODE_API_BASE_URL",
    "MOSSEN_CODE_USE_BEDROCK",
    "MOSSEN_CODE_USE_VERTEX",
    "MOSSEN_CODE_USE_GOOGLE",
    "MOSSEN_CODE_MODEL",
    "MOSSEN_CODE_SMALL_FAST_MODEL",
];

/// Safe env vars that can be applied from project-scoped settings before trust.
static SAFE_ENV_VARS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("MOSSEN_CODE_NO_FLICKER");
    set.insert("MOSSEN_CODE_DISABLE_MOUSE");
    set.insert("MOSSEN_CODE_DISABLE_MOUSE_CLICKS");
    set.insert("MOSSEN_CODE_OUTPUT_STYLE");
    set.insert("MOSSEN_CODE_MAX_CONTEXT_TOKENS");
    set.insert("MOSSEN_CODE_DISABLE_1M_CONTEXT");
    set.insert("MOSSEN_CODE_MAX_OUTPUT_TOKENS");
    set.insert("MOSSEN_CODE_THINKING_BUDGET");
    set.insert("MOSSEN_CODE_EFFORT");
    set.insert("MOSSEN_CODE_LANGUAGE");
    set
});

/// CCD spawn env keys — captured once on first apply.
static CCD_SPAWN_ENV_KEYS: Lazy<Mutex<Option<HashSet<String>>>> =
    Lazy::new(|| Mutex::new(None));

/// SSH tunnel env vars that should not be clobbered.
const SSH_TUNNEL_VARS: &[&str] = &[
    "MOSSEN_CODE_UNIX_SOCKET",
    "MOSSEN_CODE_API_BASE_URL",
    "MOSSEN_CODE_API_KEY",
    "MOSSEN_CODE_AUTH_TOKEN",
];

/// Check if a var name is a provider-managed env var.
pub fn is_provider_managed_env_var(key: &str) -> bool {
    PROVIDER_MANAGED_VARS.contains(&key)
}

/// Strip SSH tunnel vars from an env map when MOSSEN_CODE_UNIX_SOCKET is set.
fn without_ssh_tunnel_vars(env: &HashMap<String, String>) -> HashMap<String, String> {
    if std::env::var("MOSSEN_CODE_UNIX_SOCKET").is_err() {
        return env.clone();
    }
    env.iter()
        .filter(|(k, _)| !SSH_TUNNEL_VARS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Strip provider-managed vars when MOSSEN_CODE_PROVIDER_MANAGED_BY_HOST is set.
fn without_host_managed_provider_vars(
    env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let managed_by_host = std::env::var("MOSSEN_CODE_PROVIDER_MANAGED_BY_HOST")
        .map(|v| is_env_truthy(&v))
        .unwrap_or(false);

    if !managed_by_host {
        return env.clone();
    }

    env.iter()
        .filter(|(k, _)| !is_provider_managed_env_var(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Strip CCD spawn env keys.
fn without_ccd_spawn_env_keys(env: &HashMap<String, String>) -> HashMap<String, String> {
    let guard = CCD_SPAWN_ENV_KEYS.lock().unwrap();
    match &*guard {
        None => env.clone(),
        Some(keys) => env
            .iter()
            .filter(|(k, _)| !keys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

/// Compose the strip filters applied to every settings-sourced env object.
pub fn filter_settings_env(env: &HashMap<String, String>) -> HashMap<String, String> {
    without_ccd_spawn_env_keys(&without_host_managed_provider_vars(
        &without_ssh_tunnel_vars(env),
    ))
}

/// Trusted setting source names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedSettingSource {
    UserSettings,
    FlagSettings,
    PolicySettings,
}

/// Apply environment variables from trusted sources.
///
/// Called before the trust dialog so that user/enterprise env vars like
/// MOSSEN_CODE_API_BASE_URL take effect during first-run/onboarding.
///
/// Parameters:
/// - `global_config_env`: env vars from global config (~/.mossen.json)
/// - `get_settings_for_source`: function to retrieve env vars for a setting source
/// - `is_setting_source_enabled`: function to check if a setting source is enabled
/// - `get_merged_settings_env`: function to get fully-merged settings env
/// - `is_remote_managed_eligible_fn`: function to trigger remote managed settings eligibility check
pub fn apply_safe_config_environment_variables(
    global_config_env: &HashMap<String, String>,
    user_settings_env: Option<&HashMap<String, String>>,
    flag_settings_env: Option<&HashMap<String, String>>,
    policy_settings_env: Option<&HashMap<String, String>>,
    merged_settings_env: &HashMap<String, String>,
    is_user_settings_enabled: bool,
    is_ccd_mode: bool,
) -> HashMap<String, String> {
    // Capture CCD spawn-env keys before any settings.env is applied (once).
    {
        let mut guard = CCD_SPAWN_ENV_KEYS.lock().unwrap();
        if guard.is_none() {
            if is_ccd_mode {
                *guard = Some(std::env::vars().map(|(k, _)| k).collect());
            } else {
                // Mark as "not CCD" by setting to an empty sentinel value that won't match
                // Actually, we use `None` to indicate no CCD filtering needed, but we need
                // to distinguish "not yet captured" from "not CCD". Use Some(empty) for non-CCD.
                // Wait, the TS code uses `null` for "not CCD". Let's replicate:
                // ccdSpawnEnvKeys = null means not CCD → no filtering.
                // We'll keep it as None = not captured yet, and add a separate bool.
            }
        }
    }

    let mut result: HashMap<String, String> = HashMap::new();

    // Global config env
    let filtered_global = filter_settings_env(global_config_env);
    result.extend(filtered_global);

    // Apply ALL env vars from trusted setting sources, policy last.
    // Skip policySettings in first loop (applied last).
    if is_user_settings_enabled {
        if let Some(env) = user_settings_env {
            let filtered = filter_settings_env(env);
            result.extend(filtered);
        }
    }

    if let Some(env) = flag_settings_env {
        let filtered = filter_settings_env(env);
        result.extend(filtered);
    }

    // Policy settings (highest priority)
    if let Some(env) = policy_settings_env {
        let filtered = filter_settings_env(env);
        result.extend(filtered);
    }

    // Apply only safe env vars from the fully-merged settings
    let filtered_merged = filter_settings_env(merged_settings_env);
    for (key, value) in &filtered_merged {
        if SAFE_ENV_VARS.contains(key.to_uppercase().as_str()) {
            result.insert(key.clone(), value.clone());
        }
    }

    result
}

/// Apply ALL environment variables from settings to process.env.
/// Should only be called after trust is established.
pub fn apply_config_environment_variables(
    global_config_env: &HashMap<String, String>,
    merged_settings_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut result: HashMap<String, String> = HashMap::new();

    let filtered_global = filter_settings_env(global_config_env);
    result.extend(filtered_global);

    let filtered_merged = filter_settings_env(merged_settings_env);
    result.extend(filtered_merged);

    result
}

fn is_env_truthy(val: &str) -> bool {
    matches!(val, "1" | "true" | "yes" | "on")
}
