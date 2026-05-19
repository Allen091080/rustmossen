use std::collections::HashMap;
use std::path::PathBuf;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────────────

pub type ModelName = String;
pub type ModelSetting = Option<String>; // None = default

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum APIProvider {
    FirstParty,
    Bedrock,
    Vertex,
    Foundry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelAlias {
    Sonnet,
    Opus,
    Haiku,
    Best,
    Sonnet1M,
    Opus1M,
    OpusPlan,
}

pub const MODEL_ALIASES: &[&str] = &[
    "sonnet", "opus", "haiku", "best", "sonnet[1m]", "opus[1m]", "opusplan",
];

pub const MODEL_FAMILY_ALIASES: &[&str] = &["sonnet", "opus", "haiku"];

pub fn is_model_alias(model_input: &str) -> bool {
    MODEL_ALIASES.contains(&model_input)
}

pub fn is_model_family_alias(model: &str) -> bool {
    MODEL_FAMILY_ALIASES.contains(&model)
}

// ─── Model Configs ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub first_party: String,
    pub bedrock: String,
    pub vertex: String,
    pub foundry: String,
}

pub fn external_bedrock_model_id(model: &str, region: Option<&str>, date: Option<&str>, variant: Option<&str>) -> String {
    let mut id = format!("anthropic.mossen-{}", model);
    if let Some(d) = date {
        id.push_str(&format!("-{}", d));
    }
    if let Some(r) = region {
        id = format!("{}.{}", r, id);
    }
    if let Some(v) = variant {
        id.push_str(&format!(":{}", v));
    }
    id
}

pub fn external_vertex_model_id(model: &str, date: Option<&str>, variant: Option<&str>) -> String {
    let mut id = format!("mossen-{}", model);
    if let Some(d) = date {
        id.push_str(&format!("-{}", d));
    }
    if let Some(v) = variant {
        id.push_str(&format!("-{}", v));
    }
    id
}

pub fn external_foundry_model_id(model: &str) -> String {
    format!("anthropic/mossen-{}", model)
}

pub static ALL_MODEL_CONFIGS: Lazy<HashMap<&'static str, ModelConfig>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("haiku35", ModelConfig {
        first_party: "mossen-3-5-haiku-20241022".to_string(),
        bedrock: "us.anthropic.mossen-3-5-haiku-20241022-v1:0".to_string(),
        vertex: "mossen-3-5-haiku-20241022".to_string(),
        foundry: "anthropic/mossen-3-5-haiku".to_string(),
    });
    m.insert("haiku45", ModelConfig {
        first_party: "mossen-haiku-4-5-20251001".to_string(),
        bedrock: "us.anthropic.mossen-haiku-4-5-20251001-v1:0".to_string(),
        vertex: "mossen-haiku-4-5-20251001".to_string(),
        foundry: "anthropic/mossen-haiku-4-5".to_string(),
    });
    m.insert("sonnet35", ModelConfig {
        first_party: "mossen-3-5-sonnet-20241022".to_string(),
        bedrock: "anthropic.mossen-3-5-sonnet-20241022-v2:0".to_string(),
        vertex: "mossen-3-5-sonnet-20241022-v2".to_string(),
        foundry: "anthropic/mossen-3-5-sonnet".to_string(),
    });
    m.insert("sonnet37", ModelConfig {
        first_party: "mossen-3-7-sonnet-20250219".to_string(),
        bedrock: "us.anthropic.mossen-3-7-sonnet-20250219-v1:0".to_string(),
        vertex: "mossen-3-7-sonnet-20250219".to_string(),
        foundry: "anthropic/mossen-3-7-sonnet".to_string(),
    });
    m.insert("sonnet40", ModelConfig {
        first_party: "mossen-sonnet-4-20250514".to_string(),
        bedrock: "us.anthropic.mossen-sonnet-4-20250514-v1:0".to_string(),
        vertex: "mossen-sonnet-4-20250514".to_string(),
        foundry: "anthropic/mossen-sonnet-4".to_string(),
    });
    m.insert("sonnet45", ModelConfig {
        first_party: "mossen-sonnet-4-5-20250929".to_string(),
        bedrock: "us.anthropic.mossen-sonnet-4-5-20250929-v1:0".to_string(),
        vertex: "mossen-sonnet-4-5-20250929".to_string(),
        foundry: "anthropic/mossen-sonnet-4-5".to_string(),
    });
    m.insert("sonnet46", ModelConfig {
        first_party: "mossen-sonnet-4-6".to_string(),
        bedrock: "us.anthropic.mossen-sonnet-4-6".to_string(),
        vertex: "mossen-sonnet-4-6".to_string(),
        foundry: "anthropic/mossen-sonnet-4-6".to_string(),
    });
    m.insert("opus40", ModelConfig {
        first_party: "mossen-opus-4-20250514".to_string(),
        bedrock: "us.anthropic.mossen-opus-4-20250514-v1:0".to_string(),
        vertex: "mossen-opus-4-20250514".to_string(),
        foundry: "anthropic/mossen-opus-4".to_string(),
    });
    m.insert("opus41", ModelConfig {
        first_party: "mossen-opus-4-1-20250805".to_string(),
        bedrock: "us.anthropic.mossen-opus-4-1-20250805-v1:0".to_string(),
        vertex: "mossen-opus-4-1-20250805".to_string(),
        foundry: "anthropic/mossen-opus-4-1".to_string(),
    });
    m.insert("opus45", ModelConfig {
        first_party: "mossen-opus-4-5-20251101".to_string(),
        bedrock: "us.anthropic.mossen-opus-4-5-20251101-v1:0".to_string(),
        vertex: "mossen-opus-4-5-20251101".to_string(),
        foundry: "anthropic/mossen-opus-4-5".to_string(),
    });
    m.insert("opus46", ModelConfig {
        first_party: "mossen-opus-4-6".to_string(),
        bedrock: "us.anthropic.mossen-opus-4-6-v1".to_string(),
        vertex: "mossen-opus-4-6".to_string(),
        foundry: "anthropic/mossen-opus-4-6".to_string(),
    });
    m
});

