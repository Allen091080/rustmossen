//! Provider-reported model capabilities cache.
//!
//! Direct translation of `utils/model/modelCapabilities.ts`. The TS source
//! fetches `/v1/models` via the Anthropic SDK; here we keep the on-disk cache
//! reader and the lookup path that the rest of the codebase calls into. The
//! refresh helper is wired up so it can be invoked at startup: it reads from a
//! pre-staged JSON file (the request layer in `mossen-agent` writes there
//! when running against a live API).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::custom_backend::is_custom_backend_enabled;
use crate::env::get_mossen_config_home_dir;
use crate::json::safe_parse_json_value;
use crate::logging::log_for_debugging;
use crate::privacy_level::is_essential_traffic_only;
use crate::slow_operations::json_stringify;

use super::providers::{get_api_provider, is_first_party_mossen_base_url, APIProvider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCapability {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
}

fn get_cache_dir() -> PathBuf {
    get_mossen_config_home_dir().join("cache")
}

fn get_cache_path() -> PathBuf {
    get_cache_dir().join("model-capabilities.json")
}

fn can_read_model_capabilities_cache() -> bool {
    if get_api_provider() != APIProvider::FirstParty {
        return false;
    }
    if is_custom_backend_enabled() {
        return false;
    }
    true
}

fn can_refresh_model_capabilities() -> bool {
    if !can_read_model_capabilities_cache() {
        return false;
    }
    if !is_first_party_mossen_base_url() {
        return false;
    }
    true
}

fn sort_for_matching(mut models: Vec<ModelCapability>) -> Vec<ModelCapability> {
    models.sort_by(|a, b| {
        let len_cmp = b.id.len().cmp(&a.id.len());
        if len_cmp == std::cmp::Ordering::Equal {
            a.id.cmp(&b.id)
        } else {
            len_cmp
        }
    });
    models
}

fn load_cache_cache() -> &'static Mutex<HashMap<PathBuf, Option<Vec<ModelCapability>>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Vec<ModelCapability>>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_cache(path: &Path) -> Option<Vec<ModelCapability>> {
    if let Some(cached) = load_cache_cache().lock().unwrap().get(path).cloned() {
        return cached;
    }
    let result = (|| -> Option<Vec<ModelCapability>> {
        let raw = fs::read_to_string(path).ok()?;
        let parsed = safe_parse_json_value(&raw)?;
        let models = parsed.get("models")?.as_array()?;
        let mut out: Vec<ModelCapability> = Vec::with_capacity(models.len());
        for entry in models {
            if let Ok(cap) = serde_json::from_value::<ModelCapability>(entry.clone()) {
                out.push(cap);
            }
        }
        Some(out)
    })();
    load_cache_cache()
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), result.clone());
    result
}

fn invalidate_load_cache(path: &Path) {
    load_cache_cache().lock().unwrap().remove(path);
}

pub fn get_model_capability(model: &str) -> Option<ModelCapability> {
    if !can_read_model_capabilities_cache() {
        return None;
    }
    let path = get_cache_path();
    let cached = load_cache(&path)?;
    if cached.is_empty() {
        return None;
    }
    let m = model.to_lowercase();
    if let Some(exact) = cached.iter().find(|c| c.id.to_lowercase() == m) {
        return Some(exact.clone());
    }
    cached
        .iter()
        .find(|c| m.contains(&c.id.to_lowercase()))
        .cloned()
}

/// Write a new capability list into the on-disk cache. Sorts entries
/// longest-id-first to make substring matching deterministic.
pub fn write_model_capabilities(models: Vec<ModelCapability>) -> anyhow::Result<()> {
    if models.is_empty() {
        return Ok(());
    }
    let path = get_cache_path();
    let sorted = sort_for_matching(models);
    if load_cache(&path).as_deref() == Some(sorted.as_slice()) {
        log_for_debugging("[modelCapabilities] cache unchanged, skipping write");
        return Ok(());
    }
    fs::create_dir_all(get_cache_dir())?;
    let payload = serde_json::json!({
        "models": sorted,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });
    let serialized = json_stringify(&payload);
    fs::write(&path, serialized)?;
    invalidate_load_cache(&path);
    log_for_debugging(&format!(
        "[modelCapabilities] cached {} models",
        sorted.len()
    ));
    Ok(())
}

/// `refreshModelCapabilities` — best-effort refresh of the on-disk cache.
///
/// The TS source talks to the live Anthropic API; here we let the request
/// layer (mossen-agent) hand us the freshly-fetched list via the
/// `MOSSEN_CODE_MODEL_CAPABILITIES_FEED` env var (a JSON array). When
/// nothing is supplied we simply return without an error so this is safe to
/// call at startup.
pub async fn refresh_model_capabilities() {
    if !can_refresh_model_capabilities() {
        return;
    }
    if is_essential_traffic_only() {
        return;
    }
    let raw = match std::env::var("MOSSEN_CODE_MODEL_CAPABILITIES_FEED") {
        Ok(v) if !v.is_empty() => v,
        _ => return,
    };
    let parsed: Vec<ModelCapability> = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(error) => {
            log_for_debugging(&format!(
                "[modelCapabilities] fetch failed: {}",
                error
            ));
            return;
        }
    };
    if let Err(error) = write_model_capabilities(parsed) {
        log_for_debugging(&format!(
            "[modelCapabilities] fetch failed: {}",
            error
        ));
    }
}
