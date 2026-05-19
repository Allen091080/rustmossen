//! 3P model capability overrides driven by env vars.
//!
//! Direct translation of `utils/model/modelSupportOverrides.ts`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::providers::{get_api_provider, APIProvider};

/// Capability flags that can be overridden for a pinned 3P model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelCapabilityOverride {
    Effort,
    MaxEffort,
    Thinking,
    AdaptiveThinking,
    InterleavedThinking,
}

impl ModelCapabilityOverride {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelCapabilityOverride::Effort => "effort",
            ModelCapabilityOverride::MaxEffort => "max_effort",
            ModelCapabilityOverride::Thinking => "thinking",
            ModelCapabilityOverride::AdaptiveThinking => "adaptive_thinking",
            ModelCapabilityOverride::InterleavedThinking => "interleaved_thinking",
        }
    }
}

struct Tier {
    model_env_var: &'static str,
    capabilities_env_var: &'static str,
}

const TIERS: &[Tier] = &[
    Tier {
        model_env_var: "MOSSEN_CODE_DEFAULT_OPUS_MODEL",
        capabilities_env_var: "MOSSEN_CODE_DEFAULT_OPUS_MODEL_SUPPORTED_CAPABILITIES",
    },
    Tier {
        model_env_var: "MOSSEN_CODE_DEFAULT_SONNET_MODEL",
        capabilities_env_var: "MOSSEN_CODE_DEFAULT_SONNET_MODEL_SUPPORTED_CAPABILITIES",
    },
    Tier {
        model_env_var: "MOSSEN_CODE_DEFAULT_HAIKU_MODEL",
        capabilities_env_var: "MOSSEN_CODE_DEFAULT_HAIKU_MODEL_SUPPORTED_CAPABILITIES",
    },
];

fn cache() -> &'static Mutex<HashMap<String, Option<bool>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<bool>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn compute(model: &str, capability: ModelCapabilityOverride) -> Option<bool> {
    if get_api_provider() == APIProvider::FirstParty {
        return None;
    }
    let lower = model.to_lowercase();
    for tier in TIERS {
        let pinned = std::env::var(tier.model_env_var).ok().filter(|s| !s.is_empty());
        let capabilities = std::env::var(tier.capabilities_env_var).ok();
        let (pinned, capabilities) = match (pinned, capabilities) {
            (Some(p), Some(c)) => (p, c),
            _ => continue,
        };
        if lower != pinned.to_lowercase() {
            continue;
        }
        let cap_str = capability.as_str();
        return Some(
            capabilities
                .to_lowercase()
                .split(',')
                .map(|s| s.trim())
                .any(|s| s == cap_str),
        );
    }
    None
}

/// `get3PModelCapabilityOverride` — memoized lookup keyed on
/// `${model.toLowerCase()}:${capability}`.
pub fn get_3p_model_capability_override(
    model: &str,
    capability: ModelCapabilityOverride,
) -> Option<bool> {
    let key = format!("{}:{}", model.to_lowercase(), capability.as_str());
    if let Some(cached) = cache().lock().unwrap().get(&key).copied() {
        return cached;
    }
    let result = compute(model, capability);
    cache().lock().unwrap().insert(key, result);
    result
}
