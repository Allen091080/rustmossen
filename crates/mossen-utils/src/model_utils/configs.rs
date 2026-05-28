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

pub static MOSSEN_3_7_BALANCED_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-7-balanced-20250219",
        "3-7-balanced",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250219"),
            variant: Some("v1:0"),
        },
        "3-7-balanced",
        ExternalVertexOptions {
            date: Some("20250219"),
            variant: None,
        },
        "3-7-balanced",
    )
});

pub static MOSSEN_3_5_V2_BALANCED_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-5-balanced-20241022",
        "3-5-balanced",
        ExternalBedrockOptions {
            region: None,
            date: Some("20241022"),
            variant: Some("v2:0"),
        },
        "3-5-balanced",
        ExternalVertexOptions {
            date: Some("20241022"),
            variant: Some("v2"),
        },
        "3-5-balanced",
    )
});

pub static MOSSEN_3_5_FAST_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-3-5-fast-20241022",
        "3-5-fast",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20241022"),
            variant: Some("v1:0"),
        },
        "3-5-fast",
        ExternalVertexOptions {
            date: Some("20241022"),
            variant: None,
        },
        "3-5-fast",
    )
});

pub static MOSSEN_FAST_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-fast-4-5-20251001",
        "fast-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20251001"),
            variant: Some("v1:0"),
        },
        "fast-4-5",
        ExternalVertexOptions {
            date: Some("20251001"),
            variant: None,
        },
        "fast-4-5",
    )
});

pub static MOSSEN_BALANCED_4_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-balanced-4-20250514",
        "balanced-4",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250514"),
            variant: Some("v1:0"),
        },
        "balanced-4",
        ExternalVertexOptions {
            date: Some("20250514"),
            variant: None,
        },
        "balanced-4",
    )
});

pub static MOSSEN_BALANCED_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-balanced-4-5-20250929",
        "balanced-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250929"),
            variant: Some("v1:0"),
        },
        "balanced-4-5",
        ExternalVertexOptions {
            date: Some("20250929"),
            variant: None,
        },
        "balanced-4-5",
    )
});

pub static MOSSEN_MAX_4_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-max-4-20250514",
        "max-4",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250514"),
            variant: Some("v1:0"),
        },
        "max-4",
        ExternalVertexOptions {
            date: Some("20250514"),
            variant: None,
        },
        "max-4",
    )
});

pub static MOSSEN_MAX_4_1_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-max-4-1-20250805",
        "max-4-1",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20250805"),
            variant: Some("v1:0"),
        },
        "max-4-1",
        ExternalVertexOptions {
            date: Some("20250805"),
            variant: None,
        },
        "max-4-1",
    )
});

pub static MOSSEN_MAX_4_5_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-max-4-5-20251101",
        "max-4-5",
        ExternalBedrockOptions {
            region: Some("us"),
            date: Some("20251101"),
            variant: Some("v1:0"),
        },
        "max-4-5",
        ExternalVertexOptions {
            date: Some("20251101"),
            variant: None,
        },
        "max-4-5",
    )
});

pub static MOSSEN_MAX_4_6_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-max-4-6",
        "max-4-6",
        ExternalBedrockOptions {
            region: Some("us"),
            date: None,
            variant: Some("v1"),
        },
        "max-4-6",
        ExternalVertexOptions {
            date: None,
            variant: None,
        },
        "max-4-6",
    )
});

pub static MOSSEN_BALANCED_4_6_CONFIG: Lazy<ModelConfig> = Lazy::new(|| {
    make_config(
        "mossen-balanced-4-6",
        "balanced-4-6",
        ExternalBedrockOptions {
            region: Some("us"),
            date: None,
            variant: None,
        },
        "balanced-4-6",
        ExternalVertexOptions {
            date: None,
            variant: None,
        },
        "balanced-4-6",
    )
});

/// Internal short key identifying a model. Matches TS `ModelKey =
/// keyof typeof ALL_MODEL_CONFIGS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelKey {
    Fast35,
    Fast45,
    Balanced35,
    Balanced37,
    Balanced40,
    Balanced45,
    Balanced46,
    Max40,
    Max41,
    Max45,
    Max46,
}

impl ModelKey {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKey::Fast35 => "fast35",
            ModelKey::Fast45 => "fast45",
            ModelKey::Balanced35 => "balanced35",
            ModelKey::Balanced37 => "balanced37",
            ModelKey::Balanced40 => "balanced40",
            ModelKey::Balanced45 => "balanced45",
            ModelKey::Balanced46 => "balanced46",
            ModelKey::Max40 => "max40",
            ModelKey::Max41 => "max41",
            ModelKey::Max45 => "max45",
            ModelKey::Max46 => "max46",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fast35" => Some(ModelKey::Fast35),
            "fast45" => Some(ModelKey::Fast45),
            "balanced35" => Some(ModelKey::Balanced35),
            "balanced37" => Some(ModelKey::Balanced37),
            "balanced40" => Some(ModelKey::Balanced40),
            "balanced45" => Some(ModelKey::Balanced45),
            "balanced46" => Some(ModelKey::Balanced46),
            "max40" => Some(ModelKey::Max40),
            "max41" => Some(ModelKey::Max41),
            "max45" => Some(ModelKey::Max45),
            "max46" => Some(ModelKey::Max46),
            _ => None,
        }
    }

    pub fn all() -> &'static [ModelKey] {
        &[
            ModelKey::Fast35,
            ModelKey::Fast45,
            ModelKey::Balanced35,
            ModelKey::Balanced37,
            ModelKey::Balanced40,
            ModelKey::Balanced45,
            ModelKey::Balanced46,
            ModelKey::Max40,
            ModelKey::Max41,
            ModelKey::Max45,
            ModelKey::Max46,
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
    m.insert(ModelKey::Fast35, &*MOSSEN_3_5_FAST_CONFIG);
    m.insert(ModelKey::Fast45, &*MOSSEN_FAST_4_5_CONFIG);
    m.insert(ModelKey::Balanced35, &*MOSSEN_3_5_V2_BALANCED_CONFIG);
    m.insert(ModelKey::Balanced37, &*MOSSEN_3_7_BALANCED_CONFIG);
    m.insert(ModelKey::Balanced40, &*MOSSEN_BALANCED_4_CONFIG);
    m.insert(ModelKey::Balanced45, &*MOSSEN_BALANCED_4_5_CONFIG);
    m.insert(ModelKey::Balanced46, &*MOSSEN_BALANCED_4_6_CONFIG);
    m.insert(ModelKey::Max40, &*MOSSEN_MAX_4_CONFIG);
    m.insert(ModelKey::Max41, &*MOSSEN_MAX_4_1_CONFIG);
    m.insert(ModelKey::Max45, &*MOSSEN_MAX_4_5_CONFIG);
    m.insert(ModelKey::Max46, &*MOSSEN_MAX_4_6_CONFIG);
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