pub fn get_canonical_model_ids() -> Vec<&'static str> {
    vec![
        "mossen-3-5-haiku-20241022",
        "mossen-haiku-4-5-20251001",
        "mossen-3-5-sonnet-20241022",
        "mossen-3-7-sonnet-20250219",
        "mossen-sonnet-4-20250514",
        "mossen-sonnet-4-5-20250929",
        "mossen-sonnet-4-6",
        "mossen-opus-4-20250514",
        "mossen-opus-4-1-20250805",
        "mossen-opus-4-5-20251101",
        "mossen-opus-4-6",
    ]
}

// ─── Provider Detection ──────────────────────────────────────────────────────

pub fn get_api_provider() -> APIProvider {
    let use_bedrock = std::env::var("MOSSEN_CODE_USE_BEDROCK").unwrap_or_default();
    let use_vertex = std::env::var("MOSSEN_CODE_USE_VERTEX").unwrap_or_default();
    let use_foundry = std::env::var("MOSSEN_CODE_USE_FOUNDRY").unwrap_or_default();

    if is_env_truthy(&use_bedrock) {
        APIProvider::Bedrock
    } else if is_env_truthy(&use_vertex) {
        APIProvider::Vertex
    } else if is_env_truthy(&use_foundry) {
        APIProvider::Foundry
    } else {
        APIProvider::FirstParty
    }
}

fn is_env_truthy(val: &str) -> bool {
    val == "1" || val.to_lowercase() == "true"
}

pub fn is_first_party_mossen_base_url() -> bool {
    let base_url = match std::env::var("MOSSEN_CODE_API_BASE_URL") {
        Ok(url) => url,
        Err(_) => return false,
    };
    if let Ok(url) = url::Url::parse(&base_url) {
        let host = url.host_str().unwrap_or("");
        let mut allowed = vec!["api.mossen.invalid"];
        if std::env::var("USER_TYPE").ok().as_deref() == Some("ant") {
            allowed.push("api-staging.mossen.invalid");
        }
        return allowed.contains(&host);
    }
    false
}

// ─── Model Selection ─────────────────────────────────────────────────────────

