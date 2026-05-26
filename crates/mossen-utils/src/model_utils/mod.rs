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
    Balanced,
    Max,
    Fast,
    Best,
    Balanced1M,
    Max1M,
    MaxPlan,
}

pub const MODEL_ALIASES: &[&str] = &[
    "balanced",
    "max",
    "fast",
    "best",
    "balanced[1m]",
    "max[1m]",
    "maxplan",
];

pub const MODEL_FAMILY_ALIASES: &[&str] = &["balanced", "max", "fast"];

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

fn external_vendor_id() -> &'static str {
    "mossen"
}

fn external_model_stem(model: &str) -> String {
    format!("mossen-{}", model)
}

pub fn external_bedrock_model_id(
    model: &str,
    region: Option<&str>,
    date: Option<&str>,
    variant: Option<&str>,
) -> String {
    let mut id = format!("{}.{}", external_vendor_id(), external_model_stem(model));
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
    format!("{}/{}", external_vendor_id(), external_model_stem(model))
}

pub static ALL_MODEL_CONFIGS: Lazy<HashMap<&'static str, ModelConfig>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "fast35",
        ModelConfig {
            first_party: "mossen-3-5-fast-20241022".to_string(),
            bedrock: external_bedrock_model_id(
                "3-5-fast",
                Some("us"),
                Some("20241022"),
                Some("v1:0"),
            ),
            vertex: "mossen-3-5-fast-20241022".to_string(),
            foundry: external_foundry_model_id("3-5-fast"),
        },
    );
    m.insert(
        "fast45",
        ModelConfig {
            first_party: "mossen-fast-4-5-20251001".to_string(),
            bedrock: external_bedrock_model_id(
                "fast-4-5",
                Some("us"),
                Some("20251001"),
                Some("v1:0"),
            ),
            vertex: "mossen-fast-4-5-20251001".to_string(),
            foundry: external_foundry_model_id("fast-4-5"),
        },
    );
    m.insert(
        "balanced35",
        ModelConfig {
            first_party: "mossen-3-5-balanced-20241022".to_string(),
            bedrock: external_bedrock_model_id(
                "3-5-balanced",
                None,
                Some("20241022"),
                Some("v2:0"),
            ),
            vertex: "mossen-3-5-balanced-20241022-v2".to_string(),
            foundry: external_foundry_model_id("3-5-balanced"),
        },
    );
    m.insert(
        "balanced37",
        ModelConfig {
            first_party: "mossen-3-7-balanced-20250219".to_string(),
            bedrock: external_bedrock_model_id(
                "3-7-balanced",
                Some("us"),
                Some("20250219"),
                Some("v1:0"),
            ),
            vertex: "mossen-3-7-balanced-20250219".to_string(),
            foundry: external_foundry_model_id("3-7-balanced"),
        },
    );
    m.insert(
        "balanced40",
        ModelConfig {
            first_party: "mossen-balanced-4-20250514".to_string(),
            bedrock: external_bedrock_model_id(
                "balanced-4",
                Some("us"),
                Some("20250514"),
                Some("v1:0"),
            ),
            vertex: "mossen-balanced-4-20250514".to_string(),
            foundry: external_foundry_model_id("balanced-4"),
        },
    );
    m.insert(
        "balanced45",
        ModelConfig {
            first_party: "mossen-balanced-4-5-20250929".to_string(),
            bedrock: external_bedrock_model_id(
                "balanced-4-5",
                Some("us"),
                Some("20250929"),
                Some("v1:0"),
            ),
            vertex: "mossen-balanced-4-5-20250929".to_string(),
            foundry: external_foundry_model_id("balanced-4-5"),
        },
    );
    m.insert(
        "balanced46",
        ModelConfig {
            first_party: "mossen-balanced-4-6".to_string(),
            bedrock: external_bedrock_model_id("balanced-4-6", Some("us"), None, None),
            vertex: "mossen-balanced-4-6".to_string(),
            foundry: external_foundry_model_id("balanced-4-6"),
        },
    );
    m.insert(
        "max40",
        ModelConfig {
            first_party: "mossen-max-4-20250514".to_string(),
            bedrock: external_bedrock_model_id("max-4", Some("us"), Some("20250514"), Some("v1:0")),
            vertex: "mossen-max-4-20250514".to_string(),
            foundry: external_foundry_model_id("max-4"),
        },
    );
    m.insert(
        "max41",
        ModelConfig {
            first_party: "mossen-max-4-1-20250805".to_string(),
            bedrock: external_bedrock_model_id(
                "max-4-1",
                Some("us"),
                Some("20250805"),
                Some("v1:0"),
            ),
            vertex: "mossen-max-4-1-20250805".to_string(),
            foundry: external_foundry_model_id("max-4-1"),
        },
    );
    m.insert(
        "max45",
        ModelConfig {
            first_party: "mossen-max-4-5-20251101".to_string(),
            bedrock: external_bedrock_model_id(
                "max-4-5",
                Some("us"),
                Some("20251101"),
                Some("v1:0"),
            ),
            vertex: "mossen-max-4-5-20251101".to_string(),
            foundry: external_foundry_model_id("max-4-5"),
        },
    );
    m.insert(
        "max46",
        ModelConfig {
            first_party: "mossen-max-4-6".to_string(),
            bedrock: external_bedrock_model_id("max-4-6-v1", Some("us"), None, None),
            vertex: "mossen-max-4-6".to_string(),
            foundry: external_foundry_model_id("max-4-6"),
        },
    );
    m
});

