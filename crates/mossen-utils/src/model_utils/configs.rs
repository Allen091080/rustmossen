//! Per-model provider config constants.
//!
//! Direct translation of `utils/model/configs.ts`. Each constant is the
//! `(firstParty, bedrock, vertex, foundry)` provider-ID tuple for one model.

use std::collections::HashMap;

use once_cell::sync::Lazy;

use super::external_provider_ids::{
    external_bedrock_model_id, external_foundry_model_id, external_vertex_model_id,
    ExternalBedrockOptions, ExternalVertexOptions,
};
use super::providers::APIProvider;

pub type ModelName = String;

/// `firstParty` is the Mossen-owned fixture ID. The other fields are raw
/// external provider model IDs and intentionally stay provider-shaped at the
/// adapter boundary.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub first_party: ModelName,
    pub bedrock: ModelName,
    pub vertex: ModelName,
    pub foundry: ModelName,
}

impl ModelConfig {
    /// Resolve the provider-specific model ID. Mirrors the TS access pattern
    /// `ALL_MODEL_CONFIGS[key][provider]`.
    pub fn for_provider(&self, provider: APIProvider) -> &str {
        match provider {
            APIProvider::FirstParty => &self.first_party,
            APIProvider::Bedrock => &self.bedrock,
            APIProvider::Vertex => &self.vertex,
            APIProvider::Foundry => &self.foundry,
        }
    }
}

fn make_config(
    first_party: &str,
    bedrock_tail: &str,
    bedrock_opts: ExternalBedrockOptions<'_>,
    vertex_tail: &str,
    vertex_opts: ExternalVertexOptions<'_>,
    foundry_tail: &str,
) -> ModelConfig {
    ModelConfig {
        first_party: first_party.to_string(),
        bedrock: external_bedrock_model_id(bedrock_tail, bedrock_opts),
        vertex: external_vertex_model_id(vertex_tail, vertex_opts),
        foundry: external_foundry_model_id(foundry_tail),
    }
}

pub static MOSSEN_3_7_SONNET_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-7-sonnet-20250219",
        "3-7-sonnet",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250219"),
            variant: Some("v1:0"),
        },
        "3-7-sonnet",
        ExternalVertexOptions {
            date: Some("20250219"),
            variant: None,
        },
        "3-7-sonnet",
    )
});

pub static MOSSEN_3_5_V2_SONNET_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-5-sonnet-20241022",
        "3-5-sonnet",
        ExternalBedrockOptions {
            region: None,
            date: Some("20241022"),
            variant: Some("v2:0"),
        },
        "3-5-sonnet",
        ExternalVertexOptions {
            date: Some("20241022"),
            variant: Some("v2"),
        },
        "3-5-sonnet",
    )
});

pub static MOSSEN_3_5_HAIKU_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-5-haiku-20241022",
        "3-5-haiku",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20241022"),
            variant: Some("v1:0"),
        },
        "3-5-haiku",
        ExternalVertexOptions {
            date: Some("20241022"),
            variant: None,
        },
        "3-5-haiku",
    )
});

pub static MOSSEN_HAIKU_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-haiku-4-5-20251001",
        "haiku-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20251001"),
            variant: Some("v1:0"),
        },
        "haiku-4-5",
        ExternalVertexOptions {
            date: Some("20251001"),
            variant: None,
        },
        "haiku-4-5",
    )
});

pub static MOSSEN_SONNET_4_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-sonnet-4-20250514",
        "sonnet-4",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250514"),
            variant: Some("v1:0"),
        },
        "sonnet-4",
        ExternalVertexOptions {
            date: Some("20250514"),
            variant: None,
        },
        "sonnet-4",
    )
});

pub static MOSSEN_SONNET_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-sonnet-4-5-20250929",
        "sonnet-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250929"),
            variant: Some("v1:0"),
        },
        "sonnet-4-5",
        ExternalVertexOptions {
            date: Some("20250929"),
            variant: None,
        },
        "sonnet-4-5",
    )
});

pub static MOSSEN_OPUS_4_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-opus-4-20250514",
        "opus-4",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250514"),
            variant: Some("v1:0"),
        },
        "opus-4",
        ExternalVertexOptions {
            date: Some("20250514"),
            variant: None,
        },
        "opus-4",
    )
});

pub static MOSSEN_OPUS_4_1_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-opus-4-1-20250805",
        "opus-4-1",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250805"),
            variant: Some("v1:0"),
        },
        "opus-4-1",
        ExternalVertexOptions {
            date: Some("20250805"),
            variant: None,
        },
        "opus-4-1",
    )
});

pub static MOSSEN_OPUS_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-opus-4-5-20251101",
        "opus-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20251101"),
            variant: Some("v1:0"),
        },
        "opus-4-5",
        ExternalVertexOptions {
            date: Some("20251101"),
            variant: None,
        },
        "opus-4-5",
    )
});

pub static MOSSEN_OPUS_4_6_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-opus-4-6",
        "opus-4-6",
        ExternalBedrockOptions {
            region: Some("us"),
            date: None,
            variant: Some("v1"),
        },
        "opus-4-6",
        ExternalVertexOptions {
            date: None,
            variant: None,
        },
        "opus-4-6",
    )
});

