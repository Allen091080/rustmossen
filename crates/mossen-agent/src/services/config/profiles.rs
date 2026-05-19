//! Multi-profile schema — read / validate / desensitize.

use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::facade::{
    clear_mossen_config_overrides, resolve_mossen_config, set_mossen_config_override,
};
use super::types::ConfigOverrideScope;

/// Profile provider type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileProvider {
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
}

pub const PROFILE_PROVIDER_VALUES: &[&str] = &["openai-compatible"];

/// Profile schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSchema {
    pub provider: ProfileProvider,
    #[serde(rename = "baseURL")]
    pub base_url: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Profiles map type.
pub type ProfilesMap = HashMap<String, ProfileSchema>;

/// Desensitized profile (apiKey masked).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesensitizedProfile {
    pub provider: ProfileProvider,
    #[serde(rename = "baseURL")]
    pub base_url: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Profile source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileSource {
    #[serde(rename = "settings")]
    Settings,
    #[serde(rename = "fallback-env")]
    FallbackEnv,
}

/// Listed profile entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedProfile {
    pub name: String,
    pub profile: ProfileSchema,
    pub source: ProfileSource,
}

const PROFILES_KEY: &str = "mossen.profiles";
const ACTIVE_PROFILE_KEY: &str = "mossen.activeProfile";

/// Mask api key: first 6 + ... + last 4. Short keys fully masked.
pub fn mask_api_key(api_key: Option<&str>) -> String {
    let key = match api_key {
        Some(k) if !k.is_empty() => k.trim(),
        _ => return String::new(),
    };
    if key.is_empty() {
        return String::new();
    }
    if key.len() <= 12 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..6], &key[key.len() - 4..])
}

/// Desensitize a profile (mask apiKey).
pub fn desensitize_profile(profile: &ProfileSchema) -> DesensitizedProfile {
    DesensitizedProfile {
        provider: profile.provider.clone(),
        base_url: profile.base_url.clone(),
        model: profile.model.clone(),
        api_key: mask_api_key(Some(&profile.api_key)),
        name: profile.name.clone(),
    }
}

/// Desensitize all profiles in a map.
pub fn desensitize_profiles(profiles: &ProfilesMap) -> HashMap<String, DesensitizedProfile> {
    profiles
        .iter()
        .map(|(name, p)| (name.clone(), desensitize_profile(p)))
        .collect()
}

/// Validate a profile schema from a JSON value.
pub fn validate_profile(value: &Value) -> Result<ProfileSchema, String> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Err("profile must be an object".to_string()),
    };

    let provider = obj
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !PROFILE_PROVIDER_VALUES.contains(&provider) {
        return Err(format!(
            "provider must be one of {}, got \"{}\"",
            PROFILE_PROVIDER_VALUES.join("|"),
            provider
        ));
    }

    let base_url = obj
        .get("baseURL")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if base_url.is_empty() {
        return Err("baseURL required (non-empty string)".to_string());
    }

    let model = obj
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if model.is_empty() {
        return Err("model required (non-empty string)".to_string());
    }

    let api_key = obj
        .get("apiKey")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("apiKey required (non-empty string)".to_string());
    }

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(ProfileSchema {
        provider: ProfileProvider::OpenAiCompatible,
        base_url,
        model,
        api_key,
        name,
    })
}

/// Get all valid profiles from config.
pub fn get_profiles() -> ProfilesMap {
    let result = resolve_mossen_config(PROFILES_KEY, Value::Null);
    let raw = result.value;
    let obj = match raw.as_object() {
        Some(o) => o,
        None => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for (name, entry) in obj {
        if let Ok(profile) = validate_profile(entry) {
            out.insert(name.clone(), profile);
        }
    }
    out
}

/// Get active profile name. Returns None if not set or points to non-existent profile.
pub fn get_active_profile_name() -> Option<String> {
    let result = resolve_mossen_config(ACTIVE_PROFILE_KEY, Value::Null);
    let name = result.value.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let profiles = get_profiles();
    if profiles.contains_key(&name) {
        Some(name)
    } else {
        None
    }
}

/// Get active profile schema.
pub fn get_active_profile() -> Option<ProfileSchema> {
    let name = get_active_profile_name()?;
    let profiles = get_profiles();
    profiles.get(&name).cloned()
}

/// Get a profile by name.
pub fn get_profile_by_name(name: &str) -> Option<ProfileSchema> {
    get_profiles().get(name).cloned()
}

const FALLBACK_PROFILE_DEFAULT_NAME: &str = "qwen";

/// Get fallback profile from env vars.
pub fn get_fallback_profile() -> Option<ListedProfile> {
    let base_url = env::var("MOSSEN_CODE_CUSTOM_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    let api_key = env::var("MOSSEN_CODE_CUSTOM_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    let model = env::var("MOSSEN_CODE_CUSTOM_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let raw_name = env::var("MOSSEN_CODE_CUSTOM_NAME")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let name = if !raw_name.is_empty() {
        match validate_profile_name(&raw_name) {
            Ok(n) => n,
            Err(_) => FALLBACK_PROFILE_DEFAULT_NAME.to_string(),
        }
    } else {
        FALLBACK_PROFILE_DEFAULT_NAME.to_string()
    };
    let display_name = if !raw_name.is_empty() && validate_profile_name(&raw_name).is_ok() {
        Some(raw_name)
    } else {
        None
    };
    let profile = ProfileSchema {
        provider: ProfileProvider::OpenAiCompatible,
        base_url: base_url.trim_end_matches('/').to_string(),
        model,
        api_key,
        name: display_name,
    };
    Some(ListedProfile {
        name,
        profile,
        source: ProfileSource::FallbackEnv,
    })
}

/// Profile name pattern.
fn profile_name_pattern() -> Regex {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_-]{0,31}$").unwrap()
}

/// Validate a profile name.
pub fn validate_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name must be non-empty".to_string());
    }
    let pattern = profile_name_pattern();
    if !pattern.is_match(trimmed) {
        return Err(format!(
            "profile name \"{}\" must match ^[a-zA-Z][a-zA-Z0-9_-]{{0,31}}$ (start with letter, only letters/digits/_/-, 1-32 chars)",
            trimmed
        ));
    }
    Ok(trimmed.to_string())
}