pub fn get_canonical_model_ids() -> Vec<&'static str> {
    vec![
        "mossen-3-5-fast-20241022",
        "mossen-fast-4-5-20251001",
        "mossen-3-5-balanced-20241022",
        "mossen-3-7-balanced-20250219",
        "mossen-balanced-4-20250514",
        "mossen-balanced-4-5-20250929",
        "mossen-balanced-4-6",
        "mossen-max-4-20250514",
        "mossen-max-4-1-20250805",
        "mossen-max-4-5-20251101",
        "mossen-max-4-6",
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
        if std::env::var("USER_TYPE").ok().as_deref() == Some("internal") {
            allowed.push("api-staging.mossen.invalid");
        }
        return allowed.contains(&host);
    }
    false
}

// ─── Model Selection ─────────────────────────────────────────────────────────

pub fn get_small_fast_model() -> String {
    std::env::var("MOSSEN_CODE_SMALL_FAST_MODEL").unwrap_or_else(|_| get_default_fast_model())
}

pub fn get_default_fast_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("fast45").unwrap();
    match provider {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

pub fn get_default_balanced_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("balanced46").unwrap();
    match provider {
        APIProvider::FirstParty => config.first_party.clone(),
        APIProvider::Bedrock => config.bedrock.clone(),
        APIProvider::Vertex => config.vertex.clone(),
        APIProvider::Foundry => config.foundry.clone(),
    }
}

pub fn get_default_max_model() -> String {
    let provider = get_api_provider();
    let config = ALL_MODEL_CONFIGS.get("max46").unwrap();
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
        "balanced" => get_default_balanced_model(),
        "max" => get_default_max_model(),
        "fast" => get_default_fast_model(),
        "best" => get_default_max_model(),
        _ => model.to_string(),
    }
}

pub fn get_marketing_name_for_model(model: &str) -> String {
    if model.contains("max-4-6") {
        "Mossen Max 4.6".to_string()
    } else if model.contains("max-4-5") {
        "Mossen Max 4.5".to_string()
    } else if model.contains("max-4-1") {
        "Mossen Max 4.1".to_string()
    } else if model.contains("max-4") {
        "Mossen Max 4".to_string()
    } else if model.contains("balanced-4-6") {
        "Mossen Balanced 4.6".to_string()
    } else if model.contains("balanced-4-5") {
        "Mossen Balanced 4.5".to_string()
    } else if model.contains("balanced-4") {
        "Mossen Balanced 4".to_string()
    } else if model.contains("3-7-balanced") {
        "Mossen 3.7 Balanced".to_string()
    } else if model.contains("3-5-balanced") {
        "Mossen 3.5 Balanced".to_string()
    } else if model.contains("fast-4-5") {
        "Mossen Fast 4.5".to_string()
    } else if model.contains("3-5-fast") {
        "Mossen 3.5 Fast".to_string()
    } else {
        model.to_string()
    }
}

pub fn is_non_custom_max_model(model: &str) -> bool {
    model.contains("max-4")
        || model.contains("max-4-1")
        || model.contains("max-4-5")
        || model.contains("max-4-6")
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
    if model.contains("max") || model.contains("balanced-4") {
        200_000
    } else if model.contains("3-7-balanced") {
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
        "mossen-3-max-20240229",
        "mossen-3-5-balanced-20240620",
        "mossen-3-fast-20240307",
    ];
    deprecated_models.contains(&model)
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
        value: Some("balanced".to_string()),
        label: "Balanced".to_string(),
        description: "Fast and capable - good for most tasks".to_string(),
        description_for_model: Some("Mossen Balanced - fast, capable".to_string()),
    });

    options.push(ModelOption {
        value: Some("max".to_string()),
        label: "Max".to_string(),
        description: "Most capable - best for complex tasks".to_string(),
        description_for_model: Some("Mossen Max - most capable".to_string()),
    });

    options.push(ModelOption {
        value: Some("fast".to_string()),
        label: "Fast".to_string(),
        description: "Fastest - good for simple tasks".to_string(),
        description_for_model: Some("Mossen Fast - fastest".to_string()),
    });

    options
}

// ─── Agent Model ─────────────────────────────────────────────────────────────

pub fn get_agent_model() -> String {
    std::env::var("MOSSEN_CODE_AGENT_MODEL").unwrap_or_else(|_| get_default_balanced_model())
}

pub fn get_plan_model() -> String {
    std::env::var("MOSSEN_CODE_PLAN_MODEL").unwrap_or_else(|_| get_default_max_model())
}
