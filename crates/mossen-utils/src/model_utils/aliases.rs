//! Model alias definitions and helpers.
//!
//! Direct translation of `utils/model/aliases.ts`.

/// Canonical list of model aliases the user can pass to `--model` or via
/// `/model`. Strings here are the raw TS string literals; capability semantics
/// (e.g. the `[1m]` suffix) are handled by other modules.
pub const MODEL_ALIASES: &[&str] = &[
    "balanced",
    "max",
    "fast",
    "best",
    "balanced[1m]",
    "max[1m]",
    "maxplan",
];

/// Strongly-typed representation of a model alias. The TS source uses a string
/// literal union; here we reuse it as a Rust enum and provide round-trip
/// helpers.
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

impl ModelAlias {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelAlias::Balanced => "balanced",
            ModelAlias::Max => "max",
            ModelAlias::Fast => "fast",
            ModelAlias::Best => "best",
            ModelAlias::Balanced1M => "balanced[1m]",
            ModelAlias::Max1M => "max[1m]",
            ModelAlias::MaxPlan => "maxplan",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "balanced" => Some(ModelAlias::Balanced),
            "max" => Some(ModelAlias::Max),
            "fast" => Some(ModelAlias::Fast),
            "best" => Some(ModelAlias::Best),
            "balanced[1m]" => Some(ModelAlias::Balanced1M),
            "max[1m]" => Some(ModelAlias::Max1M),
            "maxplan" => Some(ModelAlias::MaxPlan),
            _ => None,
        }
    }
}

/// `isModelAlias` — case-sensitive membership test against [`MODEL_ALIASES`].
pub fn is_model_alias(model_input: &str) -> bool {
    MODEL_ALIASES.contains(&model_input)
}

/// Bare model family aliases that act as wildcards in the availableModels
/// allowlist. When "max" is in the allowlist, any max model is allowed;
/// a specific model ID in the allowlist allows only that exact version.
pub const MODEL_FAMILY_ALIASES: &[&str] = &["balanced", "max", "fast"];

/// `isModelFamilyAlias` — case-sensitive membership test.
pub fn is_model_family_alias(model: &str) -> bool {
    MODEL_FAMILY_ALIASES.contains(&model)
}
