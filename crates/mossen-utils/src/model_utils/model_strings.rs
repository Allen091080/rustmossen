//! Provider-resolved model ID strings.
//!
//! Direct translation of `utils/model/modelStrings.ts`. The TS code keeps a
//! mutable global ("modelStringsState") that gets populated lazily — once with
//! a sync snapshot for non-Bedrock providers, asynchronously for Bedrock. We
//! mirror that with a `Mutex<Option<ModelStrings>>` and an async refresh that
//! is serialized through a `tokio::sync::Mutex`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::log::log_error_str;
use crate::settings::get_session_settings_cache;

use super::bedrock::{find_first_match, get_bedrock_inference_profiles};
use super::configs::{ModelKey, ALL_MODEL_CONFIGS, CANONICAL_ID_TO_KEY};
use super::external_provider_ids::extract_external_provider_model_stem;
use super::providers::{get_api_provider, APIProvider};

/// Provider-specific model string map. Keys are the short keys from
/// [`ModelKey`]; we expose canonical fields for ergonomic access (TS uses a
/// `Record<ModelKey, string>`).
#[derive(Debug, Clone, Default)]
pub struct ModelStrings {
    pub haiku35: String,
    pub haiku45: String,
    pub sonnet35: String,
    pub sonnet37: String,
    pub sonnet40: String,
    pub sonnet45: String,
    pub sonnet46: String,
    pub opus40: String,
    pub opus41: String,
    pub opus45: String,
    pub opus46: String,
}

impl ModelStrings {
    pub fn get(&self, key: ModelKey) -> &str {
        match key {
            ModelKey::Haiku35 => &self.haiku35,
            ModelKey::Haiku45 => &self.haiku45,
            ModelKey::Sonnet35 => &self.sonnet35,
            ModelKey::Sonnet37 => &self.sonnet37,
            ModelKey::Sonnet40 => &self.sonnet40,
            ModelKey::Sonnet45 => &self.sonnet45,
            ModelKey::Sonnet46 => &self.sonnet46,
            ModelKey::Opus40 => &self.opus40,
            ModelKey::Opus41 => &self.opus41,
            ModelKey::Opus45 => &self.opus45,
            ModelKey::Opus46 => &self.opus46,
        }
    }

    pub fn set(&mut self, key: ModelKey, value: String) {
        match key {
            ModelKey::Haiku35 => self.haiku35 = value,
            ModelKey::Haiku45 => self.haiku45 = value,
            ModelKey::Sonnet35 => self.sonnet35 = value,
            ModelKey::Sonnet37 => self.sonnet37 = value,
            ModelKey::Sonnet40 => self.sonnet40 = value,
            ModelKey::Sonnet45 => self.sonnet45 = value,
            ModelKey::Sonnet46 => self.sonnet46 = value,
            ModelKey::Opus40 => self.opus40 = value,
            ModelKey::Opus41 => self.opus41 = value,
            ModelKey::Opus45 => self.opus45 = value,
            ModelKey::Opus46 => self.opus46 = value,
        }
    }
}

fn model_strings_state() -> &'static Mutex<Option<ModelStrings>> {
    static STATE: OnceLock<Mutex<Option<ModelStrings>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn get_model_strings_state() -> Option<ModelStrings> {
    model_strings_state().lock().unwrap().clone()
}

fn set_model_strings_state(value: ModelStrings) {
    *model_strings_state().lock().unwrap() = Some(value);
}

/// Public test/reset hook — mirrors the bootstrap-state setter used by the TS
/// test suite to reinitialize the cache between tests.
pub fn reset_model_strings_state() {
    *model_strings_state().lock().unwrap() = None;
}

fn get_builtin_model_strings(provider: APIProvider) -> ModelStrings {
    let mut out = ModelStrings::default();
    for key in ModelKey::all() {
        let cfg = ALL_MODEL_CONFIGS[key];
        out.set(*key, cfg.for_provider(provider).to_string());
    }
    out
}