pub static MOSSEN_SONNET_4_6_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-sonnet-4-6",
        "sonnet-4-6",
        ExternalBedrockOptions {
            region: Some("us"),
            date: None,
            variant: None,
        },
        "sonnet-4-6",
        ExternalVertexOptions {
            date: None,
            variant: None,
        },
        "sonnet-4-6",
    )
});

/// Internal short key identifying a model. Matches TS `ModelKey =
/// keyof typeof ALL_MODEL_CONFIGS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKey {
    Haiku35,
    Haiku45,
    Sonnet35,
    Sonnet37,
    Sonnet40,
    Sonnet45,
    Sonnet46,
    Opus40,
    Opus41,
    Opus45,
    Opus46,
}

impl ModelKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKey::Haiku35 => "haiku35",
            ModelKey::Haiku45 => "haiku45",
            ModelKey::Sonnet35 => "sonnet35",
            ModelKey::Sonnet37 => "sonnet37",
            ModelKey::Sonnet40 => "sonnet40",
            ModelKey::Sonnet45 => "sonnet45",
            ModelKey::Sonnet46 => "sonnet46",
            ModelKey::Opus40 => "opus40",
            ModelKey::Opus41 => "opus41",
            ModelKey::Opus45 => "opus45",
            ModelKey::Opus46 => "opus46",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "haiku35" => Some(ModelKey::Haiku35),
            "haiku45" => Some(ModelKey::Haiku45),
            "sonnet35" => Some(ModelKey::Sonnet35),
            "sonnet37" => Some(ModelKey::Sonnet37),
            "sonnet40" => Some(ModelKey::Sonnet40),
            "sonnet45" => Some(ModelKey::Sonnet45),
            "sonnet46" => Some(ModelKey::Sonnet46),
            "opus40" => Some(ModelKey::Opus40),
            "opus41" => Some(ModelKey::Opus41),
            "opus45" => Some(ModelKey::Opus45),
            "opus46" => Some(ModelKey::Opus46),
            _ => None,
        }
    }

    pub fn all() -> &'static [ModelKey] {
        &[
            ModelKey::Haiku35,
            ModelKey::Haiku45,
            ModelKey::Sonnet35,
            ModelKey::Sonnet37,
            ModelKey::Sonnet40,
            ModelKey::Sonnet45,
            ModelKey::Sonnet46,
            ModelKey::Opus40,
            ModelKey::Opus41,
            ModelKey::Opus45,
            ModelKey::Opus46,
        ]
    }
}

/// Equivalent to TS `CanonicalModelId` — type alias of `ModelName` since Rust
/// can't express the literal union directly.
pub type CanonicalModelId = ModelName;

/// `ALL_MODEL_CONFIGS` — registry from internal short key to per-provider model
/// strings. Mirrors the TS object.
pub static ALL_MODEL_CONFIGS: Lazy<HashMap<ModelKey, &'static ModelConfig>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(ModelKey::Haiku35, &*MOSSEN_3_5_HAIKU_CONFIG);
    m.insert(ModelKey::Haiku45, &*MOSSEN_HAIKU_4_5_CONFIG);
    m.insert(ModelKey::Sonnet35, &*MOSSEN_3_5_V2_SONNET_CONFIG);
    m.insert(ModelKey::Sonnet37, &*MOSSEN_3_7_SONNET_CONFIG);
    m.insert(ModelKey::Sonnet40, &*MOSSEN_SONNET_4_CONFIG);
    m.insert(ModelKey::Sonnet45, &*MOSSEN_SONNET_4_5_CONFIG);
    m.insert(ModelKey::Sonnet46, &*MOSSEN_SONNET_4_6_CONFIG);
    m.insert(ModelKey::Opus40, &*MOSSEN_OPUS_4_CONFIG);
    m.insert(ModelKey::Opus41, &*MOSSEN_OPUS_4_1_CONFIG);
    m.insert(ModelKey::Opus45, &*MOSSEN_OPUS_4_5_CONFIG);
    m.insert(ModelKey::Opus46, &*MOSSEN_OPUS_4_6_CONFIG);
    m
});

/// Iterator-friendly snapshot. The TS code uses `Object.values(...)`; this
/// gives the same flat list of configs.
pub fn all_configs() -> Vec<&'static ModelConfig> {
    ModelKey::all()
        .iter()
        .map(|k| *ALL_MODEL_CONFIGS.get(k).expect("registered model key"))
        .collect()
}

/// Runtime list of canonical model IDs. Used by comprehensiveness tests.
pub static CANONICAL_MODEL_IDS: Lazy<Vec<CanonicalModelId>> = Lazy::new(|| {
    ModelKey::all()
        .iter()
        .map(|k| ALL_MODEL_CONFIGS[k].first_party.clone())
        .collect()
});

/// Map canonical first-party model ID → internal short key. Used to apply
/// settings-based modelOverrides.
pub static CANONICAL_ID_TO_KEY: Lazy<HashMap<CanonicalModelId, ModelKey>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for key in ModelKey::all() {
        m.insert(ALL_MODEL_CONFIGS[key].first_party.clone(), *key);
    }
    m
});
