//! Internal model overrides for Mossen internal users.
//!
//! Reads the `mossen_internal_model_override` GrowthBook feature flag payload
//! from `MOSSEN_CODE_INTERNAL_MODEL_OVERRIDE`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::effort::EffortLevel;

fn serialize_effort_level<S>(value: &Option<EffortLevel>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(level) => serializer.serialize_str(level.as_str()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_effort_level<'de, D>(deserializer: D) -> Result<Option<EffortLevel>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    Ok(opt.and_then(|s| EffortLevel::from_str(&s)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalModel {
    pub alias: String,
    pub model: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        rename = "defaultEffortValue",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_effort_value: Option<f64>,
    #[serde(
        rename = "defaultEffortLevel",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_effort_level",
        deserialize_with = "deserialize_effort_level"
    )]
    pub default_effort_level: Option<EffortLevel>,
    #[serde(
        rename = "contextWindow",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub context_window: Option<u64>,
    #[serde(
        rename = "defaultMaxTokens",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_max_tokens: Option<u64>,
    #[serde(
        rename = "upperMaxTokensLimit",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub upper_max_tokens_limit: Option<u64>,
    /// Model defaults to adaptive thinking and rejects
    /// `thinking: { type: 'disabled' }`.
    #[serde(
        rename = "alwaysOnThinking",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub always_on_thinking: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalModelSwitchCalloutConfig {
    #[serde(
        rename = "modelAlias",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub model_alias: Option<String>,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InternalModelOverrideConfig {
    #[serde(
        rename = "defaultModel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_model: Option<String>,
    #[serde(
        rename = "defaultModelEffortLevel",
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_effort_level",
        deserialize_with = "deserialize_effort_level"
    )]
    pub default_model_effort_level: Option<EffortLevel>,
    #[serde(
        rename = "defaultSystemPromptSuffix",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_system_prompt_suffix: Option<String>,
    #[serde(rename = "internalModels", default, skip_serializing_if = "Option::is_none")]
    pub internal_models: Option<Vec<InternalModel>>,
    #[serde(
        rename = "switchCallout",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub switch_callout: Option<InternalModelSwitchCalloutConfig>,
}

fn read_internal_override_payload() -> Option<InternalModelOverrideConfig> {
    let raw = std::env::var("MOSSEN_CODE_INTERNAL_MODEL_OVERRIDE").ok()?;
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}

/// `getInternalModelOverrideConfig` — returns the cached internal model
/// override config, or `None` if the user isn't `internal` or no override is set.
pub fn get_internal_model_override_config() -> Option<InternalModelOverrideConfig> {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return None;
    }
    read_internal_override_payload()
}

pub fn get_internal_models() -> Vec<InternalModel> {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return Vec::new();
    }
    get_internal_model_override_config()
        .and_then(|c| c.internal_models)
        .unwrap_or_default()
}

pub fn resolve_internal_model(model: Option<&str>) -> Option<InternalModel> {
    if std::env::var("USER_TYPE").ok().as_deref() != Some("internal") {
        return None;
    }
    let model = model?;
    let lower = model.to_lowercase();
    get_internal_models()
        .into_iter()
        .find(|m| m.alias == model || lower.contains(&m.model.to_lowercase()))
}
