//! Bedrock helpers — inference profile listing, region prefix handling, and
//! ARN parsing.
//!
//! Direct translation of `utils/model/bedrock.ts`. The TypeScript source uses
//! `@aws-sdk/client-bedrock` to enumerate cross-region inference profiles; we
//! don't depend on the AWS SDK from `mossen-utils`, so the listing helpers
//! read pre-populated profile lists from environment variables (the same
//! mechanism the test suite uses) and the client constructors return Rust
//! handles holding the resolved region/proxy/auth state for the request layer
//! to consume.

use std::sync::{Mutex, OnceLock};

use crate::env::{get_aws_region, is_env_truthy};

use super::external_provider_ids::{
    external_provider_vendor_id, has_external_provider_vendor_id,
    is_external_bedrock_foundation_model,
};

/// Memoization cache for [`get_bedrock_inference_profiles`].
fn profiles_cache() -> &'static Mutex<Option<Vec<String>>> {
    static CACHE: OnceLock<Mutex<Option<Vec<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Reset hook for tests.
pub fn reset_bedrock_inference_profiles_cache() {
    *profiles_cache().lock().unwrap() = None;
}

/// `getBedrockInferenceProfiles` — memoized listing of cross-region inference
/// profile IDs that match the external provider vendor.
///
/// In the TS source this calls `bedrock.ListInferenceProfilesCommand`. Since
/// `mossen-utils` doesn't link the AWS SDK, we pull the inventory from
/// `MOSSEN_CODE_BEDROCK_INFERENCE_PROFILES` (comma-separated list) which is
/// also what the test fixtures populate. This returns `Ok(empty vec)` rather
/// than an error when nothing is configured so callers don't surface scary
/// log lines on default setups.
pub async fn get_bedrock_inference_profiles() -> anyhow::Result<Vec<String>> {
    if let Some(cached) = profiles_cache().lock().unwrap().clone() {
        return Ok(cached);
    }

    let mut profiles: Vec<String> = std::env::var("MOSSEN_CODE_BEDROCK_INFERENCE_PROFILES")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Filter for external provider models (SYSTEM_DEFINED filtering is the
    // upstream responsibility; here we only keep entries whose ID embeds the
    // external vendor identifier).
    profiles.retain(|p| has_external_provider_vendor_id(p));

    *profiles_cache().lock().unwrap() = Some(profiles.clone());
    Ok(profiles)
}

pub fn find_first_match(profiles: &[String], substring: &str) -> Option<String> {
    profiles.iter().find(|p| p.contains(substring)).cloned()
}

/// Pre-resolved Bedrock client configuration. The actual HTTP client lives in
/// the request layer; this struct carries everything required to construct it.
#[derive(Debug, Clone)]
pub struct BedrockClientConfig {
    pub region: String,
    pub endpoint: Option<String>,
    pub skip_auth: bool,
    pub bearer_token: Option<String>,
}

impl BedrockClientConfig {
    fn resolve() -> Self {
        Self {
            region: get_aws_region(),
            endpoint: std::env::var("MOSSEN_CODE_BEDROCK_BASE_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            skip_auth: is_env_truthy(
                std::env::var("MOSSEN_CODE_SKIP_BEDROCK_AUTH")
                    .ok()
                    .as_deref(),
            ),
            bearer_token: std::env::var("AWS_BEARER_TOKEN_BEDROCK")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }
}

/// Lazily-constructed handle used by the inference-profile listing helpers.
#[derive(Debug, Clone)]
pub struct BedrockClient {
    pub config: BedrockClientConfig,
}

async fn create_bedrock_client() -> anyhow::Result<BedrockClient> {
    Ok(BedrockClient {
        config: BedrockClientConfig::resolve(),
    })
}

/// Lazy handle for the Bedrock-Runtime API. The TS code constructs a real
/// AWS SDK client here; we expose the resolved config so the request layer can
/// build whatever HTTP transport it uses.
#[derive(Debug, Clone)]
pub struct BedrockRuntimeClient {
    pub config: BedrockClientConfig,
}

pub async fn create_bedrock_runtime_client() -> anyhow::Result<BedrockRuntimeClient> {
    Ok(BedrockRuntimeClient {
        config: BedrockClientConfig::resolve(),
    })
}

fn backing_model_cache() -> &'static Mutex<std::collections::HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, Option<String>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// `getInferenceProfileBackingModel` — return the foundation model ID backing
/// a given inference profile. Uses an in-memory cache.
///
/// Without the AWS SDK we cannot reach Bedrock at runtime; we instead resolve
/// the backing model from the profile ID by stripping the cross-region prefix
/// (e.g. `us.anthropic.claude-...` → `anthropic.claude-...`). This matches the
/// shape the AWS API would return in practice for system-defined profiles.
pub async fn get_inference_profile_backing_model(profile_id: &str) -> Option<String> {
    if let Some(cached) = backing_model_cache().lock().unwrap().get(profile_id).cloned() {
        return cached;
    }

    let resolved = compute_backing_model(profile_id);
    backing_model_cache()
        .lock()
        .unwrap()
        .insert(profile_id.to_string(), resolved.clone());
    let _ = create_bedrock_client();
    resolved
}

fn compute_backing_model(profile_id: &str) -> Option<String> {
    let effective = extract_model_id_from_arn(profile_id);
    let vendor = external_provider_vendor_id();
    for prefix in BEDROCK_REGION_PREFIXES {
        let needle = format!("{}.{}.", prefix, vendor);
        if effective.starts_with(&needle) {
            return Some(effective[prefix.len() + 1..].to_string());
        }
    }
    if is_external_bedrock_foundation_model(&effective) {
        return Some(effective);
    }
    None
}

/// Check if a model ID is an external Bedrock foundation model.
pub fn is_foundation_model(model_id: &str) -> bool {
    is_external_bedrock_foundation_model(model_id)
}

/// Cross-region inference profile prefixes for Bedrock.
pub const BEDROCK_REGION_PREFIXES: &[&str] = &["us", "eu", "apac", "global"];

/// Extract the model/inference profile ID from a Bedrock ARN. If the input is
/// not an ARN, returns it unchanged.
pub fn extract_model_id_from_arn(model_id: &str) -> String {
    if !model_id.starts_with("arn:") {
        return model_id.to_string();
    }
    match model_id.rfind('/') {
        Some(idx) => model_id[idx + 1..].to_string(),
        None => model_id.to_string(),
    }
}

/// Strongly-typed cross-region inference profile prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedrockRegionPrefix {
    Us,
    Eu,
    Apac,
    Global,
}

impl BedrockRegionPrefix {
    pub fn as_str(self) -> &'static str {
        match self {
            BedrockRegionPrefix::Us => "us",
            BedrockRegionPrefix::Eu => "eu",
            BedrockRegionPrefix::Apac => "apac",
            BedrockRegionPrefix::Global => "global",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "us" => Some(BedrockRegionPrefix::Us),
            "eu" => Some(BedrockRegionPrefix::Eu),
            "apac" => Some(BedrockRegionPrefix::Apac),
            "global" => Some(BedrockRegionPrefix::Global),
            _ => None,
        }
    }
}

/// Extract the region prefix from a Bedrock cross-region inference model ID.
/// Handles both plain model IDs and full ARN format.
pub fn get_bedrock_region_prefix(model_id: &str) -> Option<BedrockRegionPrefix> {
    let effective = extract_model_id_from_arn(model_id);
    let vendor = external_provider_vendor_id();
    for prefix in BEDROCK_REGION_PREFIXES {
        let needle = format!("{}.{}.", prefix, vendor);
        if effective.starts_with(&needle) {
            return BedrockRegionPrefix::from_str(prefix);
        }
    }
    None
}

/// Apply a region prefix to a Bedrock model ID.
pub fn apply_bedrock_region_prefix(model_id: &str, prefix: BedrockRegionPrefix) -> String {
    if let Some(existing) = get_bedrock_region_prefix(model_id) {
        let from = format!("{}.", existing.as_str());
        let to = format!("{}.", prefix.as_str());
        return model_id.replacen(&from, &to, 1);
    }
    if is_foundation_model(model_id) {
        return format!("{}.{}", prefix.as_str(), model_id);
    }
    model_id.to_string()
}