/// List all visible profiles.
pub fn list_all_profiles() -> Vec<ListedProfile> {
    let settings = get_profiles();
    let mut settings_list: Vec<ListedProfile> = settings
        .into_iter()
        .map(|(name, profile)| ListedProfile {
            name,
            profile,
            source: ProfileSource::Settings,
        })
        .collect();
    settings_list.sort_by(|a, b| a.name.cmp(&b.name));
    if !settings_list.is_empty() {
        return settings_list;
    }
    match get_fallback_profile() {
        Some(fb) => vec![fb],
        None => Vec::new(),
    }
}

/// Get current session profile.
pub fn get_current_profile() -> Option<ListedProfile> {
    if let Some(name) = get_active_profile_name() {
        if let Some(p) = get_profiles().remove(&name) {
            return Some(ListedProfile {
                name,
                profile: p,
                source: ProfileSource::Settings,
            });
        }
    }
    get_fallback_profile()
}

/// Get default profile (from user scope settings).
pub fn get_default_profile() -> Option<ListedProfile> {
    if let Some(name) = get_default_active_profile_name() {
        if let Some(p) = get_profiles().remove(&name) {
            return Some(ListedProfile {
                name,
                profile: p,
                source: ProfileSource::Settings,
            });
        }
    }
    get_fallback_profile()
}