pub fn get_small_fast_model() -> String {
    std::env::var("MOSSEN_CODE_SMALL_FAST_MODEL")
        .unwrap_or_else(|_| get_default_haiku_model())
}

pub fn get_default_haiku_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("haiku45").unwrap();
    match provider {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

pub fn get_default_sonnet_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("sonnet46").unwrap();
    match provider {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

pub fn get_default_opus_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("opus46").unwrap();
    match provider {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

pub fn get_user_specified_model_setting() -> Option<String> {
    // Priority: env var > settings
    if let Ok(model) = std::env::var("MOSSEN_CODE_MODEL") {
        if !model.is_empty() {
            return Some(model);
        }
    }
    None
}

pub fn get_canonical_name(model: &str) -> String {
    // Check if it's a known alias
    match model {
        "sonnet" => get_default_sonnet_model(),
        "opus" => get_default_opus_model(),
        "haiku" => get_default_haiku_model(),
        "best" => get_default_opus_model(),
        _ => model.to_string(),
    }
}

pub fn get_marketing_name_for_model(model: &str) -> String {
    if model.contains("opus-4-6") {
        "Mossen Opus 4.6".to_string()
    } else if model.contains("opus-4-5") {
        "Mossen Opus 4.5".to_string()
    } else if model.contains("opus-4-1") {
        "Mossen Opus 4.1".to_string()
    } else if model.contains("opus-4") {
        "Mossen Opus 4".to_string()
    } else if model.contains("sonnet-4-6") {
        "Mossen Sonnet 4.6".to_string()
    } else if model.contains("sonnet-4-5") {
        "Mossen Sonnet 4.5".to_string()
    } else if model.contains("sonnet-4") {
        "Mossen Sonnet 4".to_string()
    } else if model.contains("3-7-sonnet") {
        "Mossen 3.7 Sonnet".to_string()
    } else if model.contains("3-5-sonnet") {
        "Mossen 3.5 Sonnet".to_string()
    } else if model.contains("haiku-4-5") {
        "Mossen Haiku 4.5".to_string()
    } else if model.contains("3-5-haiku") {
        "Mossen 3.5 Haiku".to_string()
    } else {
        model.to_string()
    }
}

pub fn is_non_custom_opus_model(model: &str) -> bool {
    model.contains("opus-4") || model.contains("opus-4-1") || model.contains("opus-4-5") || model.contains("opus-4-6")
}

// ─── Model Capabilities ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapability {
    pub id: String,
    pub max_input_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
}

fn get_cache_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("mossen")
        .join("cache");
    config_dir.join("model-capabilities.json")
}

pub fn get_model_capability(model: &str) -> Option<ModelCapability> {
    if get_api_provider() != APIProvider::FirstParty {
        return None;
    }
    let cache_path = get_cache_path();
    let raw = std::fs::read_to_string(&cache_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let models = parsed.get("models")?.as_array()?;

    let m_lower = model.to_lowercase();

    // Exact match first
    for entry in models {
        let id = entry.get("id")?.as_str()?;
        if id.to_lowercase() == m_lower {
            return serde_json::from_value(entry.clone()).ok();
        }
    }
    // Substring match (longest ID first - models is pre-sorted)
    for entry in models {
        let id = entry.get("id")?.as_str()?;
        if m_lower.contains(&id.to_lowercase()) {
            return serde_json::from_value(entry.clone()).ok();
        }
    }
    None
}

pub fn get_context_window_for_model(model: &str) -> u64 {
    if let Some(cap) = get_model_capability(model) {
        return cap.max_input_tokens.unwrap_or(200_000);
    }
    // Default context windows based on model
    if model.contains("opus") || model.contains("sonnet-4") {
        200_000
    } else if model.contains("3-7-sonnet") {
        200_000
    } else {
        200_000
    }
}

pub async fn refresh_model_capabilities() {
    // In production: fetch from API, write to cache
    // This is called at startup to refresh the model capabilities cache
}

// ─── Model Validation ────────────────────────────────────────────────────────

pub fn validate_model(model: &str) -> Result<String, String> {
    if model.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }

    // Check if it's an alias
    if is_model_alias(model) {
        return Ok(get_canonical_name(model));
    }

    // Check if it's a known canonical ID
    let canonical_ids = get_canonical_model_ids();
    if canonical_ids.contains(&model) {
        return Ok(model.to_string());
    }

    // Allow arbitrary model IDs for custom backends
    Ok(model.to_string())
}

// ─── Model Allowlist ─────────────────────────────────────────────────────────

pub fn is_model_allowed(model: &str) -> bool {
    // If no allowlist configured, all models are allowed
    let allowlist = std::env::var("MOSSEN_CODE_AVAILABLE_MODELS").unwrap_or_default();
    if allowlist.is_empty() {
        return true;
    }

    let allowed: Vec<&str> = allowlist.split(',').map(|s| s.trim()).collect();

    // Check direct match
    if allowed.contains(&model) {
        return true;
    }

    // Check family alias match
    for alias in &allowed {
        if is_model_family_alias(alias) {
            let family = *alias;
            if model.contains(family) {
                return true;
            }
        }
    }

    false
}

// ─── Deprecation ─────────────────────────────────────────────────────────────

pub fn is_model_deprecated(model: &str) -> bool {
    let deprecated_models = [
        "mossen-3-opus-20240229",
        "mossen-3-5-sonnet-20240620",
        "mossen-3-haiku-20240307",
    ];
    deprecated_models.iter().any(|&d| model == d)
}

pub fn get_deprecation_message(model: &str) -> Option<String> {
    if is_model_deprecated(model) {
        Some(format!(
            "Model '{}' is deprecated. Please switch to a newer model.",
            model
        ))
    } else {
        None
    }
}

// ─── Bedrock ─────────────────────────────────────────────────────────────────

pub fn get_bedrock_model_id(model: &str) -> Option<String> {
    for config in ALL_MODEL_CONFIGS.values() {
        if config.first_party == model {
            return Some(config.bedrock.clone());
        }
    }
    None
}

pub fn get_vertex_model_id(model: &str) -> Option<String> {
    for config in ALL_MODEL_CONFIGS.values() {
        if config.first_party == model {
            return Some(config.vertex.clone());
        }
    }
    None
}

pub fn resolve_model_for_provider(model: &str, provider: APIProvider) -> String {
    for config in ALL_MODEL_CONFIGS.values() {
        if config.first_party == model {
            return match provider {
                APIProvider::FirstParty => config.first_party.clone(),
                APIProvider::Bedrock => config.bedrock.clone(),
                APIProvider::Vertex => config.vertex.clone(),
                APIProvider::Foundry => config.foundry.clone(),
            };
        }
    }
    model.to_string()
}

// ─── Model Options (for /model command) ──────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelOption {
    pub value: ModelSetting,
    pub label: String,
    pub description: String,
    pub description_for_model: Option<String>,
}