fn get_bedrock_profile_needle(key: ModelKey) -> String {
    let bedrock_id = &ALL_MODEL_CONFIGS[&key].bedrock;
    extract_external_provider_model_stem(bedrock_id).unwrap_or_else(|| bedrock_id.clone())
}

async fn get_bedrock_model_strings_internal() -> ModelStrings {
    let fallback = get_builtin_model_strings(APIProvider::Bedrock);
    let profiles = match get_bedrock_inference_profiles().await {
        Ok(p) => p,
        Err(e) => {
            log_error_str(&format!("{}", e));
            return fallback;
        }
    };
    if profiles.is_empty() {
        return fallback;
    }
    let mut out = ModelStrings::default();
    for key in ModelKey::all() {
        let needle = get_bedrock_profile_needle(*key);
        let resolved = find_first_match(&profiles, &needle)
            .unwrap_or_else(|| fallback.get(*key).to_string());
        out.set(*key, resolved);
    }
    out
}

fn current_model_overrides() -> Option<HashMap<String, String>> {
    get_session_settings_cache()
        .and_then(|s| s.settings.model_overrides.clone())
}

fn apply_model_overrides(mut ms: ModelStrings) -> ModelStrings {
    let overrides = match current_model_overrides() {
        Some(o) => o,
        None => return ms,
    };
    for (canonical_id, override_value) in overrides.iter() {
        if override_value.is_empty() {
            continue;
        }
        if let Some(key) = CANONICAL_ID_TO_KEY.get(canonical_id) {
            ms.set(*key, override_value.clone());
        }
    }
    ms
}

/// Resolve an overridden model ID (e.g. a Bedrock ARN) back to its canonical
/// Mossen first-party model ID. Safe to call before settings are loaded — in
/// that case it just returns the input unchanged.
pub fn resolve_overridden_model(model_id: &str) -> String {
    let overrides = match current_model_overrides() {
        Some(o) => o,
        None => return model_id.to_string(),
    };
    for (canonical_id, override_value) in overrides.iter() {
        if override_value == model_id {
            return canonical_id.clone();
        }
    }
    model_id.to_string()
}

fn bedrock_refresh_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// `updateBedrockModelStrings` — serialized async refresh of the Bedrock
/// model-string cache.
async fn update_bedrock_model_strings() {
    let _guard = bedrock_refresh_lock().lock().await;
    if get_model_strings_state().is_some() {
        return;
    }
    let ms = get_bedrock_model_strings_internal().await;
    set_model_strings_state(ms);
}

fn init_model_strings() {
    if get_model_strings_state().is_some() {
        return;
    }
    let provider = get_api_provider();
    if provider != APIProvider::Bedrock {
        set_model_strings_state(get_builtin_model_strings(provider));
        return;
    }
    // On Bedrock, kick off the refresh in the background if we have a runtime.
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async {
            update_bedrock_model_strings().await;
        });
    }
}

/// `getModelStrings()` — synchronous accessor matching the TS signature.
pub fn get_model_strings() -> ModelStrings {
    if let Some(ms) = get_model_strings_state() {
        return apply_model_overrides(ms);
    }
    init_model_strings();
    apply_model_overrides(get_builtin_model_strings(get_api_provider()))
}

/// Ensure model strings are fully initialized. For Bedrock users, this waits
/// for the inference-profile fetch to complete.
pub async fn ensure_model_strings_initialized() {
    if get_model_strings_state().is_some() {
        return;
    }
    let provider = get_api_provider();
    if provider != APIProvider::Bedrock {
        set_model_strings_state(get_builtin_model_strings(provider));
        return;
    }
    update_bedrock_model_strings().await;
}

/// Internal helper returning model strings as a HashMap keyed by the short
/// string identifier; used by the test suite and by `modelOptions` when it
/// looks up keys dynamically.
pub fn model_strings_as_map() -> HashMap<String, String> {
    let ms = get_model_strings();
    let mut out = HashMap::new();
    for key in ModelKey::all() {
        out.insert(key.as_str().to_string(), ms.get(*key).to_string());
    }
    out
}