/// Read global default activeProfile directly from user settings file.
pub fn get_default_active_profile_name() -> Option<String> {
    let config_dir = env::var("MOSSEN_CONFIG_DIR").unwrap_or_else(|_| {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".mossen").to_string_lossy().to_string()
    });
    let settings_path = Path::new(&config_dir).join("settings.json");
    if !settings_path.exists() {
        return None;
    }
    let raw = fs::read_to_string(&settings_path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let obj = parsed.as_object()?;
    let v = obj.get(ACTIVE_PROFILE_KEY)?;
    let s = v.as_str()?.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Set/overwrite a profile.
pub fn set_profile(
    name: &str,
    schema: &Value,
    scope: ConfigOverrideScope,
) -> Result<ProfilesMap, String> {
    let validated_name = validate_profile_name(name)?;
    let profile = validate_profile(schema)?;
    let mut current = get_profiles();
    current.insert(validated_name, profile);
    let map_value = serde_json::to_value(&current).unwrap_or(Value::Object(Default::default()));
    set_mossen_config_override(PROFILES_KEY, map_value, scope);
    Ok(current)
}

/// Delete a profile.
pub fn delete_profile(
    name: &str,
    scope: ConfigOverrideScope,
) -> (bool, bool, ProfilesMap) {
    let mut current = get_profiles();
    if !current.contains_key(name) {
        return (false, false, current);
    }
    current.remove(name);
    let map_value = serde_json::to_value(&current).unwrap_or(Value::Object(Default::default()));
    set_mossen_config_override(PROFILES_KEY, map_value, scope);

    let mut active_cleared = false;
    let raw_active = resolve_mossen_config(ACTIVE_PROFILE_KEY, Value::Null).value;
    if let Some(active_name) = raw_active.as_str() {
        if active_name == name {
            set_mossen_config_override(ACTIVE_PROFILE_KEY, Value::Null, scope);
            active_cleared = true;
        }
    }
    (true, active_cleared, current)
}

/// Set active profile.
pub fn set_active_profile(
    name: &str,
    scope: ConfigOverrideScope,
) -> Result<(String, ProfileSchema, ProfileSource), String> {
    let validated_name = validate_profile_name(name)?;
    if let Some(real) = get_profile_by_name(&validated_name) {
        set_mossen_config_override(ACTIVE_PROFILE_KEY, Value::String(validated_name.clone()), scope);
        return Ok((validated_name, real, ProfileSource::Settings));
    }
    if let Some(fallback) = get_fallback_profile() {
        if fallback.name == validated_name {
            set_mossen_config_override(ACTIVE_PROFILE_KEY, Value::Null, scope);
            return Ok((validated_name, fallback.profile, ProfileSource::FallbackEnv));
        }
    }
    let settings_names: Vec<String> = get_profiles().keys().cloned().collect();
    let mut existing = settings_names;
    if let Some(fb) = get_fallback_profile() {
        if !existing.contains(&fb.name) {
            existing.push(fb.name);
        }
    }
    Err(format!(
        "cannot activate profile \"{}\": not found in mossen.profiles (existing: {})",
        validated_name,
        if existing.is_empty() {
            "<none>".to_string()
        } else {
            existing.join(", ")
        }
    ))
}

/// Clear active profile.
pub fn clear_active_profile(scope: ConfigOverrideScope) {
    set_mossen_config_override(ACTIVE_PROFILE_KEY, Value::Null, scope);
}

/// Set session-only active profile (runtime override).
pub fn set_session_active_profile(
    name: &str,
) -> Result<(String, ProfileSchema, ProfileSource), String> {
    set_active_profile(name, ConfigOverrideScope::Override)
}

/// Clear session-only active profile override.
pub fn clear_session_active_profile() {
    clear_mossen_config_overrides(ConfigOverrideScope::Override, Some(ACTIVE_PROFILE_KEY));
}

/// Profile test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileTestResult {
    pub ok: bool,
    pub status: u16,
    pub url: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Test profile connectivity (GET baseURL/models).
pub async fn test_profile(
    profile: &ProfileSchema,
    timeout_ms: Option<u64>,
) -> ProfileTestResult {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(5000));
    let base_trimmed = profile.base_url.trim_end_matches('/');
    let url = format!("{}/models", base_trimmed);
    let start = Instant::now();

    let client = Client::builder().timeout(timeout).build();
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return ProfileTestResult {
                ok: false,
                status: 0,
                url,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            };
        }
    };

    match client
        .get(&url)
        .header("Authorization", format!("Bearer {}", profile.api_key))
        .header("User-Agent", "mossen-profile-test/1.0")
        .send()
        .await
    {
        Ok(res) => ProfileTestResult {
            ok: true,
            status: res.status().as_u16(),
            url,
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
        Err(e) => ProfileTestResult {
            ok: false,
            status: 0,
            url,
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

/// Migrate fallback result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum MigrateFallbackResult {
    Migrated {
        profile_name: String,
        active_profile_set: bool,
        scope: String,
    },
    NotMigrated {
        reason: String,
        profile_name: Option<String>,
        scope: String,
    },
    Error {
        reason: String,
        scope: String,
    },
}

/// Migrate env fallback to a proper settings profile.
pub fn migrate_fallback_profile(
    scope: ConfigOverrideScope,
    target_name: Option<&str>,
    force: bool,
    activate: &str, // "auto" | "always" | "never"
) -> MigrateFallbackResult {
    let scope_str = match scope {
        ConfigOverrideScope::User => "user",
        ConfigOverrideScope::Project => "project",
        ConfigOverrideScope::Override => "override",
    };

    let fallback = match get_fallback_profile() {
        Some(f) => f,
        None => {
            return MigrateFallbackResult::NotMigrated {
                reason: "no-fallback".to_string(),
                profile_name: None,
                scope: scope_str.to_string(),
            };
        }
    };

    let target_name_raw = target_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.name.clone());

    let validated_name = match validate_profile_name(&target_name_raw) {
        Ok(n) => n,
        Err(reason) => {
            return MigrateFallbackResult::Error {
                reason,
                scope: scope_str.to_string(),
            };
        }
    };

    let existing = get_profile_by_name(&validated_name);
    if existing.is_some() && !force {
        return MigrateFallbackResult::NotMigrated {
            reason: "already-exists".to_string(),
            profile_name: Some(validated_name),
            scope: scope_str.to_string(),
        };
    }

    let profile_value = serde_json::to_value(&fallback.profile).unwrap_or(Value::Null);
    if let Err(reason) = set_profile(&validated_name, &profile_value, scope) {
        return MigrateFallbackResult::Error {
            reason,
            scope: scope_str.to_string(),
        };
    }

    let active_set = match activate {
        "always" => {
            set_mossen_config_override(
                ACTIVE_PROFILE_KEY,
                Value::String(validated_name.clone()),
                scope,
            );
            true
        }
        "auto" => {
            let current_active = get_active_profile_name();
            if current_active.is_none() || current_active.as_deref() == Some(&validated_name) {
                set_mossen_config_override(
                    ACTIVE_PROFILE_KEY,
                    Value::String(validated_name.clone()),
                    scope,
                );
                true
            } else {
                false
            }
        }
        _ => false,
    };

    MigrateFallbackResult::Migrated {
        profile_name: validated_name,
        active_profile_set: active_set,
        scope: scope_str.to_string(),
    }
}