pub fn get_model_options() -> Vec<ModelOption> {
    let mut options = Vec::new();

    options.push(ModelOption {
        value: None,
        label: "Default (recommended)".to_string(),
        description: "Use the default model".to_string(),
        description_for_model: None,
    });

    options.push(ModelOption {
        value: Some("sonnet".to_string()),
        label: "Sonnet".to_string(),
        description: "Fast and capable - good for most tasks".to_string(),
        description_for_model: Some("Mossen Sonnet - fast, capable".to_string()),
    });

    options.push(ModelOption {
        value: Some("opus".to_string()),
        label: "Opus".to_string(),
        description: "Most capable - best for complex tasks".to_string(),
        description_for_model: Some("Mossen Opus - most capable".to_string()),
    });

    options.push(ModelOption {
        value: Some("haiku".to_string()),
        label: "Haiku".to_string(),
        description: "Fastest - good for simple tasks".to_string(),
        description_for_model: Some("Mossen Haiku - fastest".to_string()),
    });

    options
}

// ─── Agent Model ─────────────────────────────────────────────────────────────

pub fn get_agent_model() -> String {
    std::env::var("MOSSEN_CODE_AGENT_MODEL")
        .unwrap_or_else(|_| get_default_sonnet_model())
}

pub fn get_plan_model() -> String {
    std::env::var("MOSSEN_CODE_PLAN_MODEL")
        .unwrap_or_else(|_| get_default_opus_model())
}
