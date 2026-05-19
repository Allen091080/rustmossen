//! EnvOverrideProvider — env-based configuration override (priority 1).
//!
//! Reads MOSSEN_CONFIG_OVERRIDES (preferred) or MOSSEN_INTERNAL_FC_OVERRIDES (deprecated).
//! Content is a JSON object with mossen.* or tengu_* keys.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::env;

use crate::services::config::alias_map::resolve_aliased_key;
use crate::services::config::types::{ConfigValueSource, MossenConfigProvider, ProviderResult};

const NEW_ENV_NAME: &str = "MOSSEN_CONFIG_OVERRIDES";
const DEPRECATED_ENV_NAME: &str = "MOSSEN_INTERNAL_FC_OVERRIDES";

struct EnvOverrideCache {
    data: Option<HashMap<String, Value>>,
    parse_attempted: bool,
    deprecation_warned: bool,
}

static CACHE: Lazy<Mutex<EnvOverrideCache>> = Lazy::new(|| {
    Mutex::new(EnvOverrideCache {
        data: None,
        parse_attempted: false,
        deprecation_warned: false,
    })
});

fn parse_env_overrides() -> Option<HashMap<String, Value>> {
    let mut cache = CACHE.lock();
    if cache.parse_attempted {
        return cache.data.clone();
    }
    cache.parse_attempted = true;

    let new_raw = env::var(NEW_ENV_NAME).ok();
    let deprecated_raw = env::var(DEPRECATED_ENV_NAME).ok();

    if new_raw.is_some() && deprecated_raw.is_some() && !cache.deprecation_warned {
        eprintln!(
            "[mossen] Warning: both {} and {} set; using {} ({} is deprecated).",
            NEW_ENV_NAME, DEPRECATED_ENV_NAME, NEW_ENV_NAME, DEPRECATED_ENV_NAME
        );
        cache.deprecation_warned = true;
    } else if new_raw.is_none() && deprecated_raw.is_some() && !cache.deprecation_warned {
        eprintln!(
            "[mossen] Warning: {} is deprecated; rename to {}.",
            DEPRECATED_ENV_NAME, NEW_ENV_NAME
        );
        cache.deprecation_warned = true;
    }

    let raw = new_raw.or(deprecated_raw);
    let raw = match raw {
        Some(r) => r,
        None => return None,
    };

    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => {
            let mut data = HashMap::new();
            for (k, v) in map {
                let resolved = resolve_aliased_key(&k).to_string();
                data.insert(resolved, v);
            }
            cache.data = Some(data.clone());
            Some(data)
        }
        Ok(_) => {
            eprintln!(
                "[mossen] Warning: {}/{} must be a JSON object; ignoring.",
                NEW_ENV_NAME, DEPRECATED_ENV_NAME
            );
            None
        }
        Err(e) => {
            eprintln!(
                "[mossen] Warning: failed to parse {}/{}: {}; ignoring.",
                NEW_ENV_NAME, DEPRECATED_ENV_NAME, e
            );
            None
        }
    }
}

/// Reset internal cache (testing only).
pub fn reset_env_override_cache_for_testing() {
    let mut cache = CACHE.lock();
    cache.data = None;
    cache.parse_attempted = false;
    cache.deprecation_warned = false;
}

/// Environment variable override provider.
pub struct EnvOverrideProvider;

impl MossenConfigProvider for EnvOverrideProvider {
    fn name(&self) -> ConfigValueSource {
        ConfigValueSource::Env
    }

    fn priority(&self) -> u8 {
        ConfigValueSource::Env.priority()
    }

    fn enabled(&self) -> bool {
        true
    }

    fn get(&self, key: &str) -> Option<ProviderResult> {
        let data = parse_env_overrides()?;
        data.get(key).map(|value| ProviderResult {
            value: value.clone(),
            source: ConfigValueSource::Env,
            resolved_key: None,
        })
    }
}
